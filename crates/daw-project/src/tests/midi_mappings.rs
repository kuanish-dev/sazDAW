//! The `MidiMappingControl` contract: learning, moving parameters with a knob, removing a mapping, vanished targets.

use std::sync::{Arc, Mutex};

use super::*;
use crate::testing::FakeEffects;
use crate::{MidiMappingDeps, ProjectGraph, ProjectMidiMappings, ProjectStage, ProjectTracks};

/// Remembers what was reported to the automation writer.
#[derive(Default)]
struct Touches(Mutex<Vec<(ParamTarget, f32, TouchEnd)>>);

impl AutomationWriter for Touches {
    fn mode(&self) -> AutomationWriteMode {
        AutomationWriteMode::Off
    }
    fn set_mode(&self, _: AutomationWriteMode) {}
    fn touch(&self, target: ParamTarget, value: f32, end: TouchEnd) -> Result<(), ContractError> {
        self.0.lock().unwrap().push((target, value, end));
        Ok(())
    }
    fn release(&self, _: &ParamTarget) {}
    fn release_all(&self) {}
}

fn mappings(rig: &Rig) -> ProjectMidiMappings {
    mappings_with(rig, Arc::new(Touches::default()))
}

fn mappings_with(rig: &Rig, writer: Arc<Touches>) -> ProjectMidiMappings {
    ProjectMidiMappings::new(
        rig.service.clone(),
        MidiMappingDeps {
            graph: Arc::new(ProjectGraph::new(rig.service.clone(), Arc::new(FakeEffects))),
            tracks: Arc::new(ProjectTracks::new(rig.service.clone(), Arc::new(FakeInstruments))),
            stage: Arc::new(ProjectStage::new(rig.service.clone())),
            instruments: Arc::new(FakeInstruments),
            effects: Arc::new(FakeEffects),
            writer,
            log: rig.log.clone(),
        },
    )
}

fn volume(rig: &Rig, node: NodeId) -> f32 {
    rig.graph.graph().node(node).unwrap().kind.strip().unwrap().volume
}

#[test]
fn learning_assigns_the_next_cc_and_the_knob_then_moves_the_parameter() {
    let (rig, track, _) = rig_with_clip();
    let midi = mappings(&rig);
    let node = source(&rig, track);
    let target = ParamTarget::NodeVolume { node };
    midi.learn(target.clone()).unwrap();
    assert_eq!(midi.learning(), Some(target.clone()));

    assert!(midi.control_change(2, 21, 64), "learning takes the CC");
    assert_eq!(midi.learning(), None);
    assert_eq!(midi.mappings(), [MidiMapping { cc: 21, channel: Some(2), target: target.clone(), min: 0.0, max: MAX_LEVEL }]);

    assert!(midi.control_change(2, 21, 127));
    assert!((volume(&rig, node) - MAX_LEVEL).abs() < 1e-6);
    assert!(midi.control_change(2, 21, 0));
    assert_eq!(volume(&rig, node), 0.0);
    assert!(!midi.control_change(3, 21, 0), "another channel is not mapped");
    assert!(!midi.control_change(2, 22, 0), "another CC goes to the instrument");

    // A knob turn is one undo step together with neighbouring moves, like a slider; the mapping is its own step.
    assert!(rig.service.undo());
    assert_eq!(volume(&rig, node), DEFAULT_VOLUME);
    assert_eq!(midi.mappings().len(), 1);
}

#[test]
fn a_width_knob_spans_mono_to_twice_as_wide() {
    let rig = Rig::new();
    let midi = mappings(&rig);
    let bus = rig.graph.add_bus("Pads", 0.0, 0.0).unwrap();
    let target = ParamTarget::NodeWidth { node: bus };
    midi.learn(target.clone()).unwrap();
    midi.control_change(0, 5, 64);
    assert_eq!(midi.mappings(), [MidiMapping { cc: 5, channel: Some(0), target, min: 0.0, max: MAX_WIDTH }]);
    let width = || rig.graph.graph().node(bus).unwrap().kind.strip().unwrap().width;
    midi.control_change(0, 5, 127);
    assert_eq!(width(), MAX_WIDTH);
    midi.control_change(0, 5, 0);
    assert_eq!(width(), 0.0);
}

#[test]
fn effect_and_instrument_params_use_their_catalog_range() {
    let (rig, track, _) = rig_with_clip();
    let midi = mappings(&rig);
    let fx = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    midi.learn(ParamTarget::EffectParam { node: fx, param: "gain".into() }).unwrap();
    midi.control_change(0, 1, 0);
    midi.control_change(0, 1, 127);
    assert_eq!(effect_params(&rig, fx)["gain"], 12.0);

    midi.learn(ParamTarget::InstrumentParam { track, param: "gain".into() }).unwrap();
    midi.control_change(0, 2, 0);
    midi.control_change(0, 2, 127);
    let gain = rig.tracks.tracks()[0].instrument.as_ref().unwrap().params["gain"];
    assert_eq!(gain, 1.0);
}

#[test]
fn bad_targets_are_refused_and_vanished_ones_are_hidden_until_undo() {
    let (rig, _, _) = rig_with_clip();
    let midi = mappings(&rig);
    assert!(matches!(midi.learn(ParamTarget::NodeVolume { node: 999 }), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    let not_effect = midi.learn(ParamTarget::EffectParam { node: MASTER, param: "gain".into() });
    assert!(matches!(not_effect, Err(e) if e.is(codes::graph::NODE_NOT_EFFECT)));

    let fx = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    let target = ParamTarget::EffectParam { node: fx, param: "gain".into() };
    midi.learn(target.clone()).unwrap();
    midi.control_change(0, 5, 0);
    rig.graph.remove_node(fx).unwrap();
    assert!(midi.mappings().is_empty(), "no target, so the mapping is not shown");
    assert!(!midi.control_change(0, 5, 100), "and does not act");
    assert!(rig.service.undo());
    assert_eq!(midi.mappings().len(), 1, "undoing the removal brought the mapping back");

    midi.remove_mapping(&target).unwrap();
    assert!(midi.mappings().is_empty());
    assert!(matches!(midi.remove_mapping(&target), Err(e) if e.is(codes::midi::NO_MAPPING)));
}

#[test]
fn relearning_a_target_replaces_its_mapping_and_cancel_stops_learning() {
    let (rig, track, _) = rig_with_clip();
    let midi = mappings(&rig);
    let target = ParamTarget::NodePan { node: source(&rig, track) };
    midi.learn(target.clone()).unwrap();
    midi.control_change(0, 10, 0);
    midi.learn(target.clone()).unwrap();
    midi.control_change(0, 11, 0);
    assert_eq!(midi.mappings().iter().map(|m| m.cc).collect::<Vec<_>>(), [11]);
    assert_eq!((midi.mappings()[0].min, midi.mappings()[0].max), (-1.0, 1.0));
    let pan = || rig.graph.graph().node(source(&rig, track)).unwrap().kind.strip().unwrap().pan;
    midi.control_change(0, 11, 0);
    assert_eq!(pan(), -1.0, "the knob's bottom is hard left, not the centre");
    midi.control_change(0, 11, 127);
    assert_eq!(pan(), 1.0);

    midi.learn(target).unwrap();
    midi.cancel_learn();
    assert!(!midi.control_change(0, 12, 0));
    assert!(!midi.control_change(0, 120, 0), "channel mode messages are never mapped");
}

#[test]
fn knob_moves_are_reported_to_the_automation_writer_as_idle_touches() {
    let (rig, track, _) = rig_with_clip();
    let writer = Arc::new(Touches::default());
    let midi = mappings_with(&rig, writer.clone());
    let target = ParamTarget::NodeVolume { node: source(&rig, track) };
    midi.learn(target.clone()).unwrap();
    midi.control_change(0, 7, 0);
    assert!(writer.0.lock().unwrap().is_empty(), "learning is not a move");
    midi.control_change(0, 7, 127);
    assert_eq!(*writer.0.lock().unwrap(), [(target, MAX_LEVEL, TouchEnd::Idle)]);
}

#[test]
fn a_knob_moves_a_channel_through_the_stage() {
    let (rig, track, _) = rig_with_clip();
    let midi = mappings(&rig);
    let node = source(&rig, track);
    let depth = ParamTarget::StageDepth { node };
    assert!(matches!(midi.learn(depth.clone()), Err(e) if e.is(codes::graph::NOT_ON_STAGE)));

    rig.stage.place(node, -0.5, StagePlace { depth: 0.2, height: 0.4 }).unwrap();
    midi.learn(depth).unwrap();
    midi.control_change(0, 30, 0);
    let strip = *rig.graph.graph().node(node).unwrap().kind.strip().unwrap();
    assert_eq!(strip.stage.map(|place| place.depth), Some(-1.0), "the bottom of the knob is the wall behind the listener");
    midi.control_change(0, 30, 127);
    let strip = *rig.graph.graph().node(node).unwrap().kind.strip().unwrap();
    assert_eq!((strip.pan, strip.stage), (-0.5, Some(StagePlace { depth: 1.0, height: 0.4 })), "only the depth moves");
}
