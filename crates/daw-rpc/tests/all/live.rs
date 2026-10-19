//! The live side: telemetry, MIDI and live notes, note recording.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn the_tick_reports_position_and_meters() {
    let session = build_session(true);
    let tick = daw_rpc::tick(&session.controls);
    assert_eq!(tick["state"], "stopped");
    assert!(tick["signature"]["denominator"].is_number());
    assert_eq!(tick["latencyMs"], 0.0, "without plugin latency there is no compensation");
    let meters = tick["meters"].as_array().unwrap();
    for meter in meters {
        assert!(meter["node"].is_number() && meter["left"].is_number() && meter["right"].is_number());
        assert!(meter.get("track").is_none(), "the key is called node");
    }
}

#[test]
fn midi_and_live_methods_take_clip_and_track_params() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track);

    // Param parsing passes: errors come from the implementation now, not from parsing (no file/device).
    let not_params = |r: Result<Value, daw_contracts::ContractError>| !r.is_err_and(|e| e.is(codes::rpc::BAD_PARAMS));
    assert!(not_params(table.call(c, "midi.exportClip", json!({ "clip": clip["id"], "path": "/nonexistent-dir/x.mid" }))));
    assert!(not_params(table.call(c, "midi.importInto", json!({ "clip": clip["id"], "path": "/nonexistent/x.mid" }))));
    assert!(not_params(table.call(c, "midi.exportProject", json!({ "path": "/nonexistent-dir/p.mid" }))));
    assert!(not_params(table.call(c, "midi.connect", json!({ "device": "no such device", "target": track }))));
    assert!(not_params(table.call(c, "midi.setTarget", json!({ "target": track }))));
    assert!(not_params(table.call(c, "live.noteOn", json!({ "track": track, "pitch": 60, "velocity": 0.8 }))));
    assert!(not_params(table.call(c, "live.noteOff", json!({ "track": track, "pitch": 60 }))));

    // Old param names are rejected by parsing.
    let old = table.call(c, "live.noteOn", json!({ "channel": track, "pitch": 60, "velocity": 0.8 })).unwrap_err();
    assert!(old.is(codes::rpc::BAD_PARAMS), "{old}");
    let old = table.call(c, "midi.importInto", json!({ "pattern": 1, "channel": 1, "path": "x" })).unwrap_err();
    assert!(old.is(codes::rpc::BAD_PARAMS), "{old}");
}

#[test]
fn record_state_is_exposed_and_controllable() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    let idle = daw_rpc::tick(c)["record"].clone();
    assert_eq!(idle["armed"], false);
    assert_eq!(idle["capturing"], false);
    assert_eq!(idle["quantizeTicks"], 0, "recording is not snapped by default");

    call(&table, c, "record.setArmed", json!({ "armed": true }));
    call(&table, c, "record.setQuantize", json!({ "ticks": 480 }));
    let armed = daw_rpc::tick(c)["record"].clone();
    assert_eq!(armed["armed"], true);
    assert_eq!(armed["capturing"], false, "the transport is stopped");
    assert_eq!(armed["quantizeTicks"], 480);
    assert!(table.call(c, "record.setQuantize", json!({ "ticks": 999_999 })).is_err());
    call(&table, c, "record.setArmed", json!({ "armed": false }));
}
