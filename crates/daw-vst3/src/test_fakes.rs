//! Fakes for the crate's unit tests: an in-memory log, project params and edit history.

use std::path::PathBuf;
use std::sync::Mutex;

use daw_contracts::{AppLog, ContractError, HistoryControl, HistoryMark, HistorySteps, LogEntry, Params};

use crate::param_bridge::ProjectParams;

#[derive(Default)]
pub struct Log(pub Mutex<Vec<LogEntry>>);

impl AppLog for Log {
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

#[derive(Default)]
pub struct Project {
    pub values: Mutex<Params>,
    pub writes: Mutex<Vec<(String, f32)>>,
    pub gone: Mutex<bool>,
}

impl ProjectParams for Project {
    fn read(&self) -> Result<Params, ContractError> {
        if *self.gone.lock().unwrap() {
            Err(daw_contracts::codes::graph::NODE_NOT_FOUND.with("id", 1))
        } else {
            Ok(self.values.lock().unwrap().clone())
        }
    }
    fn write(&self, name: &str, value: f32) -> Result<(), ContractError> {
        self.values.lock().unwrap().insert(name.to_string(), value);
        self.writes.lock().unwrap().push((name.to_string(), value));
        Ok(())
    }
}

#[derive(Default)]
pub struct History(pub Mutex<Vec<String>>);

impl HistoryControl for History {
    fn undo(&self) -> bool {
        false
    }
    fn redo(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo_label(&self) -> Option<String> {
        None
    }
    fn steps(&self) -> HistorySteps {
        HistorySteps::default()
    }
    fn begin_group(&self, label: &str) {
        self.0.lock().unwrap().push(format!("begin {label}"));
    }
    fn end_group(&self) -> bool {
        self.0.lock().unwrap().push("end".into());
        true
    }
    fn cancel_group(&self) {
        self.0.lock().unwrap().push("cancel".into());
    }
    fn mark_baseline(&self) {}
    fn checkpoint(&self) -> HistoryMark {
        0
    }
    fn squash(&self, _: HistoryMark, _: &str) -> Result<bool, ContractError> {
        Ok(false)
    }
}

/// A fake scan helper: a shell script with body `body` (`$1` is the bundle, `$2` the answer file).
pub fn fake_helper(name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("daw-fake-helper-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}
