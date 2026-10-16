//! The wire for the AI "build and check" work: `analysis.levels`, `analysis.check`, batch clips `clips.addNoteMany`
//! and `project.clear` — JSON shape, one undo step, error codes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

const BAR: u32 = 3840;

#[test]
fn add_note_many_creates_all_clips_in_one_undo_step_or_none() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = call(&table, c, "tracks.add", json!({ "name": "Kick", "kind": "kick", "defaultPitch": 36 }));
    let ids = call(
        &table,
        c,
        "clips.addNoteMany",
        json!({ "clips": [
        { "track": track, "startTick": 0, "notes": [NOTE(36, 0)] },
        { "track": track, "startTick": BAR, "lengthTicks": BAR * 2, "notes": [NOTE(36, 0), NOTE(36, 960)], "loopTicks": 1920 },
    ] }),
    );
    let ids = ids.as_array().unwrap();
    assert_eq!(ids.len(), 2);
    let clips = project(&table, c)["clips"].as_array().unwrap().clone();
    assert_eq!(clips.iter().map(|clip| &clip["id"]).collect::<Vec<_>>(), ids.iter().collect::<Vec<_>>());
    assert_eq!(clips[1]["content"]["loopTicks"], 1920);

    assert_eq!(call(&table, c, "history.undo", Value::Null), true);
    assert!(project(&table, c)["clips"].as_array().unwrap().is_empty(), "both clips went away in one step");

    let overlap = table.call(
        c,
        "clips.addNoteMany",
        json!({ "clips": [
        { "track": track, "startTick": 0, "lengthTicks": BAR * 2 },
        { "track": track, "startTick": BAR },
    ] }),
    );
    assert!(overlap.is_err());
    assert!(project(&table, c)["clips"].as_array().unwrap().is_empty(), "all or nothing");
}

#[test]
fn project_clear_removes_content_in_one_undo_step() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = call(&table, c, "tracks.add", json!({ "name": "Kick", "kind": "kick", "defaultPitch": 36 }));
    call(&table, c, "clips.addNote", json!({ "track": track, "startTick": 0, "notes": [NOTE(36, 0)] }));
    call(&table, c, "graph.addBus", json!({ "name": "Bus", "x": 0, "y": 0 }));
    call(&table, c, "tempo.set", json!({ "bpm": 140 }));

    call(&table, c, "project.clear", json!({ "keepTempo": true }));
    let cleared = project(&table, c);
    assert!(cleared["tracks"].as_array().unwrap().is_empty() && cleared["clips"].as_array().unwrap().is_empty());
    assert_eq!(cleared["graph"]["nodes"].as_array().unwrap().len(), 1, "only the master");
    assert_eq!(cleared["tempoMap"]["tempos"][0]["bpm"], 140.0, "the tempo is kept");

    assert_eq!(call(&table, c, "history.undo", Value::Null), true);
    assert_eq!(project(&table, c)["tracks"].as_array().unwrap().len(), 1, "undo brought everything back");
    call(&table, c, "project.clear", json!({}));
    assert_ne!(project(&table, c)["tempoMap"]["tempos"][0]["bpm"], 140.0, "by default the tempo is reset");
}

#[test]
fn analysis_levels_and_check_answer_in_the_agreed_shape() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    call(&table, c, "tempo.set", json!({ "bpm": 120 }));
    let kick = call(&table, c, "tracks.add", json!({ "name": "Kick", "kind": "kick", "defaultPitch": 36 }));
    let notes: Vec<Value> = (0..4).map(|beat| NOTE(36, beat * 960)).collect();
    call(&table, c, "clips.addNote", json!({ "track": kick, "startTick": 0, "lengthTicks": BAR * 2, "notes": notes }));
    let muted = call(&table, c, "tracks.add", json!({ "name": "Quiet", "kind": "kick", "defaultPitch": 36 }));
    let node = project(&table, c)["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["kind"]["track"] == muted).unwrap()["id"].clone();
    call(&table, c, "graph.setMute", json!({ "id": node, "mute": true }));

    let report = call(&table, c, "analysis.levels", json!({ "per": "track", "resolutionBars": 1 }));
    assert_eq!(report["fromTick"], 0);
    assert_eq!(report["toTick"], BAR * 2);
    assert_eq!(report["sections"], json!([{ "bar": 1, "fromTick": 0, "toTick": BAR }, { "bar": 2, "fromTick": BAR, "toTick": BAR * 2 }]));
    let targets = report["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!((targets[0]["id"].clone(), targets[0]["name"].clone()), (kick.clone(), json!("Kick")));
    assert!(targets[0]["level"]["peakDb"].as_f64().unwrap() < 0.0, "{}", targets[0]);
    assert!(targets[0]["sections"][0]["rmsDb"].is_number(), "{}", targets[0]);
    assert_eq!(targets[1]["level"], Value::Null, "a track without clips is silent");

    let master = call(&table, c, "analysis.levels", json!({}));
    assert_eq!(master["targets"][0]["id"], MASTER_ID);
    assert!(master["targets"][0]["loudnessLufs"].is_number(), "{master}");

    let findings = call(&table, c, "analysis.check", json!({}));
    let muted_finding = findings.as_array().unwrap().iter().find(|f| f["code"] == "mutedTrack").expect("mute found");
    assert_eq!(muted_finding["severity"], "warning");
    assert_eq!(muted_finding["track"], muted);
    assert!(muted_finding["message"].as_str().unwrap().contains("Quiet"));

    let bad = table.call(c, "analysis.levels", json!({ "fromTick": BAR * 2, "toTick": BAR })).unwrap_err();
    assert!(bad.is(codes::export::RANGE_BACKWARDS), "{bad}");
    let failed = call(&table, c, "analysis.check", json!({ "fromTick": BAR * 2, "toTick": BAR }));
    assert!(failed.as_array().unwrap().iter().any(|f| f["code"] == "renderFailed"), "{failed}");
}

const MASTER_ID: u32 = 0;
