//! The app's error log: all failures (backend, interface, panics) go to one file so they can be investigated later.

use schemars::JsonSchema;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Unix time in milliseconds; 0 means the log fills it in.
    pub time_ms: u64,
    pub level: LogLevel,
    /// Where the failure came from: `frontend`, `rpc`, `panic`, `startup`, `audio`…
    pub source: String,
    pub message: String,
    /// Details: stack trace, call params and so on.
    pub details: Option<String>,
}

impl LogEntry {
    /// An entry without a time (the log fills it in) and without details.
    pub fn new(level: LogLevel, source: &str, message: impl Into<String>) -> Self {
        Self { time_ms: 0, level, source: source.to_owned(), message: message.into(), details: None }
    }

    pub fn warn(source: &str, message: impl Into<String>) -> Self {
        Self::new(LogLevel::Warn, source, message)
    }

    pub fn error(source: &str, message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, source, message)
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Error log. Job: accept an entry and store it reliably; a failure of the log itself does not crash the app.
pub trait AppLog: Send + Sync {
    fn record(&self, entry: LogEntry);
    /// The latest entries (newest last).
    fn recent(&self, limit: usize) -> Vec<LogEntry>;
    /// The log file, if there is one.
    fn file(&self) -> Option<PathBuf>;
}
