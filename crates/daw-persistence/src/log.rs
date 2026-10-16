//! The error log in a file: one entry is one JSON line (`errors.log`); the file rotates as it grows.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use daw_contracts::{AppLog, LogEntry};

const FILE_NAME: &str = "errors.log";
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const KEEP_OLD: usize = 3;
const MAX_FIELD_CHARS: usize = 8_000;

pub struct FileLog {
    dir: PathBuf,
    /// The lock guards writing and rotation against concurrent calls from different threads.
    lock: Mutex<()>,
}

impl FileLog {
    pub fn open(dir: &Path) -> Self {
        // If the folder is not created, writing in `record` fails too and says so on stderr: the log has nowhere else to write.
        let _ = fs::create_dir_all(dir);
        Self { dir: dir.to_path_buf(), lock: Mutex::new(()) }
    }

    fn path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    fn rotated(&self, index: usize) -> PathBuf {
        self.dir.join(format!("errors.{index}.log"))
    }

    /// Shifts `errors.log` → `errors.1.log` → … and drops the oldest.
    fn rotate(&self) {
        let _ = fs::remove_file(self.rotated(KEEP_OLD));
        for index in (1..KEEP_OLD).rev() {
            let _ = fs::rename(self.rotated(index), self.rotated(index + 1));
        }
        let _ = fs::rename(self.path(), self.rotated(1));
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn clip(text: &str) -> String {
    if text.chars().count() <= MAX_FIELD_CHARS {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(MAX_FIELD_CHARS).collect();
    cut.push('…');
    cut
}

impl AppLog for FileLog {
    #[allow(clippy::print_stderr, reason = "a failure of the log itself: there is nowhere to report it but stderr")]
    fn record(&self, mut entry: LogEntry) {
        if entry.time_ms == 0 {
            entry.time_ms = now_ms();
        }
        entry.message = clip(&entry.message);
        entry.details = entry.details.map(|d| clip(&d));
        let Ok(line) = serde_json::to_string(&entry) else { return };
        let _guard = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if fs::metadata(self.path()).is_ok_and(|m| m.len() > MAX_BYTES) {
            self.rotate();
        }
        // The log must not bring the app down: a write error is only shown on the console.
        let written = OpenOptions::new().create(true).append(true).open(self.path()).and_then(|mut file: File| writeln!(file, "{line}"));
        if let Err(error) = written {
            eprintln!("could not write the error log: {error}");
        }
    }

    fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let _guard = self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut files: Vec<PathBuf> = (1..=KEEP_OLD).rev().map(|i| self.rotated(i)).collect();
        files.push(self.path());
        let mut entries: Vec<LogEntry> = files
            .iter()
            .filter_map(|path| File::open(path).ok())
            .flat_map(|file| BufReader::new(file).lines().map_while(Result::ok).collect::<Vec<_>>())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect();
        let skip = entries.len().saturating_sub(limit);
        entries.drain(..skip);
        entries
    }

    fn file(&self) -> Option<PathBuf> {
        Some(self.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;

    fn entry(message: &str) -> LogEntry {
        LogEntry::error("test", message)
    }

    #[test]
    fn records_are_saved_in_order_with_time_and_survive_reopening() {
        let dir = temp_dir("log-order");
        let log = FileLog::open(&dir);
        log.record(entry("first"));
        log.record(entry("second").with_details("stack"));
        let reopened = FileLog::open(&dir);
        let all = reopened.recent(10);
        assert_eq!(all.iter().map(|e| e.message.as_str()).collect::<Vec<_>>(), ["first", "second"]);
        assert!(all[0].time_ms > 0, "the log sets the time");
        assert_eq!(all[1].details.as_deref(), Some("stack"));
        assert_eq!(reopened.recent(1).len(), 1, "the limit takes the newest");
        assert_eq!(reopened.recent(1)[0].message, "second");
        assert!(reopened.file().unwrap().is_file());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn long_fields_are_clipped_and_garbage_lines_are_ignored() {
        let dir = temp_dir("log-clip");
        let log = FileLog::open(&dir);
        // A multi-byte character: clipping counts characters, not bytes.
        log.record(entry(&"я".repeat(MAX_FIELD_CHARS * 2)));
        fs::write(dir.join(FILE_NAME), format!("not json\n{}\n", fs::read_to_string(dir.join(FILE_NAME)).unwrap())).unwrap();
        let all = log.recent(10);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].message.chars().count(), MAX_FIELD_CHARS + 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_big_log_is_rotated_and_old_files_are_kept_up_to_the_limit() {
        let dir = temp_dir("log-rotate");
        let log = FileLog::open(&dir);
        let big = "x".repeat(MAX_FIELD_CHARS);
        for _ in 0..(MAX_BYTES as usize / MAX_FIELD_CHARS + 50) {
            log.record(entry(&big));
        }
        assert!(dir.join("errors.1.log").is_file(), "the old file is kept");
        log.record(entry("after rotation"));
        assert_eq!(log.recent(1)[0].message, "after rotation");
        fs::remove_dir_all(dir).ok();
    }
}
