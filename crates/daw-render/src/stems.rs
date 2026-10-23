//! Stems: which buses are written as separate files, the graph for one bus's sound and its file name.

use std::path::{Path, PathBuf};

use daw_contracts::{Edge, Graph, MASTER, NodeId, NodeKind};

/// The graph for a bus stem: only what sounds in the bus (everything feeding it), the bus itself and the master.
/// The bus is always connected to the master and its mute/solo are cleared: the stem must sound whatever the bus says.
/// The bus is the master's only input: a track that sends to the bus and also plays to the master directly (or a
/// sidechain key that also plays) would otherwise leak its dry sound into the stem.
pub fn stem_graph(graph: &Graph, bus: NodeId) -> Graph {
    let mut keep = graph.upstream(bus);
    keep.insert(bus);
    keep.insert(MASTER);
    let nodes = graph
        .nodes
        .iter()
        .filter(|n| keep.contains(&n.id))
        .cloned()
        .map(|mut node| {
            if node.id == bus
                && let Some(strip) = node.kind.strip_mut()
            {
                strip.mute = false;
                strip.solo = false;
            }
            node
        })
        .collect();
    let mut edges: Vec<Edge> = graph
        .edges
        .iter()
        .filter(|e| keep.contains(&e.from) && keep.contains(&e.to) && (e.to != MASTER || e.from == bus))
        .copied()
        .collect();
    if !edges.iter().any(|e| e.from == bus && e.to == MASTER && !e.sidechain) {
        edges.push(Edge { from: bus, to: MASTER, level: 1.0, sidechain: false, pre_fader: false });
    }
    Graph { nodes, edges }
}

/// The project buses that get stems: `(node, name)`.
pub fn stem_buses(graph: &Graph) -> Vec<(NodeId, String)> {
    graph.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Bus { .. })).map(|n| (n.id, n.name.clone())).collect()
}

/// The stem file next to the main one, with the same extension (format): `<main>_<node>_<bus name>.<extension>`. The
/// node id makes the name unique: buses may share a name, and different names may match after replacing characters
/// (`/`, space...).
pub fn stem_path(base: &Path, bus: NodeId, name: &str) -> PathBuf {
    let stem = base.file_stem().map_or_else(|| "mix".to_string(), |s| s.to_string_lossy().into_owned());
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let extension = base.extension().map_or_else(|| "wav".to_string(), |e| e.to_string_lossy().into_owned());
    base.with_file_name(format!("{stem}_{bus}_{safe}.{extension}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{Node, Strip};

    fn node(id: NodeId, kind: NodeKind) -> Node {
        Node { id, name: format!("n{id}"), kind, x: 0.0, y: 0.0 }
    }

    fn instrument(track: u32) -> NodeKind {
        NodeKind::Instrument { track, strip: Strip::default() }
    }

    fn bus(id: NodeId) -> Node {
        node(id, NodeKind::Bus { strip: Strip { volume: 1.0, pan: 0.0, mute: true, solo: true, ..Strip::default() } })
    }

    fn effect(id: NodeId) -> Node {
        node(id, NodeKind::Effect { effect: "reverb".into(), bypass: false, params: Default::default(), state: None })
    }

    fn edge(from: NodeId, to: NodeId) -> Edge {
        Edge { from, to, level: 1.0, sidechain: false, pre_fader: false }
    }

    /// Instruments 10, 11, 12; effect 20; buses 30 (fed by 10 through 20, and 11), 31 (fed by 30 and 12), 32 (unrelated).
    fn sample_graph() -> Graph {
        Graph {
            nodes: vec![
                node(MASTER, NodeKind::Master { volume: 1.0 }),
                node(10, instrument(10)),
                node(11, instrument(11)),
                node(12, instrument(12)),
                effect(20),
                bus(30),
                bus(31),
                bus(32),
            ],
            edges: vec![edge(10, 20), edge(20, 30), edge(11, 30), edge(30, 31), edge(12, 31), edge(31, MASTER), edge(32, MASTER)],
        }
    }

    fn ids(graph: &Graph) -> Vec<NodeId> {
        let mut ids: Vec<_> = graph.nodes.iter().map(|n| n.id).collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn stem_keeps_only_what_feeds_the_bus_plus_master() {
        let reduced = stem_graph(&sample_graph(), 30);
        assert_eq!(ids(&reduced), [MASTER, 10, 11, 20, 30]);
        assert!(reduced.edges.contains(&edge(10, 20)) && reduced.edges.contains(&edge(11, 30)));
        assert!(!reduced.edges.iter().any(|e| e.from == 31 || e.to == 31 || e.from == 12));
        assert!(reduced.edges.contains(&edge(30, MASTER)), "the bus is connected to the master");
        assert_eq!(reduced.edges.len(), 4);
    }

    #[test]
    fn stem_of_downstream_bus_includes_upstream_buses() {
        let reduced = stem_graph(&sample_graph(), 31);
        assert_eq!(ids(&reduced), [MASTER, 10, 11, 12, 20, 30, 31]);
        assert_eq!(reduced.edges.iter().filter(|e| e.from == 31 && e.to == MASTER).count(), 1, "the edge is not duplicated");
        assert!(reduced.edges.iter().all(|e| e.from != 32));
    }

    #[test]
    fn stem_holds_only_the_bus_sound_not_the_dry_sound_of_what_also_plays_to_the_master() {
        let graph = Graph {
            nodes: vec![
                node(MASTER, NodeKind::Master { volume: 1.0 }),
                node(10, instrument(10)),
                node(14, instrument(14)),
                effect(21),
                bus(30),
            ],
            edges: vec![
                edge(10, MASTER),
                edge(10, 21),
                edge(21, 30),
                edge(30, MASTER),
                Edge { sidechain: true, ..edge(14, 21) },
                edge(14, MASTER),
            ],
        };
        let reduced = stem_graph(&graph, 30);
        assert_eq!(reduced.incoming(MASTER).map(|e| e.from).collect::<Vec<_>>(), [30], "only the bus reaches the master");
        assert!(reduced.edges.contains(&edge(10, 21)), "the send stays");
        assert!(reduced.edges.contains(&Edge { sidechain: true, ..edge(14, 21) }), "the sidechain key stays");
        assert_eq!(reduced.order().len(), reduced.nodes.len());
    }

    #[test]
    fn stem_bus_is_audible_and_graph_stays_valid() {
        let reduced = stem_graph(&sample_graph(), 30);
        let strip = reduced.node(30).unwrap().kind.strip().copied().expect("node 30 is a bus");
        assert!(!strip.mute && !strip.solo);
        assert_eq!(reduced.order().len(), reduced.nodes.len());
    }

    #[test]
    fn stems_are_made_for_every_bus() {
        let buses = stem_buses(&sample_graph());
        assert_eq!(buses.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [30, 31, 32]);
        assert_eq!(buses[0].1, "n30");
    }

    #[test]
    fn stem_file_names_are_safe() {
        let path = stem_path(Path::new("/out/mix.wav"), 30, "Кик / Bass #1");
        assert_eq!(path, Path::new("/out/mix_30_Кик___Bass__1.wav"));
    }

    /// Buses with equal names (or names equal after replacing characters) used to write to one file.
    #[test]
    fn buses_with_clashing_names_get_different_files() {
        let base = Path::new("/out/mix.wav");
        assert_ne!(stem_path(base, 30, "Bass"), stem_path(base, 31, "Bass"));
        assert_ne!(stem_path(base, 30, "Kick/Bass"), stem_path(base, 31, "Kick Bass"));
    }
}
