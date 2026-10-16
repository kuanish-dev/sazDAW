//! `noteEffects.*` methods and `catalog.get` → `noteEffects`: JSON shape, the chain in `project.get`, errors, undo.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

fn chain(table: &RpcTable, c: &daw_contracts::Controls, track: &Value) -> Vec<Value> {
    let project = project(table, c);
    let track = project["tracks"].as_array().unwrap().iter().find(|t| t["id"] == *track).unwrap().clone();
    track.get("noteEffects").and_then(Value::as_array).cloned().unwrap_or_default()
}

#[test]
fn catalog_lists_note_effect_kinds_with_choices() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let catalog = call(&table, c, "catalog.get", Value::Null);
    let kinds = catalog["noteEffects"].as_array().unwrap();
    let names: Vec<&str> = kinds.iter().map(|k| k["kind"].as_str().unwrap()).collect();
    assert_eq!(names, ["arp", "chord"]);
    let rate = kinds[0]["params"].as_array().unwrap().iter().find(|p| p["name"] == "rate").unwrap();
    assert_eq!(rate["kind"], "choice");
    assert_eq!(rate["choices"][3], "1/16");
}

#[test]
fn note_effects_are_added_edited_moved_and_removed_over_the_wire() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    assert!(chain(&table, c, &track).is_empty(), "an empty chain is not written to JSON");

    let arp = call(&table, c, "noteEffects.add", json!({ "track": track, "kind": "arp", "params": { "octaves": 2 } }));
    let chord = call(&table, c, "noteEffects.add", json!({ "track": track, "kind": "chord" }));
    let effects = chain(&table, c, &track);
    assert_eq!(effects[0]["id"], arp);
    assert_eq!(effects[0]["kind"], "arp");
    assert_eq!(effects[0]["params"]["octaves"], 2.0);
    assert_eq!(effects[0]["bypass"], false);
    assert_eq!(effects[1]["id"], chord);

    call(&table, c, "noteEffects.setParam", json!({ "track": track, "effect": chord, "name": "type", "value": 1.6 }));
    call(&table, c, "noteEffects.setBypass", json!({ "track": track, "effect": arp, "bypass": true }));
    call(&table, c, "noteEffects.move", json!({ "track": track, "effect": chord, "index": 0 }));
    let effects = chain(&table, c, &track);
    assert_eq!(effects[0]["id"], chord);
    assert_eq!(effects[0]["params"]["type"], 2.0, "the option index is whole");
    assert_eq!(effects[1]["bypass"], true);

    call(&table, c, "noteEffects.remove", json!({ "track": track, "effect": arp }));
    assert_eq!(chain(&table, c, &track).len(), 1);
    call(&table, c, "history.undo", Value::Null);
    assert_eq!(chain(&table, c, &track).len(), 2);
}

#[test]
fn wrong_requests_are_refused_with_their_codes() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let audio = call(&table, c, "tracks.add", json!({ "name": "Audio", "kind": null, "defaultPitch": 60 }));
    let error = table.call(c, "noteEffects.add", json!({ "track": audio, "kind": "arp" })).unwrap_err();
    assert!(error.is(codes::notes::NOTE_EFFECTS_NEED_INSTRUMENT), "{error}");
    let error = table.call(c, "noteEffects.add", json!({ "track": track, "kind": "flanger" })).unwrap_err();
    assert!(error.is(codes::notes::UNKNOWN_NOTE_EFFECT), "{error}");
    let error = table.call(c, "noteEffects.setParam", json!({ "track": track, "effect": 9999, "name": "rate", "value": 1 })).unwrap_err();
    assert!(error.is(codes::notes::NOTE_EFFECT_NOT_FOUND), "{error}");
    assert!(table.call(c, "noteEffects.remove", json!({ "track": track })).is_err(), "no effect given");
}
