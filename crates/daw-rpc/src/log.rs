//! The error log for the interface: writing from the browser side and reading the latest entries.

use schemars::JsonSchema;
use std::path::PathBuf;

use daw_contracts::{LogEntry, LogLevel};
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct Write {
    level: LogLevel,
    source: String,
    message: String,
    #[serde(default)]
    details: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct Recent {
    limit: usize,
}

pub(crate) fn register(table: &mut RpcTable) {
    // Writing to the log does not log its own failures: a log failure must not produce new entries.
    table.add("log.write", "Writes a message to the log.", |c, p: Write| {
        Ok(c.log.record(LogEntry { details: p.details, ..LogEntry::new(p.level, &p.source, p.message) }))
    });
    table.add("log.recent", "Returns the latest error log entries.", |c, p: Recent| Ok(c.log.recent(p.limit.min(1_000))));
    table.add("log.file", "Returns the path to the error log file.", |c, ()| Ok(c.log.file().map(|p: PathBuf| p.display().to_string())));
}
