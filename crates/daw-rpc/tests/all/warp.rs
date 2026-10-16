//! Fitting an audio clip to the tempo and pitch via RPC: JSON shape, tempo guessing, turning it off, errors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

/// A silent WAV: 16 bit, mono, 48 kHz, `frames` frames (the header is written by hand to avoid dependencies).
fn write_silent_wav(path: &std::path::Path, frames: u32) {
    let data_len = frames * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(bytes.len() + data_len as usize, 0);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn warp_and_pitch_through_rpc() {
    let dir = std::env::temp_dir().join(format!("daw-rpc-warp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("loop 90bpm.wav");
    write_silent_wav(&file, 48_000);

    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    call(&table, c, "tempo.set", json!({ "bpm": 120.0 }));
    let sample = call(&table, c, "samples.import", json!({ "path": file }));
    let track = call(&table, c, "tracks.add", json!({ "name": "Loop", "kind": null, "defaultPitch": 60 }));
    let clip = call(&table, c, "clips.addAudio", json!({ "track": track, "startTick": 0, "sample": sample["id"] }));
    assert_eq!(clip["warp"], Value::Null);
    assert_eq!(clip["pitchSemitones"], 0.0);
    assert_eq!(clip["lengthTicks"], 1920);

    let warped = call(&table, c, "clips.edit", json!({ "id": clip["id"], "edit": { "warp": {}, "pitchSemitones": -2.5 } }));
    assert_eq!(warped["warp"], json!({ "sourceBpm": 90.0 }), "the tempo is guessed from the name");
    assert_eq!(warped["lengthTicks"], 1440, "one second at 90 BPM is one and a half beats");
    assert_eq!(warped["pitchSemitones"], -2.5);
    assert_eq!(project(&table, c)["clips"][0]["warp"]["sourceBpm"], 90.0);

    let set = call(&table, c, "clips.edit", json!({ "id": clip["id"], "edit": { "warp": { "sourceBpm": 120 } } }));
    assert_eq!((set["warp"]["sourceBpm"].as_f64(), set["lengthTicks"].as_u64()), (Some(120.0), Some(1920)));
    let off = call(&table, c, "clips.edit", json!({ "id": clip["id"], "edit": { "warp": null } }));
    assert_eq!(off["warp"], Value::Null);
    assert_eq!(off["pitchSemitones"], -2.5, "a field absent from the edit doesn't change");

    let error = table.call(c, "clips.edit", json!({ "id": clip["id"], "edit": { "pitchSemitones": 40 } })).unwrap_err();
    assert!(error.is(codes::clips::PITCH_OUT_OF_RANGE), "{error}");
    let synth = add_synth(&table, c);
    let notes = add_clip(&table, c, &synth);
    assert!(table.call(c, "clips.edit", json!({ "id": notes["id"], "edit": { "warp": {} } })).is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn takes_through_rpc() {
    let dir = std::env::temp_dir().join(format!("daw-rpc-takes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (first, second) = (dir.join("take 1.wav"), dir.join("take 2.wav"));
    write_silent_wav(&first, 48_000);
    write_silent_wav(&second, 48_000);

    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let one = call(&table, c, "samples.import", json!({ "path": first }))["id"].clone();
    let two = call(&table, c, "samples.import", json!({ "path": second }))["id"].clone();
    let track = call(&table, c, "tracks.add", json!({ "name": "Voice", "kind": null, "defaultPitch": 60 }));
    let clip = call(&table, c, "clips.addAudio", json!({ "track": track, "startTick": 0, "sample": one }));
    assert_eq!(clip["content"], json!({ "type": "audio", "sample": one }), "without takes the list is not written");
    c.takes.add_take(clip["id"].as_u64().unwrap() as u32, two.as_u64().unwrap() as u32).unwrap(); // this is how recording adds it

    let back = call(&table, c, "clips.setActiveTake", json!({ "id": clip["id"], "take": one }));
    assert_eq!(back["content"], json!({ "type": "audio", "sample": one, "takes": [one, two] }));
    assert!(table.call(c, "clips.setActiveTake", json!({ "id": clip["id"], "take": 9999 })).is_err());
    call(&table, c, "history.undo", Value::Null);
    assert_eq!(project(&table, c)["clips"][0]["content"]["sample"], two, "choosing a take is undone");

    // Comp: a piece of the first take in the middle of the active second one, one undo step; invalid segments give a clear error.
    let comped = call(&table, c, "clips.chooseTakeRange", json!({ "id": clip["id"], "from": 480, "to": 960, "take": one }));
    assert_eq!(
        comped["content"]["comp"],
        json!([{ "startTicks": 0, "take": two }, { "startTicks": 480, "take": one }, { "startTicks": 960, "take": two }])
    );
    assert_eq!(project(&table, c)["clips"][0]["content"]["comp"], comped["content"]["comp"]);
    call(&table, c, "history.undo", Value::Null);
    assert_eq!(project(&table, c)["clips"][0]["content"].get("comp"), None, "the piece is undone in one step");
    let whole = json!({ "id": clip["id"], "segments": [{ "startTicks": 0, "take": two }, { "startTicks": 960, "take": one }] });
    let set = call(&table, c, "clips.setComp", whole);
    assert_eq!(set["content"]["comp"][1], json!({ "startTicks": 960, "take": one }));
    let error = table.call(c, "clips.setComp", json!({ "id": clip["id"], "segments": [{ "startTicks": 5, "take": one }] })).unwrap_err();
    assert!(error.is(codes::clips::COMP_NOT_FROM_START), "{error}");
    let cleared = call(&table, c, "clips.setActiveTake", json!({ "id": clip["id"], "take": one }));
    assert_eq!(cleared["content"].get("comp"), None, "a whole take clears the comp");

    let removed = call(&table, c, "clips.removeTake", json!({ "id": clip["id"], "take": two }));
    assert_eq!(removed["content"], json!({ "type": "audio", "sample": one }), "one sound left: a plain clip");
    assert!(table.call(c, "clips.removeTake", json!({ "id": clip["id"], "take": one })).is_err(), "the only sound can't be removed");
    std::fs::remove_dir_all(dir).ok();
}
