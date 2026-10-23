//! Editing the project via RPC: tracks, clips, notes, batch methods and error codes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn note_clip_content_json_shape() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track);
    call(&table, c, "notes.add", json!({ "clip": clip["id"], "note": NOTE(60, 0) }));

    let project = project(&table, c);
    let content = &project["clips"][0]["content"];
    assert_eq!(content["type"], "notes");
    assert_eq!(content["notes"][0]["startTick"], 0);
    assert_eq!(content["notes"][0]["pitch"], 60);
    assert!(content["nextNoteId"].is_number());
    assert!(content["loopTicks"].is_number());
    assert_eq!(content["autoLength"], true);
    assert_eq!(project["loopRegion"], Value::Null);

    assert_eq!(clip["content"]["type"], "notes");
    assert_eq!(clip["track"], track);
}

#[test]
fn editing_flow_through_rpc_changes_the_project() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track);
    let note = call(&table, c, "notes.add", json!({ "clip": clip["id"], "note": NOTE(64, 240) }));
    assert_eq!(note["pitch"], 64);
    let notes_of = |p: &Value| p["clips"][0]["content"]["notes"].as_array().unwrap().len();
    assert_eq!(notes_of(&project(&table, c)), 1);

    let status = call(&table, c, "project.status", Value::Null);
    assert_eq!(status["dirty"], true);
    assert_eq!(status["canUndo"], true);
    assert!(status["undoLabel"].is_string());
    assert!(status["path"].is_null(), "a new project is not saved yet");

    assert_eq!(call(&table, c, "history.undo", Value::Null), true);
    assert_eq!(notes_of(&project(&table, c)), 0);
    assert_eq!(call(&table, c, "history.redo", Value::Null), true);
    assert_eq!(notes_of(&project(&table, c)), 1);

    call(&table, c, "clips.setLoopRegion", json!({ "region": [0, 1920] }));
    assert_eq!(project(&table, c)["loopRegion"], json!([0, 1920]));
    call(&table, c, "clips.setLoopRegion", json!({ "region": null }));
    assert_eq!(project(&table, c)["loopRegion"], Value::Null);

    call(&table, c, "tempo.set", json!({ "bpm": 99.5 }));
    call(&table, c, "tempo.setSignature", json!({ "numerator": 3, "denominator": 4 }));
    call(&table, c, "graph.setVolume", json!({ "id": 0, "volume": 0.5 }));
    let project = project(&table, c);
    assert_eq!(project["tempoMap"]["tempos"][0]["bpm"], 99.5);
    assert_eq!(project["tempoMap"]["signatures"][0]["signature"]["numerator"], 3);
    assert_eq!(project["graph"]["nodes"][0]["kind"]["volume"], 0.5, "the master is the first node");
}

#[test]
fn clips_split_duplicate_and_remove_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = call(&table, c, "clips.addNote", json!({ "track": track, "startTick": 0, "lengthTicks": 1920 }));
    assert_eq!(clip["lengthTicks"], 1920);

    let right = call(&table, c, "clips.split", json!({ "id": clip["id"], "tick": 960 }));
    assert_eq!(right["startTick"], 960);
    let copy = call(&table, c, "clips.duplicate", json!({ "id": right["id"] }));
    assert_eq!(copy["startTick"], 1920, "the copy goes right after the original");
    call(&table, c, "clips.edit", json!({ "id": copy["id"], "edit": { "lengthTicks": 480 } }));
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 3);
    call(&table, c, "clips.remove", json!({ "id": copy["id"] }));
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 2);
}

#[test]
fn batch_clip_methods_and_track_move_work_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let a = add_synth(&table, c);
    let b = call(&table, c, "tracks.add", json!({ "name": "Bass", "kind": "synth", "defaultPitch": 36 }));
    let one = call(&table, c, "clips.addNote", json!({ "track": a, "startTick": 0, "lengthTicks": 1920 }));
    let two = call(&table, c, "clips.addNote", json!({ "track": a, "startTick": 1920, "lengthTicks": 1920 }));

    let swapped = call(
        &table,
        c,
        "clips.editMany",
        json!({ "changes": [
        { "id": one["id"], "edit": { "startTick": 1920 } },
        { "id": two["id"], "edit": { "startTick": 0, "gain": 0.5 } },
    ] }),
    );
    assert_eq!(swapped[0]["startTick"], 1920);
    assert_eq!((swapped[1]["startTick"].clone(), swapped[1]["gain"].clone()), (json!(0), json!(0.5)));
    assert!(table.call(c, "clips.editMany", json!({ "changes": [{ "id": one["id"], "edit": { "startTick": 0 } }] })).is_err());

    let copies = call(&table, c, "clips.copy", json!({ "ids": [one["id"], two["id"]], "atTick": 7680 }));
    assert_eq!(copies.as_array().unwrap().len(), 2);
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 4);
    assert!(table.call(c, "clips.copy", json!({ "ids": [], "atTick": 0 })).is_err());

    let ids: Vec<Value> = copies.as_array().unwrap().iter().map(|x| x["id"].clone()).collect();
    assert_eq!(call(&table, c, "clips.removeMany", json!({ "ids": ids })), Value::Null);
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 2);

    assert_eq!(call(&table, c, "tracks.move", json!({ "id": b, "index": 0 })), Value::Null);
    let order: Vec<Value> = project(&table, c)["tracks"].as_array().unwrap().iter().map(|t| t["id"].clone()).collect();
    assert_eq!(order, [b, a]);
    assert!(table.call(c, "tracks.move", json!({ "id": 9999, "index": 0 })).is_err());
}

#[test]
fn errors_carry_their_codes() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert!(table.call(c, "no.such", Value::Null).unwrap_err().is(codes::rpc::UNKNOWN_METHOD));
    assert!(table.call(c, "tempo.set", json!({ "bpm": "fast" })).unwrap_err().is(codes::rpc::BAD_PARAMS));
    assert!(table.call(c, "tempo.set", json!({ "bpm": 5.0 })).unwrap_err().is(codes::tempo::INVALID_TEMPO));
    assert!(table.call(c, "tracks.add", json!({ "name": "x", "kind": "nope", "defaultPitch": 60 })).is_err());
    assert!(table.call(c, "notes.remove", json!({ "clip": 99, "id": 1 })).is_err());

    let audio = call(&table, c, "tracks.add", json!({ "name": "Audio", "kind": null, "defaultPitch": 60 }));
    let on_audio = table.call(c, "clips.addNote", json!({ "track": audio, "startTick": 0, "lengthTicks": null })).unwrap_err();
    assert!(on_audio.is(codes::clips::NOTE_CLIP_NEEDS_INSTRUMENT_TRACK), "{on_audio}");

    let track = add_synth(&table, c);
    add_clip(&table, c, &track);
    let overlap = table.call(c, "clips.addNote", json!({ "track": track, "startTick": 100, "lengthTicks": null })).unwrap_err();
    assert!(overlap.is(codes::clips::NOTE_CLIPS_OVERLAP), "{overlap}");
}

#[test]
fn batch_note_methods_work_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track)["id"].clone();
    let note = |pitch: u32, start: u32| NOTE(pitch, start);
    let notes_in_clip = || project(&table, c)["clips"][0]["content"]["notes"].as_array().unwrap().len();

    let added = call(&table, c, "notes.addMany", json!({ "clip": clip, "notes": [note(60, 0), note(62, 240)] }));
    let ids: Vec<Value> = added.as_array().unwrap().iter().map(|n| n["id"].clone()).collect();
    assert_eq!(ids.len(), 2);

    let updated = call(
        &table,
        c,
        "notes.updateMany",
        json!({
            "clip": clip,
            "changes": [{ "id": ids[0], "note": note(61, 480) }, { "id": ids[1], "note": note(63, 720) }],
        }),
    );
    assert_eq!(updated[1]["startTick"], 720);
    let single = call(&table, c, "notes.update", json!({ "clip": clip, "id": ids[0], "note": note(65, 0) }));
    assert_eq!(single["pitch"], 65);

    call(&table, c, "notes.removeMany", json!({ "clip": clip, "ids": ids }));
    assert_eq!(notes_in_clip(), 0);
    assert!(table.call(c, "notes.removeMany", json!({ "clip": clip, "ids": [9999] })).is_err());

    let count = call(&table, c, "notes.replace", json!({ "clip": clip, "notes": [note(60, 0), note(64, 240), note(67, 480)] }));
    assert_eq!(count, 3);
    call(&table, c, "notes.clear", json!({ "clip": clip }));
    assert_eq!(notes_in_clip(), 0);

    call(&table, c, "notes.setLoopTicks", json!({ "clip": clip, "loopTicks": 3840 }));
    let content = project(&table, c)["clips"][0]["content"].clone();
    assert_eq!(content["loopTicks"], 3840);
    assert_eq!(content["autoLength"], false, "a manual length turns off the auto length");
}

#[test]
fn clips_add_note_creates_a_clip_with_notes_and_loop_in_one_undo_step() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = call(
        &table,
        c,
        "clips.addNote",
        json!({ "track": track, "startTick": 0, "lengthTicks": 7680, "notes": [NOTE(60, 0), NOTE(64, 480)], "loopTicks": 960 }),
    );
    let content = &clip["content"];
    assert_eq!((content["loopTicks"].as_u64(), content["autoLength"].as_bool()), (Some(960), Some(false)));
    assert_eq!(content["notes"].as_array().unwrap().len(), 2);
    assert_eq!(content["notes"][1]["startTick"], 480);

    let empty = call(&table, c, "clips.addNote", json!({ "track": track, "startTick": 7680 }));
    assert_eq!((empty["lengthTicks"].as_u64(), empty["content"]["notes"].as_array().map(Vec::len)), (Some(3840), Some(0)));

    let bad = table.call(c, "clips.addNote", json!({ "track": track, "startTick": 11520, "notes": [NOTE(200, 0)] }));
    assert!(bad.is_err(), "an invalid note is an error");
    assert_eq!(project(&table, c)["clips"].as_array().unwrap().len(), 2, "a clip with an invalid note is not created");

    call(&table, c, "history.undo", Value::Null);
    call(&table, c, "history.undo", Value::Null);
    assert!(project(&table, c)["clips"].as_array().unwrap().is_empty(), "one undo step per clip, notes included");

    let catalog = call(&table, c, "rpc.describe", Value::Null);
    let method = catalog.as_array().unwrap().iter().find(|m| m["name"] == "clips.addNote").unwrap();
    for key in ["track", "startTick", "lengthTicks", "notes", "loopTicks"] {
        assert!(method["params"]["properties"][key].is_object(), "schema without \"{key}\": {}", method["params"]);
    }
}

#[test]
fn clip_trim_and_processing_methods_work_through_rpc() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    let clip = call(&table, c, "clips.addNote", json!({ "track": track, "startTick": 0, "lengthTicks": 1920 }));
    let trimmed = call(&table, c, "clips.trimStart", json!({ "id": clip["id"], "startTick": 480 }));
    assert_eq!((trimmed["startTick"].clone(), trimmed["lengthTicks"].clone()), (json!(480), json!(1440)));

    assert_eq!(call(&table, c, "clips.normalize", json!({ "ids": [] })), json!([]));
    let not_audio = table.call(c, "clips.normalize", json!({ "ids": [clip["id"]], "peakDb": -1.0 })).unwrap_err();
    assert!(not_audio.is(codes::clips::NEEDS_AUDIO_CLIP), "{not_audio}");
    let shapes = call(&table, c, "clips.edit", json!({ "id": clip["id"], "edit": { "fadeInShape": "sCurve" } }));
    assert_eq!(shapes["fadeInShape"], "sCurve");
    assert_eq!(shapes["fadeOutShape"], "linear");
}

#[test]
fn checkpoint_and_squash_make_many_calls_one_undo_step() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let mark = call(&table, c, "history.checkpoint", Value::Null);
    assert!(mark.is_u64(), "the mark is a number: {mark}");
    let track = add_synth(&table, c);
    let clip = add_clip(&table, c, &track);
    call(&table, c, "notes.add", json!({ "clip": clip["id"], "note": NOTE(64, 240) }));

    assert_eq!(call(&table, c, "history.squash", json!({ "since": mark, "label": "AI: melody" })), true);
    assert_eq!(call(&table, c, "project.status", Value::Null)["undoLabel"], "AI: melody");
    assert_eq!(call(&table, c, "history.undo", Value::Null), true);
    assert!(project(&table, c)["tracks"].as_array().unwrap().iter().all(|t| t["id"] != track), "one undo removed the track");

    call(&table, c, "project.new", Value::Null);
    let gone = table.call(c, "history.squash", json!({ "since": mark, "label": "AI: melody" })).unwrap_err();
    assert!(gone.is(codes::history::CHECKPOINT_GONE), "{gone}");
}

#[test]
fn history_list_shows_done_and_undone_steps_and_changes_nothing() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let mark = call(&table, c, "history.checkpoint", Value::Null);
    let track = add_synth(&table, c);
    add_clip(&table, c, &track);
    call(&table, c, "history.squash", json!({ "since": mark, "label": "AI: intro" }));
    add_clip(&table, c, &track);
    assert_eq!(call(&table, c, "history.undo", Value::Null), true);
    let revision = call(&table, c, "project.status", Value::Null)["revision"].clone();

    let list = call(&table, c, "history.list", Value::Null);
    assert_eq!(list, json!({ "done": ["AI: intro"], "undone": ["Add clip"] }));
    assert_eq!(call(&table, c, "project.status", Value::Null)["revision"], revision, "reading changed nothing");
    assert_eq!(call(&table, c, "history.list", Value::Null), list);
}

#[test]
fn a_track_height_travels_in_the_project_json_only_while_it_is_set() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = add_synth(&table, c);
    assert!(project(&table, c)["tracks"][0].get("height").is_none(), "no own height: the field is left out");

    call(&table, c, "tracks.setHeight", json!({ "id": track, "height": 48 }));
    assert_eq!(project(&table, c)["tracks"][0]["height"], 48);
    call(&table, c, "tracks.setHeight", json!({ "id": track, "height": null }));
    assert!(project(&table, c)["tracks"][0].get("height").is_none());

    let error = table.call(c, "tracks.setHeight", json!({ "id": track, "height": 10 })).unwrap_err();
    assert!(error.is(codes::tracks::HEIGHT_OUT_OF_RANGE), "{error}");
}
