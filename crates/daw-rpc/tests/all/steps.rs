//! The song key (`key.*`) and melodic step lines (`clips.addSteps`) over the wire: JSON shape, defaults, the key in
//! `project.get`, degrees counted in it, one undo step and the error codes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

fn pitches(clip: &Value) -> Vec<u64> {
    clip["content"]["notes"].as_array().unwrap().iter().map(|note| note["pitch"].as_u64().unwrap()).collect()
}

#[test]
fn the_key_is_minor_by_default_and_travels_in_the_project() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert_eq!(call(&table, c, "key.get", Value::Null), Value::Null);
    assert_eq!(project(&table, c)["key"], Value::Null, "no key is null in the project");

    call(&table, c, "key.set", json!({ "root": 5 }));
    assert_eq!(call(&table, c, "key.get", Value::Null), json!({ "root": 5, "scale": "minor" }));
    call(&table, c, "key.set", json!({ "root": 2, "scale": "dorian" }));
    assert_eq!(project(&table, c)["key"], json!({ "root": 2, "scale": "dorian" }));
    call(&table, c, "key.clear", Value::Null);
    assert_eq!(call(&table, c, "key.get", Value::Null), Value::Null);
    call(&table, c, "history.undo", Value::Null);
    assert_eq!(call(&table, c, "key.get", Value::Null)["scale"], "dorian");

    let bad = table.call(c, "key.set", json!({ "root": 12 })).unwrap_err();
    assert!(bad.is(codes::notes::SCALE_ROOT_OUT_OF_RANGE), "{bad}");
}

#[test]
fn a_step_line_becomes_a_looping_clip_in_one_undo_step() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let riff = "F2 F3! - F2 Eb3~ F3 - C3 | F2 _ Ab2! - C3~ Eb3 F2 -";
    let clip = call(&table, c, "clips.addSteps", json!({ "track": track, "startTick": 0, "lengthTicks": 15_360, "steps": riff }));
    assert_eq!((clip["lengthTicks"].as_u64(), clip["content"]["loopTicks"].as_u64()), (Some(15_360), Some(3840)));
    assert_eq!(pitches(&clip), [41, 53, 41, 51, 53, 48, 41, 44, 48, 51, 41]);
    let notes = clip["content"]["notes"].as_array().unwrap();
    assert_eq!((notes[1]["velocity"].as_u64(), notes[0]["velocity"].as_u64()), (Some(115), Some(92)), "! is an accent");
    assert_eq!(notes[3]["lengthTicks"], 300, "Eb3~ slides: its step and an overlap into F3");
    assert_eq!(notes[6]["lengthTicks"], 360, "F2 _ holds a step, then the gate");

    call(&table, c, "history.undo", Value::Null);
    assert!(project(&table, c)["clips"].as_array().unwrap().is_empty(), "the clip with its notes was one undo step");
}

#[test]
fn degrees_count_in_the_key_and_mistakes_carry_their_codes() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let line = |steps: &str| json!({ "track": track, "startTick": 0, "steps": steps, "octave": 2 });
    let no_key = table.call(c, "clips.addSteps", line("1 3 5 8")).unwrap_err();
    assert!(no_key.is(codes::patterns::DEGREE_WITHOUT_KEY), "{no_key}");
    let bad = table.call(c, "clips.addSteps", line("F2 Q2")).unwrap_err();
    assert!(bad.is(codes::patterns::BAD_STEP) && bad.to_string().contains("Step 2"), "{bad}");

    call(&table, c, "key.set", json!({ "root": 5 }));
    assert_eq!(pitches(&call(&table, c, "clips.addSteps", line("1 3 5 8"))), [41, 44, 48, 53]);
}
