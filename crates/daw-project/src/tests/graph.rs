//! The `GraphControl` contract: nodes, edges, faders, effect chains.

use super::*;

#[test]
fn add_bus_is_connected_to_master_and_can_be_removed() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Percussion", 300.0, 100.0).unwrap();
    assert_eq!(edge_count(&rig, bus, MASTER), 1);
    rig.graph.set_pan(bus, 5.0).unwrap();
    rig.graph.set_volume(bus, 9.0).unwrap();
    rig.graph.set_mute(bus, true).unwrap();
    rig.graph.set_solo(bus, true).unwrap();
    let strip = *rig.graph.graph().node(bus).unwrap().kind.strip().unwrap();
    assert_eq!(strip, Strip { volume: MAX_LEVEL, pan: 1.0, mute: true, solo: true, ..Strip::default() });
    rig.graph.move_node(bus, 10.0, 20.0).unwrap();
    rig.graph.rename_node(bus, "Drums").unwrap();
    let node = rig.graph.graph().node(bus).unwrap().clone();
    assert_eq!((node.x, node.y, node.name.as_str()), (10.0, 20.0, "Drums"));
    assert!(rig.graph.set_volume(9999, 1.0).is_err());
    assert!(matches!(rig.graph.set_mute(9999, true), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
}

#[test]
fn source_faders_set_volume_pan_mute_and_solo() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    for node in [source(&rig, track), source(&rig, audio)] {
        rig.graph.set_volume(node, 9.0).unwrap();
        rig.graph.set_pan(node, -5.0).unwrap();
        rig.graph.set_mute(node, true).unwrap();
        rig.graph.set_solo(node, true).unwrap();
        let strip = *rig.graph.graph().node(node).unwrap().kind.strip().unwrap();
        assert_eq!(strip, Strip { volume: MAX_LEVEL, pan: -1.0, mute: true, solo: true, ..Strip::default() }, "the values are clamped");
        assert!(matches!(rig.graph.set_volume(node, f32::NAN), Err(e) if e.is(codes::graph::VOLUME_NOT_A_NUMBER)));
        assert!(matches!(rig.graph.set_pan(node, f32::INFINITY), Err(e) if e.is(codes::graph::PAN_NOT_A_NUMBER)));
    }
    assert!(matches!(rig.graph.remove_node(source(&rig, track)), Err(e) if e.is(codes::graph::NOT_REMOVABLE)));
    rig.automation.add_lane(ParamTarget::NodeVolume { node: source(&rig, track) }).unwrap();
    rig.automation.add_lane(ParamTarget::NodePan { node: source(&rig, audio) }).unwrap();
}

#[test]
fn per_kind_controls_reject_the_wrong_nodes() {
    let rig = Rig::new();
    let fx = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    use codes::graph::{NO_MUTE, NO_PAN, NO_SOLO, NO_VOLUME, NODE_NOT_EFFECT};
    assert!(matches!(rig.graph.set_volume(fx, 1.0), Err(e) if e.is(NO_VOLUME)), "an effect has no volume");
    assert!(matches!(rig.graph.set_pan(MASTER, 0.0), Err(e) if e.is(NO_PAN)), "the master has no pan");
    assert!(matches!(rig.graph.set_pan(fx, 0.0), Err(e) if e.is(NO_PAN)));
    assert!(matches!(rig.graph.set_mute(MASTER, true), Err(e) if e.is(NO_MUTE)));
    assert!(matches!(rig.graph.set_solo(fx, true), Err(e) if e.is(NO_SOLO)));
    assert!(matches!(rig.graph.set_effect_bypass(MASTER, true), Err(e) if e.is(NODE_NOT_EFFECT)));
    assert!(matches!(rig.graph.set_effect_param(MASTER, "gain", 1.0), Err(e) if e.is(NODE_NOT_EFFECT)));
    assert!(rig.graph.set_volume(MASTER, f32::NAN).is_err());
    assert!(matches!(rig.automation.add_lane(ParamTarget::NodeVolume { node: fx }), Err(e) if e.is(NO_VOLUME)));
    assert!(matches!(rig.automation.add_lane(ParamTarget::NodePan { node: MASTER }), Err(e) if e.is(NO_PAN)));
    rig.graph.set_effect_bypass(fx, true).unwrap();
    assert!(matches!(rig.graph.graph().node(fx).unwrap().kind, NodeKind::Effect { bypass: true, .. }));
}

#[test]
fn effects_are_created_from_the_catalog_and_params_are_clamped() {
    let rig = Rig::new();
    let fx = rig.graph.add_effect("eq", &Params::new(), 1.0, 2.0).unwrap();
    assert_eq!(effect_params(&rig, fx)["gain"], 0.0);
    assert_eq!(edge_count(&rig, fx, MASTER) + rig.graph.graph().incoming(fx).count(), 0, "no edges");
    rig.graph.set_effect_param(fx, "gain", 50.0).unwrap();
    assert_eq!(effect_params(&rig, fx)["gain"], 12.0);
    assert!(matches!(rig.graph.set_effect_param(fx, "nope", 1.0), Err(e) if e.is(codes::graph::UNKNOWN_EFFECT_PARAM)));
    assert!(matches!(rig.graph.add_effect("nope", &Params::new(), 0.0, 0.0), Err(e) if e.is(codes::graph::UNKNOWN_EFFECT)));
}

#[test]
fn connect_follows_the_graph_rules_and_failures_change_nothing() {
    let (rig, track, _) = rig_with_clip();
    let source = source(&rig, track);
    let fx1 = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    let fx2 = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.connect(source, fx1, 1.0, false).unwrap();
    rig.graph.connect(fx1, fx2, 1.0, false).unwrap();
    let before = rig.service.snapshot();
    let revision = rig.service.revision();

    assert!(rig.graph.connect(fx2, fx1, 1.0, false).is_err(), "a loop");
    assert!(rig.graph.connect(fx1, fx1, 1.0, false).is_err(), "to itself");
    assert!(rig.graph.connect(source, bus, 1.0, true).is_err(), "a sidechain goes only into an effect");
    assert!(rig.graph.connect(MASTER, bus, 1.0, false).is_err(), "from the master");
    assert!(rig.graph.connect(fx1, source, 1.0, false).is_err(), "into a source");
    assert!(matches!(rig.graph.connect(fx1, 9999, 1.0, false), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    assert!(matches!(rig.graph.connect(fx1, bus, f32::NAN, false), Err(e) if e.is(codes::graph::LEVEL_NOT_A_NUMBER)));
    assert_eq!(rig.service.snapshot(), before, "the project did not change");
    assert_eq!(rig.service.revision(), revision, "no snapshot was published");

    rig.graph.connect(source, fx2, 1.0, true).unwrap();
    assert!(rig.graph.disconnect(source, fx2, true).is_ok());
    assert!(matches!(rig.graph.disconnect(source, fx2, true), Err(e) if e.is(codes::graph::NO_SUCH_EDGE)), "the edge is gone");
}

#[test]
fn connecting_the_same_pair_replaces_the_level() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    let other = rig.graph.add_bus("Other", 0.0, 0.0).unwrap();
    rig.graph.connect(bus, other, 0.5, false).unwrap();
    rig.graph.connect(bus, other, 9.0, false).unwrap();
    let edges: Vec<_> = rig.graph.graph().edges.into_iter().filter(|e| e.from == bus && e.to == other).collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].level, MAX_LEVEL);
}

#[test]
fn removing_a_node_cleans_edges_and_automation_but_sources_and_master_stay() {
    let (rig, track, _) = rig_with_clip();
    let source = source(&rig, track);
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.connect(source, bus, 0.7, false).unwrap();
    rig.automation.add_lane(ParamTarget::NodeVolume { node: bus }).unwrap();
    rig.automation.add_lane(ParamTarget::NodePan { node: bus }).unwrap();

    rig.graph.remove_node(bus).unwrap();
    let graph = rig.graph.graph();
    assert!(graph.node(bus).is_none());
    assert!(graph.edges.iter().all(|e| e.from != bus && e.to != bus));
    assert_eq!(rig.automation.lanes(), [] as [daw_contracts::AutomationLane; 0]);
    assert!(graph.edges.iter().any(|e| e.from == source && e.to == MASTER), "the track still sounds");

    assert!(rig.graph.remove_node(MASTER).is_err());
    assert!(rig.graph.remove_node(source).is_err(), "a source goes only with its track");
    assert!(matches!(rig.graph.remove_node(9999), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
}

#[test]
fn graph_limits_are_enforced() {
    let rig = Rig::new();
    while rig.graph.graph().nodes.len() < MAX_NODES {
        rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    }
    assert!(matches!(rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0), Err(e) if e.is(codes::graph::TOO_MANY_NODES)));
    assert!(matches!(rig.graph.add_bus("x", 0.0, 0.0), Err(e) if e.is(codes::graph::TOO_MANY_NODES)));
    assert!(rig.graph.insert_effect(MASTER, "eq", &Params::new(), 0.0, 0.0).is_err());
    assert!(matches!(rig.tracks.add_track("x", Some("synth"), 60), Err(e) if e.is(codes::graph::TOO_MANY_NODES)));
}

#[test]
fn graph_edits_can_be_undone() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    assert!(rig.service.undo());
    let graph = rig.graph.graph();
    assert!(graph.node(bus).is_none());
    assert!(graph.edges.iter().all(|e| e.from != bus));
    assert!(rig.service.redo());
    assert_eq!(edge_count(&rig, bus, MASTER), 1);
}

#[test]
fn level_slider_moves_merge_into_one_undo_step() {
    let rig = Rig::new();
    let a = rig.graph.add_bus("A", 0.0, 0.0).unwrap();
    let b = rig.graph.add_bus("B", 0.0, 0.0).unwrap();
    rig.graph.connect(a, b, 0.1, false).unwrap();
    for step in 2..=8 {
        rig.graph.connect(a, b, 0.1 * step as f32, false).unwrap();
    }
    assert!(rig.service.undo());
    assert_eq!(edge_count(&rig, a, b), 0, "all level moves are one step, undone together with the edge");
}

/// A track with source `src` and two effects in a chain: `src → fx1 → fx2 → MASTER`.
fn chain_rig() -> (Rig, NodeId, NodeId, NodeId) {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let src = source(&rig, track);
    let fx1 = rig.graph.insert_effect(src, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let fx2 = rig.graph.insert_effect(fx1, "eq", &Params::new(), 0.0, 0.0).unwrap();
    (rig, src, fx1, fx2)
}

#[test]
fn insert_effect_rewires_outputs_and_keeps_levels() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let src = source(&rig, track);
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.connect(src, bus, 0.5, false).unwrap();
    rig.graph.connect(src, MASTER, 0.3, false).unwrap();

    let fx = rig.graph.insert_effect(src, "eq", &Params::new(), 5.0, 6.0).unwrap();
    let graph = rig.graph.graph();
    let node = graph.node(fx).unwrap();
    assert_eq!((node.x, node.y, node.name.as_str()), (5.0, 6.0, "EQ"));
    let level = |from, to| graph.edges.iter().find(|e| e.from == from && e.to == to).map(|e| e.level);
    assert_eq!(level(src, fx), Some(1.0));
    assert_eq!((level(fx, bus), level(fx, MASTER)), (Some(0.5), Some(0.3)), "levels are kept");
    assert_eq!((level(src, bus), level(src, MASTER)), (None, None), "there are no direct edges any more");
    assert_eq!(graph.outgoing(src).count(), 1);
    assert!(rig.service.undo(), "inserting is one undo step");
    assert_eq!(edge_count(&rig, src, bus), 1);
}

#[test]
fn insert_effect_builds_a_chain_the_ui_can_show() {
    let (rig, src, fx1, fx2) = chain_rig();
    let graph = rig.graph.graph();
    assert_eq!(graph.chain(src), TrackChain { effects: vec![fx1, fx2], output: Some(MASTER) });

    let mid = rig.graph.insert_effect(fx1, "eq", &Params::new(), 0.0, 0.0).unwrap();
    assert_eq!(rig.graph.graph().chain(src).effects, [fx1, mid, fx2]);
}

#[test]
fn insert_effect_validates_and_leaves_sidechains_untouched() {
    let rig = Rig::new();
    let kick = rig.tracks.add_track("Kick", Some("synth"), 60).unwrap();
    let bass = rig.tracks.add_track("Bass", Some("synth"), 60).unwrap();
    let (kick_node, bass_node) = (source(&rig, kick), source(&rig, bass));
    let comp = rig.graph.insert_effect(bass_node, "eq", &Params::new(), 0.0, 0.0).unwrap();
    rig.graph.connect(kick_node, comp, 0.7, true).unwrap();

    let fx = rig.graph.insert_effect(kick_node, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let graph = rig.graph.graph();
    assert!(
        graph.edges.contains(&Edge { from: kick_node, to: comp, level: 0.7, sidechain: true, pre_fader: false }),
        "the sidechain stays with the source"
    );
    assert_eq!(graph.chain(kick_node).effects, [fx]);

    rig.graph.connect(fx, comp, 0.4, true).unwrap();
    let after = rig.graph.insert_effect(fx, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let graph = rig.graph.graph();
    assert!(graph.edges.contains(&Edge { from: fx, to: comp, level: 0.4, sidechain: true, pre_fader: false }));
    assert_eq!(graph.chain(kick_node).effects, [fx, after]);

    let before = rig.service.snapshot();
    assert!(rig.graph.insert_effect(MASTER, "eq", &Params::new(), 0.0, 0.0).is_err(), "the master has no outputs");
    assert!(rig.graph.insert_effect(9999, "eq", &Params::new(), 0.0, 0.0).is_err());
    assert!(rig.graph.insert_effect(kick_node, "nope", &Params::new(), 0.0, 0.0).is_err());
    assert_eq!(rig.service.snapshot(), before, "errors change nothing");
}

#[test]
fn remove_effect_keep_path_restores_the_direct_path() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let src = source(&rig, track);
    let before = rig.graph.graph().edges;
    let fx = rig.graph.insert_effect(src, "eq", &Params::new(), 0.0, 0.0).unwrap();
    rig.graph.remove_effect_keep_path(fx).unwrap();
    let graph = rig.graph.graph();
    assert!(graph.node(fx).is_none());
    assert_eq!(graph.edges, before, "the direct path is back");
    assert_eq!(graph.chain(src), TrackChain { effects: vec![], output: Some(MASTER) });

    assert!(matches!(rig.graph.remove_effect_keep_path(src), Err(e) if e.is(codes::graph::NODE_NOT_EFFECT)), "a source is not an effect");
    assert!(rig.graph.remove_effect_keep_path(9999).is_err());
}

#[test]
fn remove_effect_keep_path_multiplies_levels_and_drops_sidechain_inputs() {
    let rig = Rig::new();
    let a = rig.tracks.add_track("A", Some("synth"), 60).unwrap();
    let b = rig.tracks.add_track("B", Some("synth"), 60).unwrap();
    let (a_node, b_node) = (source(&rig, a), source(&rig, b));
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    let fx = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    rig.graph.connect(a_node, fx, 0.5, false).unwrap();
    rig.graph.connect(b_node, fx, 1.0, false).unwrap();
    rig.graph.connect(fx, bus, 0.5, false).unwrap();
    rig.graph.connect(fx, MASTER, 1.0, false).unwrap();
    let side = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    rig.graph.connect(a_node, side, 1.0, true).unwrap();
    rig.graph.connect(side, fx, 1.0, false).unwrap();
    rig.graph.connect(a_node, fx, 1.0, true).unwrap();

    rig.graph.remove_effect_keep_path(fx).unwrap();
    let graph = rig.graph.graph();
    let level = |from, to| graph.edges.iter().find(|e| e.from == from && e.to == to && !e.sidechain).map(|e| e.level);
    assert_eq!(level(a_node, bus), Some(0.25), "0.5 × 0.5");
    assert_eq!(level(b_node, bus), Some(0.5));
    assert_eq!(level(b_node, MASTER), Some(MAX_LEVEL));
    assert_eq!(level(side, bus), Some(0.5));
    assert!(graph.edges.iter().all(|e| e.from != fx && e.to != fx), "the effect and all its edges are gone");
    assert!(graph.edges.iter().any(|e| e.from == a_node && e.to == side && e.sidechain), "another sidechain is intact");
}

#[test]
fn set_output_switches_the_single_output() {
    let (rig, src, fx1, fx2) = chain_rig();
    let bus = rig.graph.add_bus("Group", 0.0, 0.0).unwrap();

    rig.graph.set_output(fx2, bus).unwrap();
    let graph = rig.graph.graph();
    assert_eq!(graph.chain(src), TrackChain { effects: vec![fx1, fx2], output: Some(bus) });
    assert_eq!(graph.outgoing(fx2).count(), 1);
    assert_eq!(edge_count(&rig, fx2, MASTER), 0);

    rig.graph.set_output(fx2, MASTER).unwrap();
    assert_eq!(rig.graph.graph().chain(src).output, Some(MASTER));

    let track = rig.tracks.add_track("Other", Some("synth"), 60).unwrap();
    let other = source(&rig, track);
    rig.graph.connect(other, bus, 0.4, false).unwrap();
    rig.graph.set_output(other, bus).unwrap();
    let graph = rig.graph.graph();
    assert_eq!(graph.outgoing(other).map(|e| (e.to, e.level)).collect::<Vec<_>>(), [(bus, 1.0)]);
}

#[test]
fn set_output_follows_graph_rules_and_keeps_sidechains() {
    let (rig, src, fx1, fx2) = chain_rig();
    let bus = rig.graph.add_bus("Group", 0.0, 0.0).unwrap();
    rig.graph.connect(src, fx1, 0.5, true).unwrap();
    let before = rig.service.snapshot();
    assert!(rig.graph.set_output(fx2, fx1).is_err(), "a loop");
    assert!(rig.graph.set_output(MASTER, bus).is_err(), "the master has no outputs");
    assert!(rig.graph.set_output(fx1, src).is_err(), "not into a source");
    assert!(rig.graph.set_output(fx1, fx1).is_err());
    assert!(rig.graph.set_output(9999, bus).is_err());
    assert!(rig.graph.set_output(fx1, 9999).is_err());
    assert_eq!(rig.service.snapshot(), before, "errors change nothing");

    rig.graph.set_output(src, bus).unwrap();
    assert!(
        rig.graph.graph().edges.contains(&Edge { from: src, to: fx1, level: 0.5, sidechain: true, pre_fader: false }),
        "the sidechain is left alone"
    );
}

#[test]
fn new_effects_take_params_on_top_of_defaults_in_one_undo_step() {
    let rig = Rig::new();
    let src = source(&rig, rig.tracks.add_track("Synth", Some("synth"), 60).unwrap());
    let gain = |value: f32| Params::from([("gain".to_string(), value)]);

    let added = rig.graph.add_effect("eq", &gain(5.0), 0.0, 0.0).unwrap();
    assert_eq!(effect_params(&rig, added)["gain"], 5.0);
    let after = rig.graph.insert_effect(src, "eq", &gain(50.0), 0.0, 0.0).unwrap();
    assert_eq!(effect_params(&rig, after)["gain"], 12.0, "clamped to the catalog range");
    let before = rig.graph.insert_effect_before(MASTER, "eq", &gain(-3.0), 0.0, 0.0).unwrap();
    assert_eq!(effect_params(&rig, before)["gain"], -3.0);
    let default = rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    assert_eq!(effect_params(&rig, default)["gain"], 0.0, "no params means the catalog defaults");

    assert!(rig.service.undo(), "the effect and its params are one undo step");
    assert!(rig.graph.graph().node(default).is_none());
    assert!(rig.service.undo());
    assert!(rig.graph.graph().node(before).is_none());
    assert_eq!(effect_params(&rig, after)["gain"], 12.0, "the previous effect keeps its params");
}

#[test]
fn a_bad_param_creates_no_effect() {
    let rig = Rig::new();
    let src = source(&rig, rig.tracks.add_track("Synth", Some("synth"), 60).unwrap());
    let before = rig.service.snapshot();
    let unknown = Params::from([("nope".to_string(), 1.0)]);
    let not_a_number = Params::from([("gain".to_string(), f32::NAN)]);

    assert!(matches!(rig.graph.add_effect("eq", &unknown, 0.0, 0.0), Err(e) if e.is(codes::graph::UNKNOWN_EFFECT_PARAM)));
    assert!(matches!(rig.graph.insert_effect(src, "eq", &unknown, 0.0, 0.0), Err(e) if e.is(codes::graph::UNKNOWN_EFFECT_PARAM)));
    assert!(matches!(
        rig.graph.insert_effect_before(MASTER, "eq", &not_a_number, 0.0, 0.0),
        Err(e) if e.is(codes::graph::PARAM_NOT_A_NUMBER)
    ));
    assert_eq!(rig.service.snapshot(), before, "nothing is created");
}
