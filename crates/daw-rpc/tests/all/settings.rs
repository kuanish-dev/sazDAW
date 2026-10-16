//! The Settings page: user preferences (`preferences.*`) and the list of installed plugins (`plugins.list`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: a failure with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn preferences_set_returns_the_stored_value_and_rejects_invalid_ones() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let wanted = json!({
        "profile": { "author": "  Anya ", "projectsFolder": "/tmp/Projects", "openLastProject": true },
        "autosaveSeconds": 0,
        "appearance": { "theme": "dark", "accent": "purple", "uiScale": 1.25, "trackHeight": "compact", "showHints": false }
    });
    let saved = call(&table, c, "preferences.set", json!({ "preferences": wanted }));
    assert_eq!(saved["profile"]["author"], "Anya", "the author is trimmed");
    assert_eq!(saved["autosaveSeconds"], 0);
    assert_eq!(saved["appearance"]["accent"], "purple");
    assert_eq!(saved["language"], "en", "language defaults to English");
    assert_eq!(call(&table, c, "preferences.get", Value::Null), saved);

    let mut bad = saved.clone();
    bad["autosaveSeconds"] = json!(5);
    let error = table.call(c, "preferences.set", json!({ "preferences": bad })).unwrap_err();
    assert!(error.is(codes::settings::AUTOSAVE_OUT_OF_RANGE), "{error}");
    bad = saved.clone();
    bad["appearance"]["uiScale"] = json!(2.0);
    assert!(table.call(c, "preferences.set", json!({ "preferences": bad })).unwrap_err().is(codes::settings::UI_SCALE_OUT_OF_RANGE));
    bad = saved.clone();
    bad["appearance"]["theme"] = json!("pink");
    assert!(table.call(c, "preferences.set", json!({ "preferences": bad })).is_err(), "unknown theme");
    bad = saved.clone();
    bad["language"] = json!("de");
    assert!(table.call(c, "preferences.set", json!({ "preferences": bad })).is_err(), "unknown language");
    assert_eq!(call(&table, c, "preferences.get", Value::Null), saved, "a refusal changes nothing");
}

#[test]
fn plugins_list_has_the_agreed_shape() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    let list = call(&table, c, "plugins.list", Value::Null);
    assert_eq!((list["folders"].clone(), list["plugins"].clone()), (json!([]), json!([])), "the test session has no plugins");

    // The dev-only fixture synth (`test-plugins/synth`), when built, shows the shape of one entry.
    let Some(dir) = fixture_synth_dir() else { return };
    let session = daw_bootstrap::build_headless_session_in(&fixture_config("list", dir));
    let c = &session.controls;
    call(&table, c, "plugins.scan", Value::Null);
    let list = call(&table, c, "plugins.list", Value::Null);
    let synth = list["plugins"].as_array().unwrap().iter().find(|p| p["kind"] == FIXTURE_SYNTH).expect("the fixture synth is found");
    for key in ["name", "vendor", "path", "type", "params", "paramsTotal", "disabled"] {
        assert!(synth.get(key).is_some(), "the plugin has no \"{key}\": {synth}");
    }
    assert_eq!(synth["type"].as_str(), Some("instrument"));
    assert!(synth["params"].as_u64().unwrap() > 0 && synth["paramsTotal"].as_u64() >= synth["params"].as_u64());
}

const FIXTURE_SYNTH: &str = "vst3:synth";

/// Where `scripts/build-test-plugins.sh` puts the fixture synth; `None` (with a message) when it is not built.
#[allow(clippy::print_stderr, reason = "a skipped test says why")]
fn fixture_synth_dir() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-plugins/synth/target/bundled"));
    if dir.join("Synth.vst3").exists() {
        Some(dir)
    } else {
        eprintln!("SKIPPED: no fixture Synth.vst3 in {} (build it: scripts/build-test-plugins.sh)", dir.display());
        None
    }
}

/// A headless session config whose plugin scan sees only `plugin_dir`.
fn fixture_config(name: &str, plugin_dir: std::path::PathBuf) -> daw_bootstrap::SessionConfig {
    let state_dir = std::env::temp_dir().join(format!("daw-rpc-fixture-{name}-{}", std::process::id()));
    daw_bootstrap::SessionConfig { state_dir, plugin_dirs: Some(vec![plugin_dir]) }
}

#[test]
fn a_disabled_plugin_leaves_the_catalog_is_remembered_and_a_project_with_it_opens_with_a_warning() {
    let Some(plugin_dir) = fixture_synth_dir() else { return };
    let dir = std::env::temp_dir().join(format!("daw-rpc-disabled-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let file = dir.join("Song/Song.dawproj");
    let state = daw_bootstrap::SessionConfig { state_dir: dir.join("state"), plugin_dirs: Some(vec![plugin_dir]) };
    let table = RpcTable::new();
    let synth = FIXTURE_SYNTH;
    let in_catalog = |c: &daw_contracts::Controls| {
        call(&table, c, "catalog.get", Value::Null)["instruments"].as_array().unwrap().iter().any(|k| k["kind"] == synth)
    };

    let session = daw_bootstrap::build_headless_session_in(&state);
    let c = &session.controls;
    call(&table, c, "plugins.scan", Value::Null);
    assert!(in_catalog(c), "the fixture synth is in the catalog after a scan");
    call(&table, c, "tracks.add", json!({ "name": "Synth", "kind": synth, "defaultPitch": 60 }));
    call(&table, c, "project.save", json!({ "path": file }));
    call(&table, c, "plugins.setDisabled", json!({ "kind": synth, "disabled": true }));
    assert!(!in_catalog(c), "a disabled plugin is not offered");
    let listed = call(&table, c, "plugins.list", Value::Null);
    let entry = listed["plugins"].as_array().unwrap().iter().find(|p| p["kind"] == synth).cloned().unwrap();
    assert_eq!(entry["disabled"], true, "but it stays in the installed list, marked");
    assert_eq!(call(&table, c, "preferences.get", Value::Null)["disabledPlugins"], json!([synth]), "the list is in the preferences");
    let error = table.call(c, "plugins.setDisabled", json!({ "kind": "vst3:no_such", "disabled": true })).unwrap_err();
    assert!(error.is(codes::plugins::NOT_FOUND), "{error}");
    drop(session);

    // Next run: the disabled list is the same; a project using the plugin opens without it and with a warning.
    let session = daw_bootstrap::build_headless_session_in(&state);
    let c = &session.controls;
    call(&table, c, "plugins.scan", Value::Null);
    assert!(!in_catalog(c), "disabling survived the restart");
    call(&table, c, "project.load", json!({ "path": file }));
    let text = call(&table, c, "project.takeWarnings", Value::Null).to_string();
    assert!(text.contains("is disabled"), "{text}");
    assert_eq!(project(&table, c)["tracks"].as_array().unwrap().len(), 1, "the track is in place");

    call(&table, c, "plugins.setDisabled", json!({ "kind": synth, "disabled": false }));
    assert!(in_catalog(c), "enabled again, it is back in the catalog");
    drop(session);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_test_session_has_no_plugin_crash_to_report() {
    let session = build_session(false);
    let (table, c) = (RpcTable::new(), &session.controls);
    assert_eq!(call(&table, c, "plugins.lastCrash", Value::Null), Value::Null);
    assert_eq!(call(&table, c, "plugins.dismissCrash", Value::Null), Value::Null);
}
