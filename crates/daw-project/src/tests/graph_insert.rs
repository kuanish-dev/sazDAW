//! The `GraphControl` contract: effects after buses and on the master (`insert_effect` after a bus,
//! `insert_effect_before`).

use super::*;

fn level(graph: &Graph, from: NodeId, to: NodeId) -> Option<f32> {
    graph.edges.iter().find(|e| e.from == from && e.to == to && !e.sidechain).map(|e| e.level)
}

#[test]
fn bus_compressor_goes_after_the_bus() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let src = source(&rig, track);
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.set_output(src, bus).unwrap();

    let comp = rig.graph.insert_effect(bus, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let graph = rig.graph.graph();
    assert_eq!((level(&graph, bus, comp), level(&graph, comp, MASTER)), (Some(1.0), Some(1.0)), "bus → effect → master");
    assert_eq!(level(&graph, bus, MASTER), None);
    assert_eq!(graph.chain(bus), TrackChain { effects: vec![comp], output: Some(MASTER) });
    let order = graph.order();
    let at = |id| order.iter().position(|n| *n == id).unwrap();
    assert!(at(src) < at(bus) && at(bus) < at(comp) && at(comp) < at(MASTER), "the processing order follows the edges");
}

#[test]
fn master_limiter_takes_every_master_input() {
    let rig = Rig::new();
    let a = source(&rig, rig.tracks.add_track("A", Some("synth"), 60).unwrap());
    let b = source(&rig, rig.tracks.add_track("B", None, 60).unwrap());
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.connect(b, MASTER, 0.4, false).unwrap();

    let limiter = rig.graph.insert_effect_before(MASTER, "eq", &Params::new(), 7.0, 8.0).unwrap();
    let graph = rig.graph.graph();
    let node = graph.node(limiter).unwrap();
    assert_eq!((node.x, node.y), (7.0, 8.0));
    assert_eq!(
        graph.incoming(MASTER).collect::<Vec<_>>(),
        [&Edge { from: limiter, to: MASTER, level: 1.0, sidechain: false, pre_fader: false }]
    );
    assert_eq!(level(&graph, a, limiter), Some(1.0));
    assert_eq!(level(&graph, b, limiter), Some(0.4), "input levels are kept");
    assert_eq!(level(&graph, bus, limiter), Some(1.0));

    assert!(rig.service.undo(), "inserting is one undo step");
    let graph = rig.graph.graph();
    assert!(graph.node(limiter).is_none());
    assert_eq!(level(&graph, b, MASTER), Some(0.4));
}

#[test]
fn insert_before_a_bus_keeps_sidechains_and_chains_in_front() {
    let rig = Rig::new();
    let src = source(&rig, rig.tracks.add_track("Synth", Some("synth"), 60).unwrap());
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    rig.graph.set_output(src, bus).unwrap();
    let first = rig.graph.insert_effect_before(bus, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let key = source(&rig, rig.tracks.add_track("Kick", Some("synth"), 60).unwrap());
    rig.graph.connect(key, first, 0.5, true).unwrap();
    let second = rig.graph.insert_effect_before(first, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let graph = rig.graph.graph();
    assert_eq!(graph.chain(src), TrackChain { effects: vec![second, first], output: Some(bus) });
    assert!(graph.edges.contains(&Edge { from: key, to: first, level: 0.5, sidechain: true, pre_fader: false }));
}

#[test]
fn insert_before_validates_and_failures_change_nothing() {
    let rig = Rig::new();
    let src = source(&rig, rig.tracks.add_track("Synth", Some("synth"), 60).unwrap());
    let before = rig.service.snapshot();
    assert!(
        matches!(rig.graph.insert_effect_before(src, "eq", &Params::new(), 0.0, 0.0), Err(e) if e.is(codes::graph::SOURCE_HAS_NO_INPUT)),
        "a source has no inputs"
    );
    assert!(
        matches!(rig.graph.insert_effect_before(MASTER, "nope", &Params::new(), 0.0, 0.0), Err(e) if e.is(codes::graph::UNKNOWN_EFFECT))
    );
    assert!(matches!(rig.graph.insert_effect_before(9999, "eq", &Params::new(), 0.0, 0.0), Err(e) if e.is(codes::graph::NODE_NOT_FOUND)));
    let nan = rig.graph.insert_effect_before(MASTER, "eq", &Params::new(), f32::NAN, 0.0);
    assert!(matches!(nan, Err(e) if e.is(codes::graph::POSITION_NOT_A_NUMBER)));
    assert_eq!(rig.service.snapshot(), before);

    while rig.graph.graph().nodes.len() < MAX_NODES {
        rig.graph.add_effect("eq", &Params::new(), 0.0, 0.0).unwrap();
    }
    assert!(matches!(rig.graph.insert_effect_before(MASTER, "eq", &Params::new(), 0.0, 0.0), Err(e) if e.is(codes::graph::TOO_MANY_NODES)));
}

#[test]
fn cycles_through_buses_and_effects_are_rejected() {
    let rig = Rig::new();
    let bus = rig.graph.add_bus("Bus", 0.0, 0.0).unwrap();
    let fx = rig.graph.insert_effect(bus, "eq", &Params::new(), 0.0, 0.0).unwrap();
    let err = rig.graph.connect(fx, bus, 1.0, false).unwrap_err();
    assert!(err.is(codes::graph::EDGE_MAKES_LOOP), "{err}");
    let other = rig.graph.add_bus("Other", 0.0, 0.0).unwrap();
    rig.graph.connect(fx, other, 1.0, false).unwrap();
    assert!(rig.graph.connect(other, bus, 1.0, false).is_err(), "bus → effect → bus → back");
}
