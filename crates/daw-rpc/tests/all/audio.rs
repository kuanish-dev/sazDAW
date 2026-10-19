//! `audio.*` and `audioRecord.*` methods on a host without devices.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn audio_methods_report_devices_status_and_keep_the_choice_on_an_empty_host() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);

    let devices = call(&table, c, "audio.devices", Value::Null);
    assert!(devices["outputs"].is_array() && devices["inputs"].is_array(), "{devices}");
    assert_eq!(
        call(&table, c, "audio.settings", Value::Null),
        json!({ "output": null, "input": null, "bufferFrames": null, "recordLatencyMs": null })
    );
    let status = call(&table, c, "audio.status", Value::Null);
    assert_eq!(status["running"], false);
    for key in ["outputName", "sampleRate", "channels", "bufferRange", "error"] {
        assert!(status.get(key).is_some(), "the status has no {key}: {status}");
    }

    // The default choice is accepted and remembered; a missing device is refused and the choice stays.
    let defaults = json!({ "output": null, "input": null, "bufferFrames": null, "recordLatencyMs": null });
    assert_eq!(call(&table, c, "audio.apply", defaults.clone())["running"], false);
    let refused =
        table.call(c, "audio.apply", json!({ "output": "No such device", "input": null, "bufferFrames": 256, "recordLatencyMs": null }));
    assert!(refused.unwrap_err().params().iter().any(|(_, value)| value == "No such device"));
    assert_eq!(call(&table, c, "audio.settings", Value::Null), defaults);
    let latency = json!({ "output": null, "input": null, "bufferFrames": null, "recordLatencyMs": 12.5 });
    call(&table, c, "audio.apply", latency.clone());
    assert_eq!(call(&table, c, "audio.settings", Value::Null), latency);

    // Without devices the input cannot be opened but can be closed; the level is zero.
    assert!(table.call(c, "audio.watchInput", json!({ "on": true })).is_err());
    assert_eq!(call(&table, c, "audio.watchInput", json!({ "on": false })), Value::Null);
    assert_eq!(call(&table, c, "audio.inputLevel", Value::Null), json!({ "left": 0.0, "right": 0.0 }));
}

#[test]
fn the_master_window_is_captured_from_the_first_read_and_answers_as_bytes_too() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let window = call(&table, c, "audio.masterWindow", Value::Null);
    assert_eq!(window["samples"], json!([]), "the first read starts the capture");
    assert!(window["sampleRate"].is_u64(), "{window}");

    let window = call(&table, c, "audio.masterWindow", Value::Null);
    assert_eq!(window["samples"].as_array().unwrap().len(), 2048, "MASTER_WINDOW_FRAMES");
    assert!(window["samples"].as_array().unwrap().iter().all(Value::is_f64), "plain numbers");

    let bytes = table.call_binary(c, "audio.masterWindow", Value::Null).unwrap();
    assert_eq!(bytes.len(), 8 + 4 * 2048, "the rate, the count and the samples");
    let refused = table.call_binary(c, "audio.status", Value::Null).unwrap_err();
    assert!(refused.is(daw_contracts::codes::rpc::NO_BINARY_REPLY), "{refused}");
}

#[test]
fn the_master_meter_starts_on_reading_and_the_mono_check_shows_in_the_tick() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let reading = call(&table, c, "audio.masterMeter", Value::Null);
    assert_eq!((&reading["seconds"], &reading["integratedLufs"]), (&json!(0.0), &Value::Null), "a fresh measurement");
    for key in [
        "momentaryLufs",
        "shortTermLufs",
        "integratedLufs",
        "rangeLu",
        "truePeakDb",
        "maxTruePeakDb",
        "clipped",
        "correlation",
        "seconds",
        "vector",
        "sampleRate",
    ] {
        assert!(reading.get(key).is_some(), "the reading has no {key}: {reading}");
    }
    assert_eq!(call(&table, c, "audio.resetMasterMeter", Value::Null), Value::Null);

    let mut telemetry = daw_rpc::Telemetry::new();
    assert_eq!(telemetry.poll(c).tick["mono"], false, "stereo at the start");
    assert_eq!(call(&table, c, "audio.setMono", json!({ "on": true })), Value::Null);
    assert_eq!(telemetry.poll(c).tick["mono"], true);
    call(&table, c, "audio.setMono", json!({ "on": false }));
    assert_eq!(telemetry.poll(c).tick["mono"], false);
}

#[test]
fn node_windows_follow_the_nodes_read_and_the_reader_cursors() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert_eq!(call(&table, c, "audio.nodeWindows", json!({ "nodes": [] })), json!([]), "no node asked for");

    let windows = call(&table, c, "audio.nodeWindows", json!({ "nodes": [0, 5, 0], "seen": [] }));
    let windows = windows.as_array().unwrap();
    assert_eq!(windows.iter().map(|w| w["node"].clone()).collect::<Vec<_>>(), [json!(0), json!(5)], "a repeated id counts once");
    for window in windows {
        assert!(window["watch"].is_u64() && window["end"].is_u64() && window["sampleRate"].is_u64(), "{window}");
        for lane in ["input", "output"] {
            assert_eq!(window[lane].as_array().unwrap().len(), 4096, "{lane}: NODE_WINDOW_FRAMES");
        }
    }

    // With nothing rendered since, a cursor at the window's end gets no frames.
    let seen = json!([{ "node": 0, "watch": windows[0]["watch"], "end": windows[0]["end"] }]);
    let next = call(&table, c, "audio.nodeWindows", json!({ "nodes": [0, 5], "seen": seen }));
    assert_eq!((&next[0]["input"], &next[0]["output"]), (&json!([]), &json!([])));
    assert_eq!(next[1]["input"].as_array().unwrap().len(), 4096, "node 5 has no cursor: a whole window");

    let bytes = table.call_binary(c, "audio.nodeWindows", json!({ "nodes": [5] })).unwrap();
    assert_eq!(bytes.len(), 4 + 24 + 2 * 4 * 4096, "one window: its header and both lanes");
}

#[test]
fn audio_record_on_an_empty_host_fails_cleanly_and_reports_idle() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let idle = json!({
        "recording": false, "full": false, "seconds": 0.0, "left": 0.0, "right": 0.0, "droppedFrames": 0,
        "tracks": [], "monitorProblem": null
    });
    assert_eq!(call(&table, c, "audioRecord.status", Value::Null), idle);

    let error = table.call(c, "audioRecord.start", Value::Null).unwrap_err();
    assert!(error.is(codes::audio::NO_ARMED_TRACKS), "{error}");
    let track = call(&table, c, "tracks.add", json!({ "name": "Voice", "kind": null, "defaultPitch": 60 }));
    let synth = add_synth(&table, c);
    assert!(table.call(c, "audioRecord.setArmed", json!({ "track": synth, "armed": true })).is_err(), "audio tracks only");
    assert!(table.call(c, "audioRecord.setArmed", json!({ "track": 9999, "armed": true })).is_err());
    call(&table, c, "audioRecord.setArmed", json!({ "track": track, "armed": true }));
    call(&table, c, "audioRecord.setInput", json!({ "track": track, "input": "left" }));
    assert!(table.call(c, "audioRecord.setInput", json!({ "track": track, "input": "3-4" })).is_err(), "unknown input");
    let status = call(&table, c, "audioRecord.status", Value::Null);
    assert_eq!(status["tracks"], json!([{ "track": track, "armed": true, "input": "left", "left": 0.0, "right": 0.0 }]));

    // No project file: start refuses instead of staying silent.
    let error = table.call(c, "audioRecord.start", Value::Null).unwrap_err();
    assert!(error.is(codes::audio::PROJECT_NOT_SAVED), "{error}");
    assert!(table.call(c, "audioRecord.stop", Value::Null).is_err(), "nothing to stop");
    assert_eq!(call(&table, c, "audioRecord.status", Value::Null)["recording"], false);
}

#[test]
fn monitor_mode_is_set_on_the_audio_track_node_and_undone() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let track = call(&table, c, "tracks.add", json!({ "name": "Voice", "kind": null, "defaultPitch": 60 }));
    let node = |p: &Value| p["graph"]["nodes"].as_array().unwrap().iter().find(|n| n["kind"]["track"] == track).unwrap()["kind"].clone();
    assert_eq!(node(&project(&table, c))["monitor"], "off");
    call(&table, c, "tracks.setMonitor", json!({ "track": track, "mode": "auto" }));
    assert_eq!(node(&project(&table, c))["monitor"], "auto");
    assert!(table.call(c, "tracks.setMonitor", json!({ "track": track, "mode": "loud" })).is_err());
    call(&table, c, "history.undo", Value::Null);
    assert_eq!(node(&project(&table, c))["monitor"], "off");
}
