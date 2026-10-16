//! Track bounce (`tracks.bounce/freeze/unfreeze`), template (`templates.*`, `project.createFromTemplate`)
//! and preset (`presets.*`) methods: JSON shape there and back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("daw-rpc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn bounce_freeze_and_unfreeze_over_the_wire() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let dir = temp("bounce");
    call(&table, c, "project.create", json!({ "path": dir.join("s/s.dawproj") }));
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track);
    call(&table, c, "notes.add", json!({ "clip": clip["id"], "note": NOTE(60, 0) }));

    let copy = call(&table, c, "tracks.bounce", json!({ "id": track, "tailSeconds": 0.5 }));
    assert!(copy.is_number());
    let p = project(&table, c);
    assert_eq!(p["tracks"][1]["id"], copy, "the copy is below the track");
    assert!(p["tracks"][1]["instrument"].is_null(), "the copy is an audio track");
    call(&table, c, "history.undo", Value::Null);

    let frozen = call(&table, c, "tracks.freeze", json!({ "id": track }));
    assert_eq!(project(&table, c)["tracks"][0]["instrument"]["frozen"], frozen, "without tailSeconds the tail is the default");
    let refused = table.call(c, "tracks.setInstrument", json!({ "id": track, "kind": "sampler" })).unwrap_err();
    assert!(refused.is(codes::tracks::FROZEN), "{refused}");
    call(&table, c, "tracks.unfreeze", json!({ "id": track }));
    let p = project(&table, c);
    assert!(p["tracks"][0]["instrument"].get("frozen").is_none(), "the mark is not written while absent");
    assert_eq!(p["tracks"].as_array().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn templates_and_presets_over_the_wire() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let dir = temp("templates");
    let track = call(&table, c, "tracks.add", json!({ "name": "Sampler", "kind": "sampler", "defaultPitch": 60 }));
    call(&table, c, "tracks.setParam", json!({ "id": track, "name": "gain", "value": 0.25 }));

    let target = json!({ "type": "instrument", "track": track });
    call(&table, c, "presets.save", json!({ "target": target, "name": "Soft" }));
    assert_eq!(call(&table, c, "presets.list", json!({ "kind": "sampler" })), json!(["Soft"]));
    call(&table, c, "tracks.setParam", json!({ "id": track, "name": "gain", "value": 1.0 }));
    call(&table, c, "presets.apply", json!({ "target": target, "name": "Soft" }));
    assert_eq!(project(&table, c)["tracks"][0]["instrument"]["params"]["gain"], 0.25);
    call(&table, c, "presets.remove", json!({ "kind": "sampler", "name": "Soft" }));
    assert_eq!(call(&table, c, "presets.list", json!({ "kind": "sampler" })), json!([]));

    call(&table, c, "templates.save", json!({ "name": "Starter" }));
    assert_eq!(call(&table, c, "templates.list", Value::Null), json!(["Starter"]));
    call(&table, c, "project.createFromTemplate", json!({ "path": dir.join("n/n.dawproj"), "template": "Starter" }));
    let p = project(&table, c);
    assert_eq!((p["name"].as_str(), p["tracks"][0]["name"].as_str()), (Some("n"), Some("Sampler")));
    call(&table, c, "templates.remove", json!({ "name": "Starter" }));
    assert!(table.call(c, "templates.remove", json!({ "name": "Starter" })).is_err());
    std::fs::remove_dir_all(dir).ok();
}
