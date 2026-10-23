//! MIDI controllers via RPC: clip controller lanes (`notes.setControllerLane`), live controller, MIDI learn.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

fn clip_of(table: &RpcTable, c: &daw_contracts::Controls, id: &Value) -> Value {
    project(table, c)["clips"].as_array().unwrap().iter().find(|clip| clip["id"] == *id).unwrap().clone()
}

#[test]
fn controller_lane_uses_the_agreed_wire_and_is_one_undo_step() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track)["id"].clone();
    let lane =
        json!({ "clip": clip, "kind": { "type": "pitchBend" }, "points": [{ "tick": 480, "value": 0.5 }, { "tick": 0, "value": -1.0 }] });
    call(&table, c, "notes.setControllerLane", lane);
    call(
        &table,
        c,
        "notes.setControllerLane",
        json!({ "clip": clip, "kind": { "type": "cc", "number": 64 }, "points": [{ "tick": 0, "value": 1.0 }] }),
    );
    let content = clip_of(&table, c, &clip)["content"].clone();
    assert_eq!(
        content["controllers"],
        json!([
            { "kind": { "type": "pitchBend" }, "points": [{ "tick": 0, "value": -1.0 }, { "tick": 480, "value": 0.5 }] },
            { "kind": { "type": "cc", "number": 64 }, "points": [{ "tick": 0, "value": 1.0 }] }
        ])
    );

    let bad = table.call(c, "notes.setControllerLane", json!({ "clip": clip, "kind": { "type": "cc", "number": 120 }, "points": [] }));
    assert!(bad.is_err());
    let out_of_range = table.call(
        c,
        "notes.setControllerLane",
        json!({ "clip": clip, "kind": { "type": "aftertouch" }, "points": [{ "tick": 0, "value": 2.0 }] }),
    );
    assert!(out_of_range.unwrap_err().is(codes::notes::CONTROLLER_VALUE_OUT_OF_RANGE));

    call(&table, c, "history.undo", Value::Null);
    assert_eq!(clip_of(&table, c, &clip)["content"]["controllers"].as_array().unwrap().len(), 1);
    call(&table, c, "notes.setControllerLane", json!({ "clip": clip, "kind": { "type": "pitchBend" }, "points": [] }));
    assert!(clip_of(&table, c, &clip)["content"].get("controllers").is_none(), "with no lanes the field is not written");

    call(&table, c, "live.controller", json!({ "track": track, "kind": { "type": "cc", "number": 1 }, "value": 0.7 }));
}

#[test]
fn midi_learn_maps_the_next_cc_and_the_knob_moves_the_parameter() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let bus = call(&table, c, "graph.addBus", json!({ "name": "Bus", "x": 0.0, "y": 0.0 }));
    let target = json!({ "type": "nodeVolume", "node": bus });
    call(&table, c, "midi.learn", json!({ "target": target }));
    assert_eq!(call(&table, c, "midi.mappings", Value::Null)["learning"], target);

    assert!(c.midi_mappings.control_change(0, 20, 0));
    let listed = call(&table, c, "midi.mappings", Value::Null);
    assert_eq!(listed["learning"], Value::Null);
    assert_eq!(listed["mappings"], json!([{ "cc": 20, "channel": 0, "target": target, "min": 0.0, "max": 1.5 }]));
    assert_eq!(project(&table, c)["midiMappings"], listed["mappings"], "the mappings are stored in the project");

    assert!(c.midi_mappings.control_change(0, 20, 127));
    let node = project(&table, c)["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["id"] == bus).unwrap().clone();
    assert_eq!(node["kind"]["volume"], 1.5);

    call(&table, c, "midi.removeMapping", json!({ "target": target }));
    assert_eq!(call(&table, c, "midi.mappings", Value::Null)["mappings"], json!([]));
    assert!(table.call(c, "midi.removeMapping", json!({ "target": target })).is_err());
    call(&table, c, "midi.learn", json!({ "target": target }));
    call(&table, c, "midi.cancelLearn", Value::Null);
    assert!(!c.midi_mappings.control_change(0, 21, 5), "learning cancelled: the CC goes to the instrument");
    assert!(table.call(c, "midi.learn", json!({ "target": { "type": "nodeVolume", "node": 9999 } })).is_err());
}
