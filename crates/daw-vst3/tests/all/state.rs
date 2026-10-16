//! Plugin state and transport time on real plugins: the window gives its state to the project, a new instance with that
//! state sounds the same, the window picks up a state changed outside (undo); plugins get
//! `ProcessContext` and do not break with it. No plugin — the test is skipped.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: a panic with a clear message is the check"
)]

use std::sync::{Arc, Mutex, MutexGuard};

use crate::support::{DISTORTION, SURGE, SYNTH, World, world};
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AudioBlock, BlockTiming, Bpm, EditorTarget, Instrument, Params, PluginEditors, PluginState, PluginStates, StreamConfig,
    TimeSignature, TrackControl,
};

/// Plugins are created one at a time (as in the app): Surge XT touches the user's shared files.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    lock(&ONE_AT_A_TIME)
}

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

/// Segment time: 120 BPM, 4/4, playing from `start_tick`, a two-bar cycle.
fn playing(start_tick: f64) -> BlockTiming {
    let tempo = Bpm::new(120.0).unwrap();
    BlockTiming {
        frames: BLOCK,
        playing: true,
        start_tick,
        ticks_per_frame: tempo.ticks_per_frame(RATE),
        tempo,
        time_signature: TimeSignature::FOUR_FOUR,
        signature_start: 0.0,
        loop_region: Some((0, 7680)),
    }
}

/// Plays note 60 for `blocks` blocks, telling the plugin the time of each; the sound peak of the last 20 blocks (parameter
/// smoothing and the attack are over by then) and a NaN/inf check.
fn play_note(instrument: &mut dyn Instrument, blocks: usize) -> f32 {
    instrument.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
    instrument.note_on(60, 1.0);
    let mut peak = 0.0f32;
    let mut buffer = vec![0.0f32; BLOCK * 2];
    for block in 0..blocks {
        buffer.fill(0.0);
        let timing = playing(block as f64 * BLOCK as f64 * playing(0.0).ticks_per_frame);
        instrument.set_timing(&timing);
        instrument.render(&mut AudioBlock::new(&mut buffer, 2));
        assert!(buffer.iter().all(|v| v.is_finite()), "the sound has NaN/inf");
        if block + 20 >= blocks {
            peak = buffer.iter().fold(peak, |a, v| a.max(v.abs()));
        }
    }
    peak
}

fn instrument_state(world: &World, track: u32) -> Option<Arc<PluginState>> {
    world.states.get(EditorTarget::Instrument { track }).unwrap()
}

fn param_named(world: &World, label: &str) -> String {
    let params = world.instrument_factories[0].describe();
    params.iter().find(|p| p.label.get() == label).unwrap_or_else(|| panic!("no \"{label}\"")).name.to_string()
}

#[test]
fn synth_window_state_goes_to_the_project_and_a_new_instance_sounds_the_same() {
    let _one = serial();
    let Some(world) = world("state-synth", &[SYNTH]) else { return };
    world.add_instrument_track(1, world.instruments[0].0);
    let target = EditorTarget::Instrument { track: 1 };
    let session = world.editors.open(target).expect("the window is created");
    world.editors.store_states();
    assert!(world.states.writes.lock().unwrap().is_empty(), "opened the window, changed nothing: nothing to write");

    // A 40 Hz cutoff almost mutes the saw (note 60): the sound shows whether the value got through the state.
    let cutoff = param_named(&world, "Cutoff, Hz");
    world.tracks.set_instrument_param(1, &cutoff, 40.0).unwrap();
    world.editors.sync(session.id).unwrap();
    world.editors.store_states();
    assert_eq!(world.states.writes.lock().unwrap().len(), 1, "the window state differs, so it is written");
    let state = instrument_state(&world, 1).expect("the state is in the project");
    eprintln!("Synth: component state {} B, controller {} B", state.component.len(), state.controller.len());
    world.editors.store_states();
    assert_eq!(world.states.writes.lock().unwrap().len(), 1, "a repeat without changes writes nothing");

    let factory = &world.instrument_factories[0];
    let restored = play_note(&mut *factory.create(&Params::new(), Some(&state)).unwrap(), 80);
    let by_params = play_note(&mut *factory.create(&Params::from([(cutoff.clone(), 40.0)]), None).unwrap(), 80);
    let default = play_note(&mut *factory.create(&Params::new(), None).unwrap(), 80);
    eprintln!("Synth: peak from state {restored}, from params {by_params}, default {default}");
    assert!(default > 0.01 && by_params < default * 0.5, "the cutoff must be audible: {by_params} vs {default}");
    assert!(
        (restored - by_params).abs() <= by_params * 0.05 + 1e-4,
        "a new instance from the state sounds the same: {restored} / {by_params}"
    );

    world.editors.close(session.id);
    assert_eq!(world.states.writes.lock().unwrap().len(), 1, "closing without changes writes nothing");
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn window_follows_a_state_changed_outside_and_reports_a_broken_one() {
    let _one = serial();
    let Some(world) = world("state-follow", &[SYNTH]) else { return };
    world.add_instrument_track(1, world.instruments[0].0);
    let target = EditorTarget::Instrument { track: 1 };
    let session = world.editors.open(target).unwrap();
    let cutoff = param_named(&world, "Cutoff, Hz");
    world.tracks.set_instrument_param(1, &cutoff, 300.0).unwrap();
    world.editors.sync(session.id).unwrap();
    world.editors.store_states();
    let written = instrument_state(&world, 1).unwrap();

    // "Undo": the state was removed from the project. The window returns to its initial state and does not write it back.
    world.tracks.0.lock().unwrap()[0].instrument.as_mut().unwrap().state = None;
    world.editors.sync(session.id).unwrap();
    world.editors.store_states();
    assert_eq!(world.states.writes.lock().unwrap().len(), 1, "the window took the project state instead of overwriting it");
    // "Redo": the state is back, the window loads it again.
    world.tracks.0.lock().unwrap()[0].instrument.as_mut().unwrap().state = Some(written);
    world.editors.sync(session.id).unwrap();
    world.editors.store_states();
    assert_eq!(world.states.writes.lock().unwrap().len(), 1);
    world.editors.close(session.id);
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));

    // A broken state: the window still opens, and the plugin's refusal is logged.
    let broken = PluginState { component: b"not a synth state".to_vec(), controller: Vec::new() };
    world.tracks.0.lock().unwrap()[0].instrument.as_mut().unwrap().state = Some(Arc::new(broken.clone()));
    let session = world.editors.open(target).expect("the window opens even with a bad state");
    world.editors.close(session.id);
    assert!(world.log.recent(10).iter().any(|e| e.message.contains("the saved state did not load")), "log: {:?}", world.log.recent(10));
    // The sound instance with the same state is created (with the project params), the refusal is logged.
    let before = world.log.recent(100).len();
    let mut synth = world.instrument_factories[0].create(&Params::new(), Some(&broken)).expect("the instrument is created");
    assert!(play_note(&mut *synth, 20) > 0.0);
    assert!(world.log.recent(100).len() > before, "the plugin's refusal to take the state did not vanish silently");
}

#[test]
fn surge_state_is_large_survives_a_new_instance_and_plays_with_transport_context() {
    let _one = serial();
    let Some(world) = world("state-surge", &[SURGE]) else { return };
    world.add_instrument_track(2, world.instruments[0].0);
    let session = world.editors.open(EditorTarget::Instrument { track: 2 }).unwrap();
    let first = world.instruments[0].1[0];
    world.tracks.set_instrument_param(2, first, 0.3).unwrap();
    world.editors.sync(session.id).unwrap();
    world.editors.close(session.id);
    let state = instrument_state(&world, 2).expect("closing a changed window writes the state");
    eprintln!("Surge XT: component state {} B, controller {} B", state.component.len(), state.controller.len());
    assert!(state.component.len() > 1000, "Surge XT's state is a whole patch");
    let mut surge = world.instrument_factories[0].create(&Params::new(), Some(&state)).expect("an instance from the state");
    let peak = play_note(&mut *surge, 100);
    eprintln!("Surge XT: note peak with a transport context {peak}");
    assert!(peak > 1e-4, "Surge XT is silent with a state and transport time");
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn distortion_effect_state_round_trips_through_its_window() {
    let _one = serial();
    let Some(world) = world("state-fx", &[DISTORTION]) else { return };
    let Some((kind, params)) = world.effects.first().cloned() else {
        eprintln!("SKIPPED: the Distortion effect is not found");
        return;
    };
    world.add_effect_node(4, kind);
    let target = EditorTarget::Effect { node: 4 };
    let Ok(session) = world.editors.open(target) else {
        eprintln!("SKIPPED: Distortion has no window");
        return;
    };
    let mut graph = world.graph.0.lock().unwrap();
    if let Some(daw_contracts::NodeKind::Effect { params: values, .. }) = graph.node_mut(4).map(|n| &mut n.kind) {
        values.insert(params[0].to_string(), 0.0);
    }
    drop(graph);
    world.editors.sync(session.id).unwrap();
    world.editors.close(session.id);
    let state = world.states.get(target).unwrap();
    // The project knows all Distortion params: the window state may match the original, then there is nothing to write.
    if let Some(state) = state {
        let mut effect = world.effect_factories[0].create(&Params::new(), Some(&state)).expect("an effect from the state");
        effect.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
        effect.set_timing(&playing(0.0));
        let mut block: Vec<f32> = (0..BLOCK * 2).map(|n| (n as f32 * 0.05).sin() * 0.5).collect();
        effect.process(&mut block, None);
        assert!(block.iter().all(|v| v.is_finite()));
    }
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}
