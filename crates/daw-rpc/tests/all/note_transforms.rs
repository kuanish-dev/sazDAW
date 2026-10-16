//! Musical note edits via RPC: `notes.quantize/humanize/transpose/fitToScale/legato`, groove (`notes.groove`,
//! `notes.grooveFromClip`) and variations (`notes.vary`, `notes.varyToNewClip`) — param shape and errors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn note_transforms_work_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track)["id"].clone();
    let added = call(&table, c, "notes.addMany", json!({ "clip": clip, "notes": [NOTE(61, 100), NOTE(66, 500)] }));
    let first = added[0]["id"].clone();
    let starts = || {
        let notes = project(&table, c)["clips"][0]["content"]["notes"].clone();
        notes.as_array().unwrap().iter().map(|n| (n["pitch"].as_u64().unwrap(), n["startTick"].as_u64().unwrap())).collect::<Vec<_>>()
    };

    // strength and ends are optional: by default exactly onto the line, length kept.
    let quantized = call(&table, c, "notes.quantize", json!({ "clip": clip, "grid": 240 }));
    assert_eq!(quantized.as_array().unwrap().len(), 2);
    assert_eq!(starts(), [(61, 0), (66, 480)]);

    call(&table, c, "notes.fitToScale", json!({ "clip": clip, "root": 0, "scale": "major" }));
    assert_eq!(starts(), [(60, 0), (65, 480)]);

    call(&table, c, "notes.transpose", json!({ "clip": clip, "ids": [first], "semitones": 12 }));
    assert_eq!(starts(), [(72, 0), (65, 480)]);
    assert!(table.call(c, "notes.transpose", json!({ "clip": clip, "semitones": 100 })).is_err());

    let legato = call(&table, c, "notes.legato", json!({ "clip": clip }));
    assert_eq!(legato[0]["lengthTicks"], 480);

    let a = call(&table, c, "notes.humanize", json!({ "clip": clip, "timingTicks": 20, "velocity": 10, "seed": 5 }));
    call(&table, c, "history.undo", Value::Null);
    let b = call(&table, c, "notes.humanize", json!({ "clip": clip, "timingTicks": 20, "velocity": 10, "seed": 5 }));
    assert_eq!(a, b, "one seed, one result");

    assert!(table.call(c, "notes.fitToScale", json!({ "clip": clip, "root": 0, "scale": "blues" })).is_err(), "unknown scale");
    assert!(table.call(c, "notes.quantize", json!({ "clip": clip, "ids": [], "grid": 240 })).is_err(), "empty note list");
}

/// (start, velocity) of the notes of clip `clip` in the project.
fn timing(table: &RpcTable, c: &daw_contracts::Controls, clip: &Value) -> Vec<(u64, u64)> {
    let project = project(table, c);
    let found = project["clips"].as_array().unwrap().iter().find(|x| x["id"] == *clip).unwrap().clone();
    let notes = found["content"]["notes"].as_array().unwrap().clone();
    notes.iter().map(|n| (n["startTick"].as_u64().unwrap(), n["velocity"].as_u64().unwrap())).collect()
}

#[test]
fn groove_works_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track)["id"].clone();
    call(&table, c, "notes.addMany", json!({ "clip": clip, "notes": [NOTE(60, 0), NOTE(60, 240), NOTE(60, 480), NOTE(60, 720)] }));

    // amount is optional: 1 by default.
    let swing = json!({ "type": "swing", "grid": "sixteenth", "percent": 58 });
    call(&table, c, "notes.groove", json!({ "clip": clip, "template": swing }));
    assert_eq!(timing(&table, c, &clip), [(0, 100), (278, 100), (480, 100), (758, 100)]);
    let accent = json!({ "type": "accent", "percent": 150 });
    call(&table, c, "notes.groove", json!({ "clip": clip, "template": accent, "amount": 0.5 }));
    assert_eq!(timing(&table, c, &clip)[0], (0, 125));
    let laid_back = json!({ "type": "laidBack", "ticks": 500 });
    assert!(table.call(c, "notes.groove", json!({ "clip": clip, "template": laid_back })).is_err(), "shift above 120");
    assert!(table.call(c, "notes.groove", json!({ "clip": clip, "template": { "type": "shuffle" } })).is_err(), "unknown template");

    // Groove from a clip: the source has this same swing; a straight clip receives it.
    let straight = call(&table, c, "clips.addNote", json!({ "track": track, "startTick": 3840, "lengthTicks": null }))["id"].clone();
    call(&table, c, "notes.addMany", json!({ "clip": straight, "notes": [NOTE(62, 0), NOTE(62, 240)] }));
    call(&table, c, "notes.grooveFromClip", json!({ "clip": straight, "sourceClip": clip, "amount": 1 }));
    assert_eq!(timing(&table, c, &straight).iter().map(|t| t.0).collect::<Vec<_>>(), [0, 278]);
    assert!(table.call(c, "notes.grooveFromClip", json!({ "clip": straight, "sourceClip": 9999 })).is_err());
}

#[test]
fn variations_work_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track)["id"].clone();
    let notes: Vec<Value> = (0..8).map(|i| NOTE([48, 51, 55, 58][i % 4], i as u32 * 480)).collect();
    call(&table, c, "notes.addMany", json!({ "clip": clip, "notes": notes }));

    let params = json!({ "clip": clip, "seed": 11, "density": 0.5, "pitch": 0.5, "rhythm": 0.5, "velocity": 0.5,
        "scale": { "root": 0, "scale": "minor" } });
    let a = call(&table, c, "notes.vary", params.clone());
    call(&table, c, "history.undo", Value::Null);
    let b = call(&table, c, "notes.vary", params);
    assert_eq!(a, b, "one seed, one result");
    call(&table, c, "history.undo", Value::Null);
    // All params except clip are optional: an empty variation changes nothing.
    let same = call(&table, c, "notes.vary", json!({ "clip": clip }));
    assert_eq!(same.as_array().unwrap().len(), 8);
    assert!(table.call(c, "notes.vary", json!({ "clip": clip, "density": 3 })).is_err());

    let copy = call(&table, c, "notes.varyToNewClip", json!({ "clip": clip, "seed": 2, "pitch": 1 }));
    assert_eq!(copy["startTick"], 3840, "the copy goes right after the clip");
    assert_eq!(copy["track"], track);
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 2);
    call(&table, c, "history.undo", Value::Null);
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 1, "the copy and the variation are one undo step");
}
