//! The `PluginStates` contract: a plugin state in the project, the undo step, recreating the sounding object, target
//! errors.

use super::*;
use crate::testing::BAD_STATE;

fn state(bytes: &[u8]) -> PluginState {
    PluginState { component: bytes.to_vec(), controller: b"ctl".to_vec() }
}

fn rebuilds(rig: &Rig, what: &str) -> usize {
    rig.dsp_log().iter().filter(|line| *line == what).count()
}

#[test]
fn instrument_state_is_one_undo_step_and_rebuilds_the_instrument() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let target = EditorTarget::Instrument { track };
    let add = format!("add_instrument:{track}");
    assert_eq!(rig.plugin_states.get(target).unwrap(), None, "a new track has no state");

    rig.plugin_states.set(target, state(b"patch-1")).unwrap();
    assert_eq!(rig.plugin_states.get(target).unwrap().unwrap().component, b"patch-1");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Plugin state"));
    assert_eq!(rebuilds(&rig, &add), 2, "a state cannot be passed to a live instrument, so it is created again");
    assert_eq!(rig.last_snapshot().project.tracks[0].instrument.as_ref().unwrap().state.as_ref().unwrap().component, b"patch-1");

    rig.plugin_states.set(target, state(b"patch-1")).unwrap();
    assert_eq!(rebuilds(&rig, &add), 2, "the same state changes nothing");
    rig.tracks.set_instrument_param(track, "gain", 0.2).unwrap();
    assert_eq!(rebuilds(&rig, &add), 2, "a parameter on top of the state goes without recreating");

    rig.plugin_states.set(target, state(b"patch-2")).unwrap();
    assert_eq!(rebuilds(&rig, &add), 3);
    assert!(rig.service.undo());
    assert_eq!(rig.plugin_states.get(target).unwrap().unwrap().component, b"patch-1", "undo brings back the previous state");
    assert_eq!(rebuilds(&rig, &add), 4, "and the previous sound: the instrument is created again with it");
    assert!(rig.service.redo());
    assert_eq!(rig.plugin_states.get(target).unwrap().unwrap().component, b"patch-2");
}

#[test]
fn effect_state_rebuilds_the_effect_and_survives_param_changes() {
    let rig = Rig::new();
    let node = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    let target = EditorTarget::Effect { node };
    let add = format!("add_effect:{node}");
    rig.plugin_states.set(target, state(b"room")).unwrap();
    assert_eq!(rebuilds(&rig, &add), 2);
    rig.graph.set_effect_param(node, "gain", 3.0).unwrap();
    assert_eq!(rebuilds(&rig, &add), 2);
    assert_eq!(rig.plugin_states.get(target).unwrap().unwrap().component, b"room");
    assert!(rig.service.undo());
    assert!(rig.service.undo());
    assert_eq!(rig.plugin_states.get(target).unwrap(), None);
    assert_eq!(rebuilds(&rig, &add), 3, "an effect without a state is a new object again");
}

#[test]
fn state_reaches_the_catalog_and_a_refusal_is_a_warning() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    rig.plugin_states.set(EditorTarget::Instrument { track }, state(BAD_STATE)).unwrap();
    let warnings = rig.files.take_warnings();
    assert!(warnings.iter().any(|w| w.message().contains("the plugin state was rejected")), "{warnings:?}");
    assert!(rig.log.0.lock().unwrap().iter().any(|e| e.source == "dsp"), "the failure is in the log");
}

#[test]
fn wrong_targets_are_errors() {
    let rig = Rig::new();
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    let audio_target = EditorTarget::Instrument { track: audio };
    assert!(matches!(rig.plugin_states.get(audio_target), Err(e) if e.is(codes::tracks::NO_INSTRUMENT)));
    assert!(matches!(rig.plugin_states.set(audio_target, state(b"x")), Err(e) if e.is(codes::tracks::NO_INSTRUMENT)));
    assert!(matches!(rig.plugin_states.get(EditorTarget::Effect { node: bus }), Err(e) if e.is(codes::graph::NODE_NOT_EFFECT)));
    assert!(matches!(rig.plugin_states.get(EditorTarget::Effect { node: 999 }), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    let missing = rig.plugin_states.set(EditorTarget::Instrument { track: 999 }, state(b"x"));
    assert!(matches!(missing, Err(e) if e.is(codes::tracks::NOT_FOUND)));
}

#[test]
fn state_stays_with_its_kind_only() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let target = EditorTarget::Instrument { track };
    rig.plugin_states.set(target, state(b"patch")).unwrap();
    rig.tracks.set_instrument(track, "synth", Params::from([("gain".to_string(), 0.1)]), None).unwrap();
    assert!(rig.plugin_states.get(target).unwrap().is_some(), "the same kind keeps the state");
    rig.tracks.set_instrument(track, "sampler", Params::new(), None).unwrap();
    assert_eq!(rig.plugin_states.get(target).unwrap(), None, "another kind: the state belongs to someone else");
}

#[test]
fn state_is_saved_and_loaded_with_the_project() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    rig.plugin_states.set(EditorTarget::Instrument { track }, state(b"saved")).unwrap();
    rig.files.save(Path::new("/p/state.daw")).unwrap();
    rig.files.new_project();
    rig.files.load(Path::new("/p/state.daw")).unwrap();
    assert_eq!(rig.plugin_states.get(EditorTarget::Instrument { track }).unwrap().unwrap().component, b"saved");
}
