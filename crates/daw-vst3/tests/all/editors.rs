//! Plugin windows on real plugins, WITHOUT showing the window: creating the interface, size, closing, errors.
//! Attaching to an `NSView` is not tested (it needs the main thread and a real window). No plugin — the test is skipped.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: a panic with a clear message is the check"
)]

use std::sync::{Mutex, MutexGuard};

use crate::support::{DISTORTION, SURGE, SURGE_FX, world};
use daw_contracts::codes::{graph, plugins, tracks};
use daw_contracts::sync::lock;
use daw_contracts::{AppLog, EditorTarget, GraphControl, PluginEditors, TrackControl};

/// Plugins are created one at a time (as in the app): Surge XT touches the user's shared files.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    lock(&ONE_AT_A_TIME)
}

#[test]
fn surge_instrument_opens_with_a_size_closes_and_reopens() {
    let _one = serial();
    let Some(world) = world("inst", &[SURGE]) else { return };
    assert_eq!(world.instruments.len(), 1);
    world.add_instrument_track(1, world.instruments[0].0);
    let target = EditorTarget::Instrument { track: 1 };
    let session = world.editors.open(target).expect("the window is created");
    assert!(session.width >= 100 && session.height >= 100, "{session:?}");
    assert_eq!(session.title, "Surge XT");
    assert!(matches!(world.editors.open(target), Err(e) if e.is(plugins::EDITOR_ALREADY_OPEN)));
    assert_eq!(world.editors.sync(session.id).unwrap(), None);
    world.editors.close(session.id);
    world.editors.close(session.id);
    assert!(matches!(world.editors.sync(session.id), Err(e) if e.is(plugins::EDITOR_NOT_FOUND)));
    let again = world.editors.open(target).expect("opens again after closing");
    assert_ne!(again.id, session.id);
    world.editors.close(again.id);
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn surge_effect_opens_through_a_graph_node() {
    let _one = serial();
    let Some(world) = world("fx", &[SURGE_FX]) else { return };
    let Some((kind, _)) = world.effects.first() else {
        eprintln!("SKIPPED: no effect found in Surge XT Effects");
        return;
    };
    world.add_effect_node(7, kind);
    let session = world.editors.open(EditorTarget::Effect { node: 7 }).expect("the window is created");
    assert!(session.width > 0 && session.height > 0);
    world.editors.close(session.id);
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn project_values_are_pulled_on_sync_and_a_removed_target_ends_the_window() {
    let _one = serial();
    let Some(world) = world("sync", &[SURGE]) else { return };
    let (kind, params) = &world.instruments[0];
    world.add_instrument_track(3, kind);
    let session = world.editors.open(EditorTarget::Instrument { track: 3 }).unwrap();
    world.tracks.set_instrument_param(3, params[0], 0.42).unwrap();
    assert_eq!(world.editors.sync(session.id).unwrap(), None);
    world.tracks.remove_track(3).unwrap();
    assert!(matches!(world.editors.sync(session.id), Err(e) if e.is(tracks::NOT_FOUND)));
    world.editors.close(session.id);
}

#[test]
fn builtin_audio_track_and_unknown_targets_give_clear_errors() {
    let _one = serial();
    let Some(world) = world("errors", &[]) else { return };
    world.add_effect_node(1, "reverb");
    world.add_instrument_track(2, "synth");
    world.add_audio_track(3);
    let fails_with = |target, code| matches!(world.editors.open(target), Err(e) if e.is(code));
    assert!(fails_with(EditorTarget::Effect { node: 1 }, plugins::BUILTIN_EFFECT_HAS_NO_WINDOW));
    assert!(fails_with(EditorTarget::Instrument { track: 2 }, plugins::BUILTIN_INSTRUMENT_HAS_NO_WINDOW));
    assert!(fails_with(EditorTarget::Instrument { track: 3 }, tracks::NO_INSTRUMENT));
    assert!(fails_with(EditorTarget::Instrument { track: 99 }, tracks::NOT_FOUND));
    assert!(fails_with(EditorTarget::Effect { node: 99 }, graph::NODE_NOT_FOUND));
    world.add_effect_node(4, "vst3:no_such_plugin");
    assert!(fails_with(EditorTarget::Effect { node: 4 }, plugins::NOT_FOUND));
}

#[test]
fn distortion_has_a_small_native_window_and_follows_the_project() {
    let _one = serial();
    let Some(world) = world("distortion", &[DISTORTION]) else { return };
    let (kind, params) = &world.effects[0];
    world.add_effect_node(2, kind);
    let session = match world.editors.open(EditorTarget::Effect { node: 2 }) {
        Ok(session) => session,
        Err(error) => {
            assert!(error.is(plugins::NO_EDITOR), "{error}");
            return;
        }
    };
    assert_eq!((session.width, session.height), (360, 240));
    assert_eq!(params.len(), 5);
    world.graph.set_effect_param(2, params[0], 0.9).unwrap();
    assert_eq!(world.editors.sync(session.id).unwrap(), None);
    world.editors.close(session.id);
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}
