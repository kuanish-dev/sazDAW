//! Scanning in a separate process: "scan one bundle so that its crash or hang does not affect the app".
//!
//! Both sides of one protocol:
//! * the app (`IsolatedScan::scan`) runs the helper `daw-plugin-scan <bundle> <answer file>` and waits no longer than
//!   `SCAN_TIMEOUT`; a signal, an error code or a timeout becomes a `ScanFailure` with a clear reason, the scan goes on;
//! * the helper (`helper_main`, binary `src/bin/daw_plugin_scan.rs`) loads the bundle with the regular `Vst3Scanner::scan_bundle`
//!   and writes the result as JSON to the answer file: what was found, refusals and its own log entries (it has no access to the app log).
//!
//! The answer is in a file, not stdout: plugins often print to stdout while loading and would spoil the JSON; the helper's
//! output is discarded entirely.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use daw_contracts::sync::lock;
use daw_contracts::{AppLog, LogEntry, ScanFailure};
use serde::{Deserialize, Serialize};

use crate::journal;
use crate::scanner::{ScanOutcome, Vst3Scanner};

/// Helper name: in the built app it lies next to the executable (`SazDAW.app/Contents/MacOS`).
pub const HELPER_NAME: &str = "daw-plugin-scan";
/// How long to wait for one bundle: an honest plugin loads in seconds, longer means it hung.
pub const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(20);

#[derive(Serialize, Deserialize)]
struct HelperReport {
    outcome: ScanOutcome,
    /// What the helper would have written to the log: the app moves it to its own.
    journal: Vec<LogEntry>,
}

pub struct IsolatedScan {
    helper: PathBuf,
    timeout: Duration,
}

impl IsolatedScan {
    pub fn new(helper: PathBuf, timeout: Duration) -> Self {
        Self { helper, timeout }
    }

    pub fn helper(&self) -> &Path {
        &self.helper
    }

    /// The result for one bundle. A helper failure is `failed` with a reason (and a log entry); the helper's log entries
    /// are moved to `log` (among them refusals of the plugins themselves).
    pub fn scan(&self, bundle: &Path, log: &dyn AppLog) -> ScanOutcome {
        match self.run(bundle) {
            Ok(report) => {
                for entry in report.journal {
                    log.record(entry);
                }
                report.outcome
            }
            Err(reason) => {
                journal::warn(log, format!("VST3 {}: {reason}", bundle.display()));
                ScanOutcome { plugins: Vec::new(), failed: vec![ScanFailure { path: bundle.display().to_string(), reason }] }
            }
        }
    }

    fn run(&self, bundle: &Path) -> Result<HelperReport, String> {
        let answer = answer_file();
        let started = Command::new(&self.helper)
            .arg(bundle)
            .arg(&answer)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("could not start the scan helper {}: {error}", self.helper.display()));
        let result = started.and_then(|mut child| {
            let status = self.wait(&mut child)?;
            if !status.success() {
                return Err(failure_reason(status));
            }
            let text = std::fs::read_to_string(&answer).map_err(|error| format!("the scan helper left no answer: {error}"))?;
            serde_json::from_str(&text).map_err(|error| format!("unreadable answer from the scan helper: {error}"))
        });
        std::fs::remove_file(&answer).ok();
        result
    }

    /// Waits for the helper no longer than `timeout`; a hung one is killed.
    fn wait(&self, child: &mut std::process::Child) -> Result<ExitStatus, String> {
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) if Instant::now() >= deadline => {
                    child.kill().ok();
                    child.wait().ok();
                    return Err(format!("the plugin did not answer within {} s (hung while loading)", self.timeout.as_secs()));
                }
                Ok(None) => std::thread::sleep(POLL),
                Err(error) => return Err(format!("could not wait for the scan helper: {error}")),
            }
        }
    }
}

/// A fresh answer file per run: scans in different sessions of one process (tests) do not get in each other's way.
fn answer_file() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{HELPER_NAME}-{}-{number}.json", std::process::id()))
}

fn failure_reason(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.signal(), status.code()) {
        (Some(signal), _) => format!("the plugin crashed while loading (signal {signal}{})", signal_name(signal)),
        (None, Some(code)) => format!("the scan helper exited with code {code}"),
        (None, None) => "the scan helper exited in an unknown way".into(),
    }
}

fn signal_name(signal: i32) -> &'static str {
    match signal {
        4 => ", SIGILL",
        5 => ", SIGTRAP",
        6 => ", SIGABRT",
        8 => ", SIGFPE",
        9 => ", SIGKILL",
        10 => ", SIGBUS",
        11 => ", SIGSEGV",
        _ => "",
    }
}

/// The helper's log: collects entries to hand them to the app in the answer.
#[derive(Default)]
struct Collected(Mutex<Vec<LogEntry>>);

impl AppLog for Collected {
    fn record(&self, entry: LogEntry) {
        lock(&self.0).push(entry);
    }
    fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let entries = lock(&self.0);
        entries[entries.len().saturating_sub(limit)..].to_vec()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}

/// Helper side: `daw-plugin-scan <bundle.vst3> <answer file>`. Code 0 — the answer is written; 2 — wrong arguments;
/// 3 — the answer was not written. If the plugin crashes the process there is no code at all — which is what the app sees.
pub fn helper_main(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let (Some(bundle), Some(answer), None) = (args.next(), args.next(), args.next()) else {
        return ExitCode::from(2);
    };
    let log = Arc::new(Collected::default());
    let outcome = Vst3Scanner::new(Vec::new(), log.clone()).scan_bundle(Path::new(&bundle));
    let journal = std::mem::take(&mut *lock(&log.0));
    let written = serde_json::to_string(&HelperReport { outcome, journal })
        .map_err(|error| error.to_string())
        .and_then(|text| std::fs::write(&answer, text).map_err(|error| error.to_string()));
    if written.is_ok() { ExitCode::SUCCESS } else { ExitCode::from(3) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fakes::{Log, fake_helper};

    fn scan_with(body: &str, name: &str, timeout: Duration) -> (ScanOutcome, Arc<Log>) {
        let log = Arc::new(Log::default());
        let outcome = IsolatedScan::new(fake_helper(name, body), timeout).scan(Path::new("/plugins/Fuzz.vst3"), &*log);
        (outcome, log)
    }

    #[test]
    fn a_crashing_plugin_becomes_a_failure_with_the_signal() {
        let (outcome, log) = scan_with("kill -SEGV $$", "crash", SCAN_TIMEOUT);
        assert!(outcome.plugins.is_empty());
        let reason = &outcome.failed[0].reason;
        assert!(reason.contains("crashed while loading") && reason.contains("signal 11, SIGSEGV"), "{reason}");
        assert_eq!(outcome.failed[0].path, "/plugins/Fuzz.vst3");
        assert_eq!(log.recent(10).len(), 1, "logged as well");
    }

    #[test]
    fn a_hanging_plugin_is_killed_after_the_timeout() {
        let started = Instant::now();
        let (outcome, _) = scan_with("sleep 30", "hang", Duration::from_millis(300));
        assert!(started.elapsed() < Duration::from_secs(10), "did not wait longer than allowed");
        assert!(outcome.failed[0].reason.contains("did not answer"), "{:?}", outcome.failed);
    }

    #[test]
    fn exit_codes_and_garbage_answers_are_failures_too() {
        let (outcome, _) = scan_with("exit 3", "code", SCAN_TIMEOUT);
        assert!(outcome.failed[0].reason.contains("code 3"), "{:?}", outcome.failed);
        let (outcome, _) = scan_with("echo garbage > \"$2\"", "garbage", SCAN_TIMEOUT);
        assert!(outcome.failed[0].reason.contains("unreadable answer"), "{:?}", outcome.failed);
        let log = Arc::new(Log::default());
        let outcome = IsolatedScan::new("/no/such/helper".into(), SCAN_TIMEOUT).scan(Path::new("/x.vst3"), &*log);
        assert!(outcome.failed[0].reason.contains("could not start"), "{:?}", outcome.failed);
    }

    #[test]
    fn the_helper_answer_and_its_journal_reach_the_app() {
        let answer = r#"{"outcome":{"plugins":[],"failed":[{"path":"/x.vst3","reason":"no factory"}]},
                "journal":[{"timeMs":0,"level":"warn","source":"vst3","message":"VST3 /x.vst3: no factory","details":null}]}"#;
        let (outcome, log) = scan_with(&format!("cat > \"$2\" <<'EOF'\n{answer}\nEOF"), "answer", SCAN_TIMEOUT);
        assert_eq!(outcome.failed, [ScanFailure { path: "/x.vst3".into(), reason: "no factory".into() }]);
        assert_eq!(log.recent(10)[0].message, "VST3 /x.vst3: no factory");
    }

    #[test]
    fn helper_side_rejects_wrong_arguments_and_answers_for_a_missing_bundle() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>().into_iter();
        assert_eq!(helper_main(args(&["one"])), ExitCode::from(2));
        let answer = std::env::temp_dir().join(format!("daw-helper-answer-{}.json", std::process::id()));
        assert_eq!(helper_main(args(&["/no/such/Fuzz.vst3", answer.to_str().unwrap()])), ExitCode::SUCCESS);
        let report: HelperReport = serde_json::from_str(&std::fs::read_to_string(&answer).unwrap()).unwrap();
        std::fs::remove_file(answer).ok();
        assert_eq!(report.outcome.failed.len(), 1, "a bundle that did not load is a refusal in the answer");
        assert_eq!(report.journal.len(), 1, "and a log entry for the app");
    }
}
