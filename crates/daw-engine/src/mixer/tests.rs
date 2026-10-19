//! Mixer tests through its interface, without `EngineLink`.

use daw_contracts::{Edge, EngineSnapshot, MASTER, Node, Project, Strip};

use super::*;

fn snapshot(project: Project) -> EngineSnapshot {
    EngineSnapshot { version: 1, order: project.graph.order(), project, samples: Default::default(), clip_audio: Default::default() }
}

#[test]
fn rebuilding_tables_stays_within_preallocated_capacity() {
    let mut project = Project::empty();
    for id in 10..10 + 2 * MAX_NODES as u32 {
        project.graph.nodes.push(Node {
            id,
            name: String::new(),
            kind: NodeKind::Bus { strip: Strip { volume: 1.0, pan: 0.0, mute: false, solo: false, ..Strip::default() } },
            x: 0.0,
            y: 0.0,
        });
        for _ in 0..8 {
            project.graph.edges.push(Edge { from: id, to: 0, level: 1.0, sidechain: false, pre_fader: false });
        }
    }
    let mut mixer = GraphMixer::new();
    let board = Meters::new();
    let capacities =
        |m: &GraphMixer| (m.nodes.capacity(), m.edges.capacity(), m.order.capacity(), m.starts.capacity(), m.lookup.capacity());
    let before = capacities(&mixer);
    let first = snapshot(project.clone());
    mixer.apply_snapshot(&first, None, &board);
    mixer.apply_snapshot(&snapshot(project), Some(&first), &board);
    assert_eq!(before, capacities(&mixer), "the tables did not grow");
    assert_eq!(mixer.nodes.len(), MAX_NODES);
    assert_eq!(mixer.edges.len(), MAX_EDGES);
    mixer.clear(256);
    mixer.process(256, &board, &NodeTap::new(std::sync::Arc::new(crate::scope::FrameClock::new())));
    assert_eq!(board.take().len(), MAX_NODES, "the meters took all slots: the master and 255 buses");
}

#[test]
fn nodes_are_found_by_id_whatever_their_order_in_the_snapshot() {
    let mut project = Project::empty();
    for id in [40, 7, 23] {
        project.graph.nodes.push(Node {
            id,
            name: String::new(),
            kind: NodeKind::Bus { strip: Strip { volume: 1.0, pan: 0.0, mute: false, solo: false, ..Strip::default() } },
            x: 0.0,
            y: 0.0,
        });
    }
    let mut mixer = GraphMixer::new();
    mixer.apply_snapshot(&snapshot(project), None, &Meters::new());
    for id in [MASTER, 40, 7, 23] {
        let index = mixer.index_of(id).expect("the node is found");
        assert_eq!(mixer.nodes[index].id, id);
    }
    assert_eq!(mixer.index_of(99), None);
}
