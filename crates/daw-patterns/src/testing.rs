//! Test doubles of the crate.

use std::path::PathBuf;

use daw_contracts::{AppLog, LogEntry};

/// A log nobody should write to: any entry fails the test (the built-in pattern table must be valid).
pub(crate) struct NoLog;

impl AppLog for NoLog {
    fn record(&self, entry: LogEntry) {
        panic!("unexpected log entry: {entry:?}");
    }
    fn recent(&self, _: usize) -> Vec<LogEntry> {
        Vec::new()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}
