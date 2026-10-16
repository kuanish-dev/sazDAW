//! Shared helpers for the RPC integration tests.
#![allow(dead_code, reason = "each test module takes only the helpers it needs")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "test helpers: a failed setup should stop the test loudly, and skips say why"
)]

pub use daw_bootstrap::build_session;
pub use daw_contracts::codes;
pub use daw_rpc::RpcTable;
pub use serde_json::{Value, json};

pub fn call(table: &RpcTable, controls: &daw_contracts::Controls, method: &str, params: Value) -> Value {
    table.call(controls, method, params).unwrap_or_else(|e| panic!("{method} → {e}"))
}

pub const NOTE: fn(u32, u32) -> Value = |pitch, start| json!({ "pitch": pitch, "startTick": start, "lengthTicks": 240, "velocity": 100 });

pub fn add_synth(table: &RpcTable, c: &daw_contracts::Controls) -> Value {
    call(table, c, "tracks.add", json!({ "name": "Synth", "kind": "synth", "defaultPitch": 60 }))
}

pub fn add_clip(table: &RpcTable, c: &daw_contracts::Controls, track: &Value) -> Value {
    call(table, c, "clips.addNote", json!({ "track": track, "startTick": 0, "lengthTicks": null }))
}

pub fn project(table: &RpcTable, c: &daw_contracts::Controls) -> Value {
    call(table, c, "project.get", Value::Null)
}
