use super::*;

fn node(id: NodeId, kind: NodeKind) -> Node {
    Node { id, name: format!("n{id}"), kind, x: 0.0, y: 0.0 }
}
fn instrument(id: NodeId) -> Node {
    node(id, NodeKind::Instrument { track: id, strip: Strip { volume: 1.0, ..Strip::default() } })
}
fn effect(id: NodeId) -> Node {
    node(id, NodeKind::Effect { effect: "reverb".into(), bypass: false, params: Params::new(), state: None })
}
fn bus(id: NodeId) -> Node {
    node(id, NodeKind::Bus { strip: Strip { volume: 1.0, ..Strip::default() } })
}
fn edge(from: NodeId, to: NodeId) -> Edge {
    Edge { from, to, level: 1.0, sidechain: false, pre_fader: false }
}

/// 10 instrument, 20 and 21 effects, 30 and 31 buses, 0 master.
fn sample() -> Graph {
    Graph {
        nodes: vec![node(MASTER, NodeKind::Master { volume: 1.0 }), instrument(10), effect(20), effect(21), bus(30), bus(31)],
        edges: vec![edge(10, 20), edge(20, 30), edge(30, MASTER)],
    }
}

fn fails_with(result: Result<(), ContractError>, code: crate::ErrorCode) -> bool {
    matches!(result, Err(e) if e.is(code))
}

#[test]
fn sources_only_give_master_only_takes_processors_connect_freely() {
    let g = sample();
    assert!(g.check_edge(10, 21, false).is_ok(), "instrument → effect");
    assert!(g.check_edge(10, 30, false).is_ok(), "instrument → bus directly");
    assert!(g.check_edge(20, 21, false).is_ok(), "effect → effect");
    assert!(g.check_edge(30, 31, false).is_ok(), "bus → bus");
    assert!(g.check_edge(30, 21, false).is_ok(), "bus → effect (compressor on a bus)");
    assert!(g.check_edge(21, MASTER, false).is_ok(), "effect → master (limiter on the master)");
    assert!(g.check_edge(31, MASTER, false).is_ok());

    assert!(fails_with(g.check_edge(20, 10, false), codes::SOURCE_HAS_NO_INPUT), "nothing goes into a source");
    assert!(fails_with(g.check_edge(MASTER, 21, false), codes::MASTER_HAS_NO_OUTPUT), "nothing comes out of the master");
    assert!(g.check_edge(10, 10, false).is_err(), "node to itself");
    assert!(g.check_edge(10, 99, false).is_err(), "missing node");
}

#[test]
fn kind_helpers_name_inputs_and_outputs() {
    let g = sample();
    let kind = |id| &g.node(id).unwrap().kind;
    assert!(kind(10).is_source() && !kind(10).accepts_input() && kind(10).has_output());
    for id in [20, 30] {
        assert!(!kind(id).is_source() && kind(id).accepts_input() && kind(id).has_output());
    }
    assert!(kind(MASTER).accepts_input() && !kind(MASTER).has_output());
}

#[test]
fn cycles_are_rejected_between_any_processors() {
    let mut g = sample();
    g.edges.push(edge(20, 21));
    assert!(fails_with(g.check_edge(21, 20, false), codes::EDGE_MAKES_LOOP));
    g.edges.push(edge(30, 31));
    assert!(fails_with(g.check_edge(31, 30, false), codes::EDGE_MAKES_LOOP));
    // Edges: 10 → 20 → 30 → 31 and 20 → 21. Bus → the effect before it is a loop; an effect on another branch is fine.
    assert!(fails_with(g.check_edge(30, 20, false), codes::EDGE_MAKES_LOOP), "bus → the effect before it");
    assert!(fails_with(g.check_edge(31, 20, false), codes::EDGE_MAKES_LOOP), "through two buses is a loop too");
    assert!(g.check_edge(31, 21, false).is_ok(), "21 does not lead to 31: no loop");
    assert!(fails_with(g.check_edge(31, 20, true), codes::EDGE_MAKES_LOOP), "a sidechain cannot close a loop either");
}

#[test]
fn sidechain_goes_only_into_effects() {
    let g = sample();
    assert!(g.check_edge(10, 21, true).is_ok());
    assert!(fails_with(g.check_edge(10, 30, true), codes::SIDECHAIN_NEEDS_EFFECT));
}

#[test]
fn order_respects_edges_and_is_deterministic() {
    let g = sample();
    let order = g.order();
    assert_eq!(order.len(), 6, "every node, even unconnected ones (21, 31)");
    let at = |id| order.iter().position(|n| *n == id).unwrap();
    assert!(at(10) < at(20) && at(20) < at(30) && at(30) < at(MASTER));
    assert_eq!(g.order(), order);
}

#[test]
fn order_is_topological_when_buses_feed_effects() {
    // 10 → 30 → 21 → 31 → 20 → master: bus → effect → bus → effect (limiter) → master.
    let mut g = sample();
    g.edges = vec![edge(10, 30), edge(30, 21), edge(21, 31), edge(31, 20), edge(20, MASTER)];
    let order = g.order();
    let at = |id| order.iter().position(|n| *n == id).unwrap();
    assert!(at(10) < at(30) && at(30) < at(21) && at(21) < at(31) && at(31) < at(20) && at(20) < at(MASTER));
    assert_eq!(order.len(), 6);
}

#[test]
fn upstream_collects_everything_that_feeds_a_node() {
    let mut g = sample();
    g.edges.push(edge(21, 30));
    let up = g.upstream(30);
    assert_eq!(up.into_iter().collect::<Vec<_>>(), [10, 20, 21]);
    assert!(g.upstream(10).is_empty());
    assert_eq!(g.upstream(MASTER).len(), 4, "10, 20, 21 and bus 30");
}

#[test]
fn graph_survives_a_json_round_trip() {
    let g = sample();
    let text = serde_json::to_string(&g).unwrap();
    assert!(text.contains(r#""type":"instrument""#) && text.contains(r#""type":"master""#));
    assert_eq!(serde_json::from_str::<Graph>(&text).unwrap(), g);
}

#[test]
fn strip_fields_sit_flat_in_the_node_json() {
    let kind = NodeKind::Instrument { track: 3, strip: Strip { volume: 0.5, pan: -0.25, mute: true, solo: false, ..Strip::default() } };
    let text = serde_json::to_string(&kind).unwrap();
    assert_eq!(
        text,
        r#"{"type":"instrument","track":3,"volume":0.5,"pan":-0.25,"mute":true,"solo":false,"polarity":{"invertLeft":false,"invertRight":false},"width":1.0,"mono":false,"delayMs":0.0}"#
    );
    assert_eq!(serde_json::from_str::<NodeKind>(&text).unwrap(), kind);
    let utility =
        Strip { polarity: Polarity { invert_left: true, invert_right: false }, width: 1.5, mono: true, delay_ms: -2.5, ..Strip::default() };
    let bus = serde_json::to_string(&NodeKind::Bus { strip: utility }).unwrap();
    assert_eq!(
        bus,
        r#"{"type":"bus","volume":0.8,"pan":0.0,"mute":false,"solo":false,"polarity":{"invertLeft":true,"invertRight":false},"width":1.5,"mono":true,"delayMs":-2.5}"#
    );
    assert_eq!(serde_json::from_str::<NodeKind>(&bus).unwrap(), NodeKind::Bus { strip: utility });
    let mut master = NodeKind::Master { volume: 1.0 };
    assert!(master.strip_mut().is_none());
    *master.volume_mut().unwrap() = 0.5;
    assert_eq!(master, NodeKind::Master { volume: 0.5 });
}

#[test]
fn chain_follows_a_single_path_through_effects() {
    let mut g = sample();
    // 10 → 20 → 30 → master: effect 20, output bus 30.
    let chain = g.chain(10);
    assert_eq!(chain, TrackChain { effects: vec![20], output: Some(30) });
    g.edges.push(edge(20, 21));
    g.edges.retain(|e| !(e.from == 20 && e.to == 30));
    g.edges.push(edge(21, 31));
    assert_eq!(g.chain(10), TrackChain { effects: vec![20, 21], output: Some(31) });
    // Branching: the path is no longer a single line.
    g.edges.push(edge(21, 30));
    assert_eq!(g.chain(10), TrackChain { effects: vec![20, 21], output: None });
    // A node without connections.
    assert_eq!(sample().chain(31), TrackChain::default());
}

#[test]
fn sources_are_found_by_track_and_have_faders() {
    let g = sample();
    assert_eq!(g.source_of(10).map(|n| n.id), Some(10));
    assert!(g.source_of(99).is_none());
    let kind = &g.node(10).unwrap().kind;
    assert!(kind.has_volume() && kind.has_strip());
    assert!(!g.node(20).unwrap().kind.has_volume());
    assert!(g.node(MASTER).unwrap().kind.has_volume() && !g.node(MASTER).unwrap().kind.has_strip());
}

#[test]
fn pre_fader_only_from_strips_and_sends_stay_out_of_chain() {
    let mut g = sample();
    assert!(g.check_pre_fader(10).is_ok(), "source");
    assert!(g.check_pre_fader(30).is_ok(), "bus");
    assert!(fails_with(g.check_pre_fader(20), codes::PRE_FADER_NEEDS_STRIP), "effect");
    assert!(g.check_pre_fader(MASTER).is_err(), "master");
    assert!(matches!(g.check_pre_fader(99), Err(e) if e.is(codes::NODE_NOT_FOUND)));

    // A pre-fader send to bus 31 is a branch: the track chain still goes through effect 20.
    g.edges.push(Edge { pre_fader: true, ..edge(10, 31) });
    assert_eq!(g.chain(10), TrackChain { effects: vec![20], output: Some(30) });
}

#[test]
fn edge_json_says_pre_fader_and_defaults_to_after_the_fader() {
    let text = serde_json::to_value(Edge { pre_fader: true, ..edge(10, 30) }).unwrap();
    assert_eq!(text["preFader"], true);
    let old: Edge = serde_json::from_str(r#"{"from":10,"to":30,"level":1.0,"sidechain":false}"#).unwrap();
    assert!(!old.pre_fader);
}

#[test]
fn edge_level_target_json_names_the_edge_and_defaults_to_a_normal_one() {
    let target = crate::ParamTarget::EdgeLevel { from: 3, to: 4, sidechain: false };
    assert_eq!(serde_json::to_value(&target).unwrap(), serde_json::json!({ "type": "edgeLevel", "from": 3, "to": 4, "sidechain": false }));
    let short: crate::ParamTarget = serde_json::from_str(r#"{ "type": "edgeLevel", "from": 3, "to": 4 }"#).unwrap();
    assert_eq!(short, target);
}

#[test]
fn insert_path_ignores_sends_and_stops_at_a_shared_effect() {
    let mut g = sample();
    g.nodes.push(instrument(11));
    // The send 10 → 31 is the second connection: the path follows the first, through insert 20 into bus 30.
    g.edges.push(edge(10, 31));
    assert_eq!(g.insert_path(10), TrackChain { effects: vec![20], output: Some(30) });
    assert_eq!(g.chain(10).output, None, "`chain` branches at a send, `insert_path` does not");
    // Another track feeds effect 20: it is shared, so it is the output, not an insert.
    g.edges.push(edge(11, 20));
    assert_eq!(g.insert_path(10), TrackChain { effects: vec![], output: Some(20) });
    assert_eq!(g.insert_path(MASTER), TrackChain::default(), "the master has no path");
}
