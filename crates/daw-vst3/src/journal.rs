//! Writes VST3 host failures to the app log (`AppLog`, source "vst3").

use daw_contracts::{AppLog, LogEntry, LogLevel};

const SOURCE: &str = "vst3";

pub fn warn(log: &dyn AppLog, message: String) {
    log.record(LogEntry::new(LogLevel::Warn, SOURCE, message));
}

pub fn error(log: &dyn AppLog, message: String) {
    log.record(LogEntry::new(LogLevel::Error, SOURCE, message));
}
