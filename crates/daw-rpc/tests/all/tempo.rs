//! `tempo.*` methods: the tempo map over the wire — JSON shape, undo, refusing an invalid map, the tempo in `tick`; the transport
//! cue and the song loop.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn tempo_changes_go_over_the_wire_and_are_undone() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let changes = json!([{ "tick": 0, "bpm": 124 }, { "tick": 241_920, "bpm": 128 }, { "tick": 303_360, "bpm": 140, "ramp": true }]);
    call(&table, c, "tempo.setChanges", json!({ "changes": changes }));
    let map = call(&table, c, "tempo.get", Value::Null);
    assert_eq!(
        map["tempos"],
        json!([
            { "tick": 0, "bpm": 124.0, "ramp": false },
            { "tick": 241_920, "bpm": 128.0, "ramp": false },
            { "tick": 303_360, "bpm": 140.0, "ramp": true }
        ])
    );
    assert_eq!(project(&table, c)["tempoMap"], map, "project.get carries the same map");

    call(&table, c, "history.undo", Value::Null);
    assert_eq!(call(&table, c, "tempo.get", Value::Null)["tempos"].as_array().unwrap().len(), 1);
}

#[test]
fn signature_changes_are_checked_on_the_bar() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let seven_eight = json!([
        { "tick": 0, "signature": { "numerator": 4, "denominator": 4 } },
        { "tick": 61_440, "signature": { "numerator": 7, "denominator": 8 } },
        { "tick": 68_160, "signature": { "numerator": 4, "denominator": 4 } }
    ]);
    call(&table, c, "tempo.setSignatureChanges", json!({ "changes": seven_eight }));
    assert_eq!(call(&table, c, "tempo.get", Value::Null)["signatures"], seven_eight);

    let mid_bar = json!([{ "tick": 0, "signature": { "numerator": 4, "denominator": 4 } }, { "tick": 100, "signature": { "numerator": 3, "denominator": 4 } }]);
    let error = table.call(c, "tempo.setSignatureChanges", json!({ "changes": mid_bar })).unwrap_err();
    assert!(error.is(codes::tempo::SIGNATURE_OFF_BAR), "{error}");
    let error = table.call(c, "tempo.setChanges", json!({ "changes": [{ "tick": 960, "bpm": 120 }] })).unwrap_err();
    assert!(error.is(codes::tempo::TEMPO_MAP_FIRST_NOT_AT_ZERO), "{error}");
}

#[test]
fn the_tick_reports_tempo_and_signature_at_the_playhead() {
    let mut session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    call(&table, c, "tempo.setChanges", json!({ "changes": [{ "tick": 0, "bpm": 100 }, { "tick": 3840, "bpm": 150 }] }));
    call(
        &table,
        c,
        "tempo.setSignatureChanges",
        json!({ "changes": [
        { "tick": 0, "signature": { "numerator": 4, "denominator": 4 } },
        { "tick": 3840, "signature": { "numerator": 6, "denominator": 8 } }
    ] }),
    );
    assert_eq!(daw_rpc::tick(c)["tempo"], 100.0);
    call(&table, c, "transport.seek", json!({ "tick": 4000 }));
    session.renderer.render(&mut [0.0; 128]);
    let tick = daw_rpc::tick(&session.controls);
    assert_eq!(tick["tempo"], 150.0);
    assert_eq!(tick["signature"], json!({ "numerator": 6, "denominator": 8 }));
}

#[test]
fn the_cue_is_set_over_rpc_and_shown_in_the_telemetry() {
    let mut session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert_eq!(daw_rpc::tick(c)["cue"], Value::Null);
    call(&table, c, "transport.setCue", json!({ "tick": 3840 }));
    assert_eq!(daw_rpc::tick(c)["cue"], 3840);
    call(&table, c, "transport.stop", Value::Null);
    session.renderer.render(&mut [0.0; 128]);
    assert_eq!(daw_rpc::tick(c)["positionTicks"], 3840, "stop returns to the cue");
    call(&table, c, "transport.setCue", json!({ "tick": null }));
    assert_eq!(daw_rpc::tick(c)["cue"], Value::Null);
}

#[test]
fn the_song_loop_is_switched_over_rpc_and_shown_in_the_telemetry() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let tick = daw_rpc::tick(c);
    assert_eq!((&tick["songLoop"], &tick["wrap"]), (&json!(true), &Value::Null), "on, but an empty song has nothing to loop");

    let track = call(&table, c, "tracks.add", json!({ "name": "Kick", "kind": "kick", "defaultPitch": 36 }));
    call(&table, c, "clips.addNote", json!({ "track": track, "startTick": 0, "lengthTicks": 4800 }));
    assert_eq!(daw_rpc::tick(c)["wrap"], json!([0, 7680]), "the song end rounded up to a whole bar");
    call(&table, c, "clips.setLoopRegion", json!({ "region": [0, 1920] }));
    assert_eq!(daw_rpc::tick(c)["wrap"], json!([0, 1920]), "the loop region wins");
    call(&table, c, "clips.setLoopRegion", json!({ "region": null }));

    call(&table, c, "transport.setSongLoop", json!({ "enabled": false }));
    let tick = daw_rpc::tick(c);
    assert_eq!((&tick["songLoop"], &tick["wrap"]), (&json!(false), &Value::Null), "off: playback runs on past the end");
}
