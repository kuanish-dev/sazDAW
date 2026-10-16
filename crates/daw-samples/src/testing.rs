//! Test doubles of the crate.

use std::path::PathBuf;
use std::sync::Mutex;

use daw_contracts::{AppLog, LogEntry};

/// An in-memory log: tests check that a failure did not pass silently.
#[derive(Default)]
pub(crate) struct MemoryLog(Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn recent(&self, _: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}
