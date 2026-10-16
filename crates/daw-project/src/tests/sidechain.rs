//! The `GraphControl::set_sidechain` contract: one key source per effect, the effect's `sidechain` switch follows it.

use super::*;

fn sidechains_into(rig: &Rig, effect: NodeId) -> Vec<NodeId> {
    rig.graph.graph().incoming(effect).filter(|e| e.sidechain).map(|e| e.from).collect()
}

/// Kick and bass tracks, a compressor on the bass.
fn duck_rig() -> (Rig, NodeId, NodeId, NodeId) {
    let rig = Rig::new();
    let kick = rig.tracks.add_track("Kick", Some("synth"), 60).unwrap();
    let bass = rig.tracks.add_track("Bass", Some("synth"), 60).unwrap();
    let (kick, bass) = (source(&rig, kick), source(&rig, bass));
    let comp = rig.graph.insert_effect(bass, "compressor", 0.0, 0.0).unwrap();
    (rig, kick, bass, comp)
}

#[test]
fn picking_a_source_connects_it_and_switches_the_key_on() {
    let (rig, kick, _, comp) = duck_rig();
    rig.graph.set_sidechain(comp, Some(kick)).unwrap();
    assert_eq!(sidechains_into(&rig, comp), [kick]);
    assert!(rig.graph.graph().edges.contains(&Edge { from: kick, to: comp, level: 1.0, sidechain: true, pre_fader: false }));
    assert_eq!(effect_params(&rig, comp)["sidechain"], 1.0);
}

#[test]
fn a_new_source_replaces_the_old_ones_and_none_switches_the_key_off() {
    let (rig, kick, _, comp) = duck_rig();
    let bus = rig.graph.add_bus("Drums", 0.0, 0.0).unwrap();
    rig.graph.connect(kick, comp, 0.5, true).unwrap();
    rig.graph.connect(bus, comp, 0.5, true).unwrap();
    rig.automation.add_lane(ParamTarget::EdgeLevel { from: kick, to: comp, sidechain: true }).unwrap();

    rig.graph.set_sidechain(comp, Some(bus)).unwrap();
    assert_eq!(sidechains_into(&rig, comp), [bus]);
    assert_eq!(rig.automation.lanes(), [] as [AutomationLane; 0], "the removed edge takes its lane along");

    rig.graph.set_sidechain(comp, None).unwrap();
    assert!(sidechains_into(&rig, comp).is_empty());
    assert_eq!(effect_params(&rig, comp)["sidechain"], 0.0);
    assert!(rig.graph.graph().edges.iter().any(|e| e.to == comp && !e.sidechain), "the audio input stays");
}

#[test]
fn picking_a_source_is_one_undo_step() {
    let (rig, kick, _, comp) = duck_rig();
    let bus = rig.graph.add_bus("Drums", 0.0, 0.0).unwrap();
    rig.graph.set_sidechain(comp, Some(kick)).unwrap();
    rig.graph.set_sidechain(comp, Some(bus)).unwrap();
    assert!(rig.service.undo());
    assert_eq!(sidechains_into(&rig, comp), [kick]);
    assert_eq!(effect_params(&rig, comp)["sidechain"], 1.0);
}

#[test]
fn wrong_targets_and_sources_are_refused_without_changes() {
    let (rig, kick, bass, comp) = duck_rig();
    rig.graph.set_sidechain(comp, Some(kick)).unwrap();
    let eq = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    let before = rig.service.snapshot();

    assert!(matches!(rig.graph.set_sidechain(eq, Some(kick)), Err(e) if e.is(codes::graph::NO_SIDECHAIN_INPUT)));
    assert!(matches!(rig.graph.set_sidechain(bass, Some(kick)), Err(e) if e.is(codes::graph::NODE_NOT_EFFECT)));
    assert!(matches!(rig.graph.set_sidechain(9999, None), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    assert!(matches!(rig.graph.set_sidechain(comp, Some(9999)), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    assert!(matches!(rig.graph.set_sidechain(comp, Some(comp)), Err(e) if e.is(codes::graph::EDGE_TO_ITSELF)));
    assert!(matches!(rig.graph.set_sidechain(comp, Some(MASTER)), Err(e) if e.is(codes::graph::MASTER_HAS_NO_OUTPUT)));
    assert_eq!(rig.service.snapshot(), before, "errors change nothing");

    // A bus fed by the compressor cannot key it: that would make a loop.
    let after = rig.graph.add_bus("After", 0.0, 0.0).unwrap();
    rig.graph.connect(comp, after, 1.0, false).unwrap();
    let before_loop = rig.service.snapshot();
    assert!(matches!(rig.graph.set_sidechain(comp, Some(after)), Err(e) if e.is(codes::graph::EDGE_MAKES_LOOP)));
    assert_eq!(rig.service.snapshot(), before_loop, "a refused source keeps the old one");
    assert_eq!(sidechains_into(&rig, comp), [kick]);
}
