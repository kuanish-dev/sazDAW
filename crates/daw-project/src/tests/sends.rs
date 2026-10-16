//! The sends contract: pre/post-fader edges (`GraphControl::set_edge_pre_fader`) and edge level automation
//! (`ParamTarget::EdgeLevel`), which lives as long as its edge.

use super::*;

/// A track (source `src` → master) and a reverb bus `bus` with a send `src → bus` (level 0.5).
fn send_rig() -> (Rig, NodeId, NodeId) {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let src = source(&rig, track);
    let bus = rig.graph.add_bus("Reverb", 0.0, 0.0).unwrap();
    rig.graph.connect(src, bus, 0.5, false).unwrap();
    (rig, src, bus)
}

fn edge(rig: &Rig, from: NodeId, to: NodeId) -> Edge {
    *rig.graph.graph().edges.iter().find(|e| e.from == from && e.to == to && !e.sidechain).expect("the edge")
}

fn level_lane(from: NodeId, to: NodeId) -> ParamTarget {
    ParamTarget::EdgeLevel { from, to, sidechain: false }
}

fn has_lane(rig: &Rig, target: &ParamTarget) -> bool {
    rig.automation.lanes().iter().any(|lane| lane.target == *target)
}

#[test]
fn new_edges_are_post_fader_and_the_toggle_is_one_undo_step() {
    let (rig, src, bus) = send_rig();
    assert!(!edge(&rig, src, bus).pre_fader, "post-fader by default");
    rig.graph.set_edge_pre_fader(src, bus, true).unwrap();
    assert!(edge(&rig, src, bus).pre_fader);
    rig.graph.connect(src, bus, 0.7, false).unwrap();
    assert_eq!((edge(&rig, src, bus).level, edge(&rig, src, bus).pre_fader), (0.7, true), "changing the level leaves the send kind alone");
    assert!(rig.service.undo());
    assert!(rig.service.undo());
    assert!(!edge(&rig, src, bus).pre_fader, "switching is one undo step");
    // A bus can send pre-fader too.
    rig.graph.set_edge_pre_fader(bus, MASTER, true).unwrap();
    assert!(edge(&rig, bus, MASTER).pre_fader);
}

#[test]
fn pre_fader_is_rejected_from_effects_master_and_missing_edges() {
    let (rig, src, bus) = send_rig();
    let fx = rig.graph.insert_effect(bus, "eq", 0.0, 0.0).unwrap();
    let fails = |r: Result<(), ContractError>, code| matches!(r, Err(e) if e.is(code));
    assert!(fails(rig.graph.set_edge_pre_fader(fx, MASTER, true), codes::graph::PRE_FADER_NEEDS_STRIP), "an effect has no fader");
    assert!(fails(rig.graph.set_edge_pre_fader(src, fx, true), codes::graph::NO_SUCH_EDGE), "there is no edge src → fx");
    assert!(fails(rig.graph.set_edge_pre_fader(src, 9999, true), codes::graph::NODE_NOT_FOUND));
    let kick = source(&rig, rig.tracks.add_track("Kick", Some("synth"), 36).unwrap());
    rig.graph.connect(kick, fx, 1.0, true).unwrap();
    assert!(fails(rig.graph.set_edge_pre_fader(kick, fx, true), codes::graph::NO_SUCH_EDGE), "a sidechain edge is never a send");
    let before = rig.graph.graph();
    assert!(rig.graph.set_edge_pre_fader(MASTER, bus, true).is_err(), "the master has no outputs");
    assert_eq!(rig.graph.graph(), before, "a refusal changes nothing");
}

#[test]
fn pre_fader_sends_stay_with_the_node_when_the_path_changes() {
    let (rig, src, bus) = send_rig();
    rig.graph.set_edge_pre_fader(src, bus, true).unwrap();
    let fx = rig.graph.insert_effect(src, "eq", 0.0, 0.0).unwrap();
    assert!(edge(&rig, src, bus).pre_fader, "the send stayed with the track instead of moving to the effect");
    assert_eq!(rig.graph.graph().chain(src).effects, [fx]);
    let other = rig.graph.add_bus("Other", 0.0, 0.0).unwrap();
    rig.graph.set_output(src, other).unwrap();
    assert!(edge(&rig, src, bus).pre_fader, "changing the output leaves the send alone");
    assert_eq!(edge_count(&rig, src, fx), 0);
}

#[test]
fn edge_level_lanes_need_an_existing_edge_and_die_with_it() {
    let (rig, src, bus) = send_rig();
    assert!(
        matches!(rig.automation.add_lane(level_lane(bus, src)), Err(e) if e.is(codes::graph::NO_SUCH_EDGE)),
        "there is no edge bus → src"
    );
    assert!(matches!(rig.automation.add_lane(level_lane(src, 9999)), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    rig.automation.add_lane(level_lane(src, bus)).unwrap();
    assert!(rig.automation.add_lane(level_lane(src, bus)).is_err(), "a second lane for the same edge");
    rig.graph.set_edge_pre_fader(src, bus, true).unwrap();
    assert!(has_lane(&rig, &level_lane(src, bus)), "the send kind leaves the lane alone");

    rig.graph.disconnect(src, bus, false).unwrap();
    assert!(!has_lane(&rig, &level_lane(src, bus)), "the edge is removed, and so is the lane");
    assert!(rig.service.undo());
    assert!(has_lane(&rig, &level_lane(src, bus)), "undo brings the edge back with its lane");

    rig.graph.remove_node(bus).unwrap();
    assert!(rig.automation.lanes().is_empty(), "removing a node removes the lanes of its edges");
}

#[test]
fn sidechain_levels_are_automatable_separately() {
    let (rig, src, _) = send_rig();
    let comp = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    rig.graph.connect(src, comp, 1.0, false).unwrap();
    rig.graph.connect(src, comp, 0.5, true).unwrap();
    let key = ParamTarget::EdgeLevel { from: src, to: comp, sidechain: true };
    rig.automation.add_lane(key.clone()).unwrap();
    rig.automation.add_lane(level_lane(src, comp)).unwrap();
    rig.graph.disconnect(src, comp, false).unwrap();
    assert!(has_lane(&rig, &key) && !has_lane(&rig, &level_lane(src, comp)), "only the lane of the removed edge goes away");
}

#[test]
fn edge_level_lanes_follow_edges_moved_by_effect_insertion() {
    let (rig, src, bus) = send_rig();
    rig.automation.add_lane(level_lane(src, MASTER)).unwrap();
    rig.automation.add_lane(level_lane(bus, MASTER)).unwrap();
    let fx = rig.graph.insert_effect(src, "eq", 0.0, 0.0).unwrap();
    assert!(has_lane(&rig, &level_lane(fx, MASTER)), "the edge src → master moved to the effect with its lane");
    let limiter = rig.graph.insert_effect_before(MASTER, "eq", 0.0, 0.0).unwrap();
    assert!(has_lane(&rig, &level_lane(bus, limiter)), "the master input moved to the effect with its lane");
    rig.graph.remove_effect_keep_path(limiter).unwrap();
    assert_eq!(rig.automation.lanes().len(), 0, "the effect's edges are gone, and so are their lanes");
}

#[test]
fn removing_an_effect_keeps_the_pre_fader_kind_of_its_input() {
    let (rig, src, bus) = send_rig();
    let fx = rig.graph.add_effect("eq", 0.0, 0.0).unwrap();
    rig.graph.connect(src, fx, 1.0, false).unwrap();
    rig.graph.connect(fx, MASTER, 1.0, false).unwrap();
    rig.graph.disconnect(src, MASTER, false).unwrap();
    rig.graph.set_edge_pre_fader(src, fx, true).unwrap();
    rig.graph.remove_effect_keep_path(fx).unwrap();
    assert!(edge(&rig, src, MASTER).pre_fader);
    assert!(!edge(&rig, src, bus).pre_fader);
}
