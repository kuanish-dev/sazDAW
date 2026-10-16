//! Session plugins: nothing is scanned or loaded at startup, a scan by the button is remembered on disk. The plugins
//! actually loaded here are the dev-only fixtures (`test-plugins/`, built by `scripts/build-test-plugins.sh`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: failing with a clear message is the check itself"
)]

use std::path::PathBuf;

use daw_bootstrap::{SessionConfig, build_session, build_session_in};
use daw_contracts::codes::plugins as codes;
use daw_contracts::{Controls, Params, SYNTH_KIND, StreamConfig};

const DISTORTION: &str = "vst3:distortion";
/// Where `scripts/build-test-plugins.sh` puts the fixture distortion.
const DISTORTION_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-plugins/distortion/target/bundled");

fn temp_state(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daw-boot-plugins-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn has_kind(controls: &Controls, kind: &str) -> bool {
    controls.effects.kinds().iter().any(|k| k.kind == kind)
}

fn distortion_built() -> bool {
    let built = PathBuf::from(DISTORTION_DIR).join("Distortion.vst3").exists();
    if !built {
        eprintln!("SKIPPED: no fixture Distortion.vst3 in {DISTORTION_DIR} (build it: scripts/build-test-plugins.sh)");
    }
    built
}

#[test]
fn the_builtin_synth_is_an_instrument_not_a_plugin() {
    let session = build_session(false);
    let synth = session.controls.instruments.kinds().into_iter().find(|k| k.kind == SYNTH_KIND).expect("the built-in synth");
    assert_eq!(synth.label.get(), "Synth");
    assert_eq!(synth.params.len(), 30, "oscillators, unison, AHDSR, filter, pitch envelope, mono, LFO, gain");
    assert!(session.controls.plugins.installed().is_empty(), "nothing in the plugin list without a scan");
}

#[test]
fn test_sessions_have_an_empty_scanner_and_no_vst3_in_the_catalog() {
    let session = build_session(false);
    let report = session.controls.plugins.scan().unwrap();
    assert_eq!((report.found, report.failed.len()), (0, 0));
    assert!(session.controls.effects.kinds().iter().all(|k| !k.kind.starts_with("vst3:")));
}

#[test]
fn startup_does_not_scan_and_a_missing_cache_means_no_plugins() {
    let state = temp_state("startup");
    let session = build_session_in(&SessionConfig::new(state.clone()));
    assert!(session.controls.effects.kinds().iter().all(|k| !k.kind.starts_with("vst3:")), "no plugins without the button");
    assert!(!state.join("plugins.json").exists());
    std::fs::remove_dir_all(state).ok();
}

#[test]
fn startup_takes_plugins_from_the_cache_without_loading_their_files() {
    let state = temp_state("cache");
    std::fs::create_dir_all(&state).unwrap();
    // There is no plugin file at this path: if startup tried to load it, a failure would appear in the log.
    std::fs::write(
        state.join("plugins.json"),
        r#"{"plugins":[{"path":"/missing/Ghost.vst3","class_id":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16],
            "name":"Ghost","vendor":"Nobody","kind":"Effect","params":[{"id":1,"label":"Amount","shape":"Number","choices":[],"min":0.0,"max":1.0,"default":0.5,"default_normalized":0.5}],"params_total":1,"bypass_param":null},
           {"path":"/missing/Phantom.vst3","class_id":[16,15,14,13,12,11,10,9,8,7,6,5,4,3,2,1],
            "name":"Phantom Synth","vendor":"Nobody","kind":"Instrument","params":[{"id":2,"label":"Cutoff, Hz","shape":"Number","choices":[],"min":40.0,"max":18000.0,"default":1000.0,"default_normalized":0.1}],"params_total":500,"bypass_param":null}]}"#,
    )
    .unwrap();
    let session = build_session_in(&SessionConfig::new(state.clone()));
    let controls = &session.controls;
    assert!(has_kind(controls, "vst3:ghost"));
    assert_eq!(controls.effects.defaults("vst3:ghost").unwrap().len(), 1);
    let synths = controls.instruments.kinds();
    let phantom =
        synths.iter().find(|k| k.kind == "vst3:phantom_synth").expect("an instrument from the cache is in the instrument catalog");
    assert_eq!((phantom.label.get(), phantom.params.len()), ("Phantom Synth (VST3)", 1));
    assert!(!has_kind(controls, "vst3:phantom_synth"), "the instrument did not get into the effects");
    assert_eq!(controls.instruments.defaults("vst3:phantom_synth").unwrap()["p2"], 1000.0);
    assert!(controls.log.recent(10).is_empty(), "the plugin was not opened");

    let error = controls.effects.create("vst3:ghost", &Params::new(), None).err().expect("the plugin is not on disk");
    assert!(error.is(codes::LOAD_FAILED), "{error}");
    let error = controls.instruments.create("vst3:phantom_synth", &Params::new(), None).err().expect("the plugin is not on disk");
    assert!(error.is(codes::LOAD_FAILED), "{error}");
    assert_eq!(controls.log.recent(10).len(), 2, "and this went to the log");
    std::fs::remove_dir_all(state).ok();
}

#[test]
fn scanning_finds_distortion_remembers_it_and_the_next_start_restores_it() {
    if !distortion_built() {
        return;
    }
    let state = temp_state("scan");
    let config = || SessionConfig { state_dir: state.clone(), plugin_dirs: Some(vec![PathBuf::from(DISTORTION_DIR)]) };
    let session = build_session_in(&config());
    let controls = &session.controls;
    assert!(!has_kind(controls, DISTORTION), "no plugin in the catalog before the button");

    // The scan runs in the `daw-plugin-scan` helper (a separate process); `cargo test --workspace` or `cargo build` builds it.
    let report = match controls.plugins.scan() {
        Err(error) if error.is(codes::SCAN_HELPER_MISSING) => return eprintln!("SKIPPED: {error}"),
        result => result.unwrap(),
    };
    assert!(report.found >= 1, "{report:?}");
    assert!(has_kind(controls, DISTORTION), "the effect appeared in the catalog");
    let mut effect = controls.effects.create(DISTORTION, &Params::new(), None).unwrap();
    effect.prepare(StreamConfig { sample_rate: 48_000, channels: 2 });
    let mut buffer: Vec<f32> = (0..512).map(|n| (n as f32 * 0.1).sin() * 0.5).collect();
    for _ in 0..10 {
        effect.process(&mut buffer, None);
    }
    assert!(buffer.iter().all(|v| v.is_finite()));

    assert!(state.join("plugins.json").is_file(), "the list is remembered");
    let again = build_session_in(&config());
    assert!(has_kind(&again.controls, DISTORTION), "after a restart the plugin is there without a new scan");
    assert!(again.controls.effects.create(DISTORTION, &Params::new(), None).is_ok());
    drop(effect);
    std::fs::remove_dir_all(state).ok();
}

#[test]
fn builtin_instruments_and_effects_have_no_plugin_window() {
    use daw_contracts::EditorTarget;

    let session = build_session(true);
    let track = session.controls.tracks.tracks().into_iter().find(|t| t.instrument.is_some()).expect("the demo has an instrument");
    let error = session.controls.plugin_editors.open(EditorTarget::Instrument { track: track.id }).unwrap_err();
    assert!(error.is(codes::BUILTIN_INSTRUMENT_HAS_NO_WINDOW), "{error}");
    let effect = session.controls.graph.add_effect("reverb", &Params::new(), 0.0, 0.0).unwrap();
    let error = session.controls.plugin_editors.open(EditorTarget::Effect { node: effect }).unwrap_err();
    assert!(error.is(codes::BUILTIN_EFFECT_HAS_NO_WINDOW), "{error}");
}

/// The previous session "crashed" inside a plugin: the next start reports it and does NOT open the last project (the
/// same plugin would crash the app again) until the user answers the report.
#[test]
fn after_a_crash_inside_a_plugin_the_last_project_is_not_reopened_until_the_report_is_dismissed() {
    use daw_contracts::LogEntry;
    let state = temp_state("crash");
    let file = state.join("Song/Song.dawproj");
    let first = daw_bootstrap::build_headless_session_in(&SessionConfig::new(state.clone()));
    first.controls.project.create(&file).unwrap();
    first.controls.plugin_crash.end_session();
    let (recent, storage, log) = (first.controls.recent.clone(), first.controls.storage.clone(), first.controls.log.clone());
    drop(first);

    // A session that broke off in the middle of plugin processing: the mark is set and not cleared, there was no normal exit.
    let guard = daw_vst3::Vst3CrashGuard::open(&state.join("plugin-guard.bin"), &*recent, &*storage, log);
    let slot = guard.marker().claim("vst3:synth", "Synth").unwrap();
    std::mem::forget(slot.enter());
    drop((slot, guard));

    let session = daw_bootstrap::build_headless_session_in(&SessionConfig::new(state.clone()));
    let controls = &session.controls;
    let crash = controls.plugin_crash.last_crash().expect("the crash is found");
    assert_eq!((crash.kind.as_str(), crash.name.as_str()), ("vst3:synth", "Synth"));
    assert_eq!(crash.project.as_deref(), Some(file.as_path()), "the last project from the recent list");
    assert_eq!(session.startup().open(), None, "a project with a crashed plugin does not open by itself");
    assert!(controls.log.recent(20).iter().any(|e: &LogEntry| e.source == "startup" && e.message.contains("Synth")));
    controls.plugin_crash.dismiss();
    assert_eq!(session.startup().open(), Some(file), "after answering the report, as usual");
    std::fs::remove_dir_all(state).ok();
}
