//! Watching the audio engine from the control side (the `daw-engine-watch` thread, ~20 times a second): frees what the
//! engine has let go (`DspSink::collect_garbage`) and logs every growth of losses (`EngineStats::losses`).
//! Without it the release queue would overflow during long playback without edits, and losses would go unnoticed.
//! The thread lives as long as the engine: it holds it only through weak references.

use std::sync::{Arc, Weak};
use std::thread;
use std::time::Duration;

use daw_contracts::{AppLog, DspSink, EngineLosses, EngineStats, LogEntry, LogLevel};

const INTERVAL: Duration = Duration::from_millis(50);

pub(crate) fn spawn(dsp: Weak<dyn DspSink>, stats: Weak<dyn EngineStats>, log: Arc<dyn AppLog>) {
    let worker_log = Arc::clone(&log);
    let spawned = thread::Builder::new().name("daw-engine-watch".into()).spawn(move || {
        let mut seen = EngineLosses::default();
        while let (Some(dsp), Some(stats)) = (dsp.upgrade(), stats.upgrade()) {
            dsp.collect_garbage();
            let now = stats.losses();
            for message in loss_messages(seen, now) {
                worker_log.record(LogEntry::new(LogLevel::Error, "engine", message));
            }
            seen = now;
            drop((dsp, stats));
            thread::sleep(INTERVAL);
        }
    });
    if let Err(error) = spawned {
        let message = format!("could not start the engine watch: sound memory will not be freed ({error})");
        log.record(LogEntry::new(LogLevel::Error, "engine", message));
    }
}

/// A loss counter before and after, and the log text for the growth.
type Loss = (u64, u64, fn(u64) -> String);

/// Log entries about what was lost since the previous check.
fn loss_messages(before: EngineLosses, now: EngineLosses) -> Vec<String> {
    let losses: [Loss; 5] = [
        (before.live_notes, now.live_notes, |n| format!("live notes the engine could not take: {n} (the queue is full)")),
        (before.recorded_events, now.recorded_events, |n| format!("notes lost while recording: {n} (the recording queue is full)")),
        (before.leaked_garbage, now.leaked_garbage, |n| format!("objects the engine left in memory: {n} (the release queue is full)")),
        (before.monitor_underruns, now.monitor_underruns, |n| {
            format!(
                "input monitoring ran out of sound (times: {n}): the input lags behind the output, so monitoring clicks; recording is not affected"
            )
        }),
        (before.latency_clamped, now.latency_clamped, |n| {
            format!(
                "plugin latency compensation was cut (times: {n}): a path delay exceeds the engine limit or delay lines ran out, so paths are not aligned"
            )
        }),
    ];
    losses.into_iter().filter(|(old, new, _)| new > old).map(|(old, new, message)| message(new - old)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_growth_is_reported() {
        let before = EngineLosses { live_notes: 2, recorded_events: 0, leaked_garbage: 1, latency_clamped: 0, monitor_underruns: 0 };
        assert!(loss_messages(before, before).is_empty());
        let now = EngineLosses { live_notes: 5, recorded_events: 4, leaked_garbage: 1, latency_clamped: 2, monitor_underruns: 0 };
        let messages = loss_messages(before, now);
        assert_eq!(messages.len(), 3);
        assert!(messages[0].contains(": 3") && messages[1].contains(": 4") && messages[2].contains(": 2"), "{messages:?}");
    }
}
