//! The method table: kind catalog, plugin scan, group completeness and the `rpc.describe` self-description.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn plugins_scan_reports_found_and_failed_in_the_agreed_shape() {
    // The test session: the scanner is empty and doesn't depend on the plugins on the machine.
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert_eq!(call(&table, c, "plugins.scan", Value::Null), json!({ "found": 0, "effects": 0, "instruments": 0, "failed": [] }));
}

#[test]
fn saving_and_rendering_methods_let_the_plugin_windows_hand_over_their_state_first() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Windows(AtomicUsize);
    impl daw_contracts::PluginStateFlush for Windows {
        fn flush(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let session = build_session(false);
    let c = &session.controls;
    let windows = Arc::new(Windows(AtomicUsize::new(0)));
    let mut table = RpcTable::new();
    table.set_plugin_windows(windows.clone());
    call(&table, c, "project.get", Value::Null);
    assert_eq!(windows.0.load(Ordering::SeqCst), 0, "reading the project does not disturb the windows");
    // The windows are asked before the method runs, so even a save refused by the rules asked them.
    assert!(table.call(c, "templates.save", json!({ "name": "" })).is_err());
    assert_eq!(windows.0.load(Ordering::SeqCst), 1);
}

#[test]
fn every_documented_method_group_is_registered() {
    let table = RpcTable::new();
    let methods = table.methods();
    for expected in [
        "transport.play",
        "transport.seek",
        "transport.setCue",
        "transport.setSongLoop",
        "tempo.get",
        "tempo.set",
        "tempo.setSignature",
        "tempo.setChanges",
        "tempo.setSignatureChanges",
        "midi.importTempo",
        "project.get",
        "project.changes",
        "project.summary",
        "project.export",
        "history.undo",
        "history.list",
        "chords.strums",
        "chords.build",
        "chords.preview",
        "chords.stopPreview",
        "history.checkpoint",
        "history.squash",
        "tracks.add",
        "tracks.addMany",
        "tracks.remove",
        "tracks.rename",
        "tracks.setInstrument",
        "tracks.setParam",
        "tracks.move",
        "tracks.setColor",
        "tracks.setHeight",
        "markers.add",
        "markers.edit",
        "markers.remove",
        "folders.create",
        "folders.edit",
        "folders.remove",
        "folders.setTracks",
        "clips.addNote",
        "clips.addAudio",
        "clips.edit",
        "clips.split",
        "clips.duplicate",
        "clips.remove",
        "clips.editMany",
        "clips.removeMany",
        "clips.copy",
        "clips.setLoopRegion",
        "notes.add",
        "notes.update",
        "notes.remove",
        "notes.clear",
        "notes.addMany",
        "notes.updateMany",
        "notes.removeMany",
        "notes.replace",
        "notes.setLoopTicks",
        "notes.quantize",
        "notes.humanize",
        "notes.transpose",
        "notes.fitToScale",
        "notes.legato",
        "notes.groove",
        "notes.grooveFromClip",
        "notes.vary",
        "notes.varyToNewClip",
        "graph.connect",
        "graph.addBus",
        "graph.addEffect",
        "graph.setEffectParam",
        "graph.insertEffect",
        "graph.insertEffectBefore",
        "graph.removeEffectKeepPath",
        "graph.setOutput",
        "graph.setEdgePreFader",
        "graph.setSidechain",
        "graph.setPolarity",
        "graph.setWidth",
        "graph.setMono",
        "graph.setDelay",
        "stage.place",
        "stage.takeOff",
        "stage.room",
        "stage.setRoom",
        "automation.addClip",
        "automation.editClip",
        "automation.duplicateClip",
        "automation.removeClip",
        "automation.setPoints",
        "automation.drawShape",
        "automation.setWriteMode",
        "automation.touch",
        "samples.import",
        "library.home",
        "midi.exportProject",
        "midi.exportClip",
        "midi.importInto",
        "midi.connect",
        "midi.setTarget",
        "midi.learn",
        "midi.cancelLearn",
        "midi.mappings",
        "midi.removeMapping",
        "notes.setControllerLane",
        "live.noteOn",
        "live.noteOff",
        "live.controller",
        "record.setArmed",
        "audio.devices",
        "audio.settings",
        "audio.status",
        "audio.apply",
        "audio.watchInput",
        "audio.inputLevel",
        "audio.masterWindow",
        "audio.masterMeter",
        "audio.resetMasterMeter",
        "audio.setMono",
        "audio.nodeWindows",
        "audioRecord.setArmed",
        "audioRecord.setInput",
        "audioRecord.start",
        "audioRecord.stop",
        "audioRecord.status",
        "tracks.setMonitor",
        "clips.setActiveTake",
        "clips.removeTake",
        "clips.setComp",
        "clips.chooseTakeRange",
        "clips.trimStart",
        "clips.normalize",
        "clips.reverse",
        "plugins.scan",
        "plugins.list",
        "plugins.setDisabled",
        "plugins.lastCrash",
        "plugins.dismissCrash",
        "preferences.get",
        "preferences.set",
        "patterns.list",
        "patterns.get",
        "drums.build",
        "drums.preview",
        "drums.stopPreview",
    ] {
        assert!(methods.contains(&expected), "no method {expected}");
    }
    let count = |group: &str| methods.iter().filter(|m| m.starts_with(group)).count();
    for removed_group in ["channels.", "playlist.", "mixer."] {
        assert_eq!(count(removed_group), 0, "group {removed_group} must be gone");
    }
    assert!(!methods.contains(&"midi.exportPattern"));
    assert!(!methods.contains(&"library.list"), "no whole-disk browser: sounds are browsed only in the project and linked folders");
}

#[test]
fn rpc_describe_lists_every_method_with_description_and_parameter_schema() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let catalog = call(&table, c, "rpc.describe", Value::Null);
    let methods = catalog.as_array().unwrap();
    // All registered methods except `rpc.describe` itself, which builds this catalog.
    assert_eq!(methods.len(), table.methods().len() - 1);
    let add = methods.iter().find(|m| m["name"] == "tracks.add").unwrap();
    assert!(add["description"].as_str().unwrap().contains("track"));
    assert_eq!(add["params"]["type"], "object");
    assert!(add["params"]["properties"]["defaultPitch"].is_object(), "{}", add["params"]);
}

/// Answers that used to be built as separate DTOs are now the contract types themselves; the JSON shape for the interface is the same.
#[test]
fn wire_shapes_of_contract_types_stay_as_the_frontend_expects() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    let catalog = call(&table, c, "catalog.get", Value::Null);
    let kinds: Vec<_> = catalog["instruments"].as_array().unwrap().iter().map(|k| k["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "kick",
            "snare",
            "hat",
            "clap",
            "openhat",
            "tom_hi",
            "tom_mid",
            "tom_lo",
            "crash",
            "ride",
            "rim",
            "sampler",
            "synth",
            "guitar",
            "electric_guitar",
            "ukulele",
            "bass_guitar"
        ]
    );
    assert_eq!(catalog["effects"].as_array().unwrap().len(), 10);
    let param = &catalog["instruments"][0]["params"][0];
    for key in ["name", "label", "min", "max", "default", "kind"] {
        assert!(param.get(key).is_some(), "the param has no \"{key}\": {param}");
    }
    let kinds: Vec<&str> = catalog["instruments"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|k| k["params"].as_array().unwrap())
        .filter_map(|p| p["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"number") && kinds.contains(&"sample") && kinds.contains(&"choice"), "{kinds:?}");
    let synth = catalog["instruments"].as_array().unwrap().iter().find(|k| k["kind"] == "synth").unwrap();
    let waveform = synth["params"].as_array().unwrap().iter().find(|p| p["label"] == "Waveform").unwrap();
    assert_eq!(waveform["choices"], serde_json::json!(["Sine", "Triangle", "Saw", "Square"]), "{waveform}");
    assert!(synth["params"].as_array().unwrap().iter().any(|p| p["label"] == "High cut, Hz" && p["max"].as_f64() > Some(17_000.0)));
    assert!(param.get("choices").is_none(), "a non-choice has no options in JSON: {param}");

    let status = call(&table, c, "project.status", Value::Null);
    for key in ["name", "dirty", "canUndo", "canRedo", "undoLabel", "revision", "path", "folder"] {
        assert!(status.get(key).is_some(), "project.status without \"{key}\": {status}");
    }
    assert!(
        table
            .call(
                c,
                "project.export",
                json!({ "path": "/tmp/x.wav", "sampleRate": 44100, "bitDepth": "int64", "stems": false, "tailSeconds": 1.0 })
            )
            .is_err()
    );
}
