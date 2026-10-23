//! `project.*` and `log.*` methods and the window's project feed: project JSON, files, demo, the error log.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;
use daw_rpc::ProjectFeed;

#[test]
fn project_json_uses_camel_case_and_tagged_enums() {
    let session = build_session(true);
    let table = RpcTable::new();
    let project = project(&table, &session.controls);

    assert!(project["tempoMap"]["signatures"][0]["signature"]["numerator"].is_number());
    assert!(project["tracks"].as_array().is_some_and(|t| !t.is_empty()));
    assert!(project["clips"].as_array().is_some_and(|c| !c.is_empty()));
    assert!(project.get("loopRegion").is_some(), "the loopRegion field is always there (null without a loop)");
    for removed in ["channels", "patterns", "playlist", "mode", "currentPattern"] {
        assert!(project.get(removed).is_none(), "the project must not have \"{removed}\"");
    }
    let nodes = project["graph"]["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["id"], 0, "the master is node 0");
    assert_eq!(nodes[0]["kind"]["type"], "master");

    let track = &project["tracks"][0];
    assert!(track["id"].is_number() && track["name"].is_string() && track["defaultPitch"].is_number());
    let clip = &project["clips"][0];
    assert!(clip["startTick"].is_number() && clip["lengthTicks"].is_number() && clip["track"].is_number());
}

#[test]
fn project_summary_is_a_short_overview_of_the_same_project() {
    let session = build_session(true);
    let (table, c) = (RpcTable::new(), &session.controls);
    let full = project(&table, c);
    let summary = call(&table, c, "project.summary", Value::Null);

    let ids = |tracks: &Value| tracks.as_array().unwrap().iter().map(|t| t["id"].clone()).collect::<Vec<_>>();
    assert_eq!(ids(&summary["tracks"]), ids(&full["tracks"]), "the same tracks in the same order");
    assert!(summary["bpm"].is_number() && summary["endTick"].as_u64().is_some_and(|end| end > 0), "{summary}");
    let track = &summary["tracks"][0];
    assert!(track["node"].is_number() && track["volume"].is_number() && track["spans"].is_array(), "{track}");
    assert!(summary.to_string().len() < full.to_string().len(), "the summary is shorter than the project");
}

#[test]
fn saving_remembers_the_path_and_loading_restores_it() {
    let session = build_session(true);
    let (table, c) = (RpcTable::new(), &session.controls);
    let dir = std::env::temp_dir().join(format!("daw-rpc-path-{}", std::process::id()));
    let file = dir.join("song.dawproj");

    call(&table, c, "project.save", json!({ "path": file.display().to_string() }));
    let status = call(&table, c, "project.status", Value::Null);
    assert_eq!(status["path"], file.display().to_string());
    assert_eq!(status["dirty"], false);

    call(&table, c, "project.new", Value::Null);
    assert!(call(&table, c, "project.status", Value::Null)["path"].is_null());
    call(&table, c, "project.load", json!({ "path": file.display().to_string() }));
    assert_eq!(call(&table, c, "project.status", Value::Null)["path"], file.display().to_string());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn project_folder_lifecycle_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let root = std::env::temp_dir().join(format!("daw-rpc-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let file = root.join("Song").join("Song.dawproj");

    call(&table, c, "project.create", json!({ "path": file.display().to_string() }));
    let status = call(&table, c, "project.status", Value::Null);
    assert_eq!(status["name"], "Song");
    assert_eq!(status["folder"], root.join("Song").display().to_string());
    assert_eq!(status["dirty"], false);
    let recent = call(&table, c, "project.recent", Value::Null);
    assert_eq!(recent[0]["name"], "Song");
    assert_eq!(recent[0]["path"], file.display().to_string());
    assert_eq!(call(&table, c, "project.autosave", Value::Null), Value::Null);

    add_synth(&table, c);
    let autosave = root.join("Song/autosave/autosave.dawproj");
    std::fs::create_dir_all(autosave.parent().unwrap()).unwrap();
    c.storage.save_quick(&autosave, &c.view.snapshot()).unwrap();
    assert_eq!(call(&table, c, "project.autosave", Value::Null), autosave.display().to_string());

    call(&table, c, "project.load", json!({ "path": file.display().to_string() }));
    assert_eq!(call(&table, c, "project.get", Value::Null)["tracks"].as_array().unwrap().len(), 0);
    call(&table, c, "project.restoreAutosave", Value::Null);
    assert_eq!(call(&table, c, "project.get", Value::Null)["tracks"].as_array().unwrap().len(), 1);
    assert_eq!(call(&table, c, "project.status", Value::Null)["dirty"], true);
    assert_eq!(call(&table, c, "project.autosave", Value::Null), Value::Null, "the file is removed after restoring");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn errors_and_frontend_reports_land_in_the_error_log() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    call(&table, c, "log.write", json!({ "level": "error", "source": "frontend", "message": "boom", "details": "stack" }));
    assert!(table.call(c, "graph.connect", json!({ "from": 0, "to": 99, "level": 1.0, "sidechain": false })).is_err());
    assert!(table.call(c, "tracks.add", json!({ "nope": 1 })).is_err());
    assert!(table.call(c, "no.such", Value::Null).is_err());

    let recent = call(&table, c, "log.recent", json!({ "limit": 10 }));
    let entries = recent.as_array().unwrap();
    assert_eq!(entries[0]["source"], "frontend");
    assert_eq!(entries[0]["details"], "stack");
    let rpc: Vec<&Value> = entries.iter().filter(|e| e["source"] == "rpc").collect();
    assert_eq!(rpc.len(), 3, "three refusals: {entries:?}");
    assert_eq!(rpc[0]["level"], "warn", "a refusal by the rules is a warning");
    assert_eq!(rpc[1]["level"], "error", "invalid params are an error");
    assert!(rpc[1]["message"].as_str().unwrap().contains("tracks.add"));
    assert!(call(&table, c, "log.file", Value::Null).as_str().unwrap().ends_with("errors.log"));
    assert_eq!(call(&table, c, "log.recent", json!({ "limit": 100 })).as_array().unwrap().len(), entries.len());
}

#[test]
fn fill_demo_adds_a_beat_to_an_empty_project_and_leaves_it_unmodified() {
    let session = build_session(false);
    let table = RpcTable::new();
    assert!(project(&table, &session.controls)["tracks"].as_array().unwrap().is_empty());

    call(&table, &session.controls, "project.fillDemo", Value::Null);

    assert_eq!(project(&table, &session.controls)["tracks"].as_array().unwrap().len(), 5);
    let status = call(&table, &session.controls, "project.status", Value::Null);
    assert_eq!(status["dirty"], false);
    assert_eq!(status["canUndo"], false);
}

/// `project.export` over the wire: new fields in camelCase, extension by format, loudness and peak in the answer, refusals
/// before rendering come as errors.
#[test]
fn export_takes_format_normalize_and_range_over_the_wire() {
    let session = build_session(true);
    let (table, c) = (RpcTable::new(), &session.controls);
    let dir = std::env::temp_dir().join(format!("daw-rpc-export-{}", std::process::id()));
    let path = dir.join("song.wav");
    let base = json!({ "path": path.display().to_string(), "sampleRate": 44100, "bitDepth": "int16", "stems": false, "tailSeconds": 0.0 });
    let with = |extra: Value| {
        let mut request = base.clone();
        request.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        request
    };

    let report = call(
        &table,
        c,
        "project.export",
        with(json!({
            "format": "flac", "normalize": { "lufs": -16.0 }, "range": { "fromTick": 0, "toTick": 3840 }
        })),
    );
    let file = report["files"][0].as_str().unwrap();
    assert!(file.ends_with("song.flac"), "{file}");
    assert!(std::path::Path::new(file).is_file());
    assert!(report["frames"].as_u64().unwrap() > 0);
    let lufs = report["loudnessLufs"].as_f64().unwrap();
    let peak = report["peakDb"].as_f64().unwrap();
    assert!(peak <= -0.95, "ceiling −1 dBTP: {peak}");
    assert!(lufs <= -15.5, "not louder than the target: {lufs}");
    assert!(!dir.join("song.flac.render-tmp").exists(), "the temporary file is removed");

    let plain = call(&table, c, "project.export", with(json!({ "range": { "fromTick": 0, "toTick": 1920 } })));
    assert!(plain["files"][0].as_str().unwrap().ends_with("song.wav"));
    assert!(plain["loudnessLufs"].is_number() && plain["peakDb"].is_number());

    for bad in [
        json!({ "format": "mp3", "mp3Bitrate": 100 }),
        json!({ "format": "flac", "bitDepth": "float32" }),
        json!({ "range": "loop" }),
        json!({ "normalize": { "lufs": 5.0 } }),
        json!({ "format": "ogg" }),
    ] {
        assert!(table.call(c, "project.export", with(bad.clone())).is_err(), "{bad}");
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_feed_sends_the_whole_project_first_and_then_only_what_changed() {
    let session = build_session(true);
    let (table, c) = (RpcTable::new(), &session.controls);
    let mut feed = ProjectFeed::new();
    let first = update(&mut feed, c).expect("the first poll sends the project");
    let project = project(&table, c);
    assert_eq!(first["whole"], true);
    assert_eq!(first["changes"]["tracks"]["ids"].as_array().unwrap().len(), project["tracks"].as_array().unwrap().len());
    assert_eq!(first["changes"]["nodes"]["changed"], project["graph"]["nodes"], "the whole graph, without plugin states like project.get");
    assert_eq!(first["status"]["canUndo"], false);
    assert!(first["status"].get("revision").is_none(), "the window keeps no revision");
    assert!(update(&mut feed, c).is_none(), "without edits there is nothing to send");

    call(&table, c, "graph.setVolume", json!({ "id": 0, "volume": 0.5 }));
    let changed = update(&mut feed, c).expect("an edit is sent with the next poll");
    assert_eq!(changed["whole"], false);
    assert_eq!(
        changed["changes"],
        json!({ "nodes": { "ids": changed["changes"]["nodes"]["ids"], "changed": [project_node(&table, c, 0)] } }),
        "only the master"
    );
    assert_eq!(changed["status"]["canUndo"], true, "the status comes with the changes");

    call(&table, c, "history.undo", Value::Null);
    let undone = update(&mut feed, c).unwrap();
    assert_eq!(undone["changes"]["nodes"]["changed"][0]["id"], 0, "undo comes like any edit");
}

#[test]
fn a_window_that_asks_gets_the_whole_project_with_the_next_poll() {
    let session = build_session(true);
    let c = &session.controls;
    let mut feed = ProjectFeed::new();
    let first = update(&mut feed, c).unwrap();
    assert!(update(&mut feed, c).is_none());

    feed.whole_request().ask();
    let again = update(&mut feed, c).expect("asked for the whole project");
    assert_eq!((&again["whole"], &again["changes"]), (&first["whole"], &first["changes"]));
    assert!(update(&mut feed, c).is_none(), "asked once, sent once");
}

/// The feed's next update as the window gets it.
fn update(feed: &mut ProjectFeed, c: &daw_contracts::Controls) -> Option<Value> {
    feed.poll(c).map(|update| serde_json::to_value(update).unwrap())
}

/// One graph node as `project.get` returns it.
fn project_node(table: &RpcTable, c: &daw_contracts::Controls, id: u64) -> Value {
    let project = project(table, c);
    project["graph"]["nodes"].as_array().unwrap().iter().find(|node| node["id"] == id).unwrap().clone()
}
