//! Compiling a graph snapshot into the mixer tables: nodes, connections, processing order, audibility under solo,
//! effect binding, meter slots, channel utility slots, stage voices. Runs on the audio side at a block boundary: the
//! tables are refilled within the preallocated capacity, nodes are found by binary search over sorted tables.

use daw_contracts::{EngineSnapshot, Graph, MAX_EDGES, MAX_NODES, NodeId, NodeKind, Params};

use super::{EdgeRt, GraphMixer, NodeRt, Role, StageRun, StereoGain, find_index};
use crate::meters::{MAX_METERS, Meters};

/// What a previous node hands over to the new one with the same id.
#[derive(Clone, Copy)]
struct Carry {
    id: NodeId,
    /// The node index in the previous snapshot (the node table follows the snapshot node order).
    index: usize,
    gain: StereoGain,
    /// The channel utility slot: its delay lines keep playing the channel's sound.
    utility_slot: Option<usize>,
}

/// Rebuild work tables: preallocated so as not to allocate in the audio thread.
pub(super) struct Scratch {
    /// The previous nodes by ascending id.
    previous: Vec<Carry>,
    /// Connections `(from, connection)` before they are laid out by source.
    resolved: Vec<(usize, EdgeRt)>,
    cursor: Vec<usize>,
    seen: Vec<bool>,
    /// Latency compensation (`latency.rs`): when the sound arrives at a node's inputs and when it leaves it.
    pub(super) arrival: Vec<u32>,
    pub(super) output: Vec<u32>,
    /// Regular (not sidechain) inputs per node, and whether a node's sound reaches the master (stage binding).
    inputs: Vec<u16>,
    reaches_master: Vec<bool>,
}

impl Scratch {
    pub(super) fn new() -> Self {
        Self {
            previous: Vec::with_capacity(MAX_NODES),
            resolved: Vec::with_capacity(MAX_EDGES),
            cursor: Vec::with_capacity(MAX_NODES + 1),
            seen: Vec::with_capacity(MAX_NODES),
            arrival: Vec::with_capacity(MAX_NODES),
            output: Vec::with_capacity(MAX_NODES),
            inputs: Vec::with_capacity(MAX_NODES),
            reaches_master: Vec::with_capacity(MAX_NODES),
        }
    }

    fn carried(&self, id: NodeId) -> Option<Carry> {
        let found = self.previous.binary_search_by_key(&id, |c| c.id).ok()?;
        Some(self.previous[found])
    }
}

impl GraphMixer {
    /// Rebuilds the tables from the snapshot. Effects get the params changed since `previous` (the snapshot the current
    /// tables were built from); without `previous`, all of them.
    pub fn apply_snapshot(&mut self, snapshot: &EngineSnapshot, previous: Option<&EngineSnapshot>, meters: &Meters) {
        let graph = &snapshot.project.graph;
        self.rebuild_nodes(graph);
        self.bind_utilities();
        self.rebuild_edges(graph);
        self.rebuild_order(&snapshot.order);
        self.mark_heard();
        self.bind_effects(graph, previous.map(|p| &p.project.graph));
        self.assign_meters(meters);
        self.stage.set_room(snapshot.project.room);
        self.bind_stage();
    }

    /// The snapshot nodes; the gains and channel utility slots of previous nodes are kept so edits do not click.
    fn rebuild_nodes(&mut self, graph: &Graph) {
        let scratch = &mut self.scratch;
        scratch.previous.clear();
        scratch.previous.extend(self.nodes.iter().enumerate().map(|(index, n)| Carry {
            id: n.id,
            index,
            gain: n.gain,
            utility_slot: n.utility_slot,
        }));
        scratch.previous.sort_unstable_by_key(|c| c.id);

        self.nodes.clear();
        for spec in graph.nodes.iter().take(MAX_NODES) {
            let (volume, pan) = match &spec.kind {
                NodeKind::Master { volume } => (*volume, 0.0),
                NodeKind::Effect { .. } => (1.0, 0.0),
                kind => kind.strip().map_or((1.0, 0.0), |strip| (strip.volume, strip.pan)),
            };
            let strip = spec.kind.strip();
            let utility = strip.copied().unwrap_or_default();
            let carried = self.scratch.carried(spec.id);
            self.nodes.push(NodeRt {
                id: spec.id,
                role: Role::of(&spec.kind),
                volume,
                pan,
                polarity: utility.polarity,
                width: utility.width,
                mono: utility.mono,
                delay_ms: utility.delay_ms,
                utility_slot: carried.and_then(|c| c.utility_slot).filter(|_| strip.is_some()),
                place: utility.stage,
                stage_after: None,
                effect: None,
                meter: None,
                gain: carried.map_or_else(StereoGain::default, |c| c.gain),
                volume_override: None,
                pan_override: None,
                width_override: None,
                depth_override: None,
                height_override: None,
            });
        }
        self.lookup.clear();
        self.lookup.extend(self.nodes.iter().enumerate().map(|(index, n)| (n.id, index)));
        self.lookup.sort_unstable_by_key(|(id, _)| *id);
        self.any_solo = self.nodes.iter().any(|n| n.role.strip().is_some_and(|(_, solo)| solo));
        self.master = self.nodes.iter().position(|n| matches!(n.role, Role::Master)).unwrap_or(MAX_NODES);
    }

    /// Connections into a flat table grouped by source.
    fn rebuild_edges(&mut self, graph: &Graph) {
        let count = self.nodes.len();
        self.scratch.resolved.clear();
        for edge in graph.edges.iter().take(MAX_EDGES) {
            if let (Some(from), Some(to)) = (self.index_of(edge.from), self.index_of(edge.to))
                && from != to
            {
                let pre_fader = edge.pre_fader && !edge.sidechain && self.nodes[from].role.strip().is_some();
                self.scratch.resolved.push((from, EdgeRt::new(to, edge.level, edge.sidechain, pre_fader)));
            }
        }
        self.starts.clear();
        self.starts.resize(count + 1, 0);
        for (from, _) in &self.scratch.resolved {
            self.starts[from + 1] += 1;
        }
        for i in 0..count {
            self.starts[i + 1] += self.starts[i];
        }
        let cursor = &mut self.scratch.cursor;
        cursor.clear();
        cursor.extend_from_slice(&self.starts);
        self.edges.clear();
        self.edges.resize(self.scratch.resolved.len(), EdgeRt::new(0, 0.0, false, false));
        for (from, edge) in &self.scratch.resolved {
            self.edges[cursor[*from]] = *edge;
            cursor[*from] += 1;
        }
    }

    /// The processing order comes ready; unknown and repeated nodes are skipped.
    fn rebuild_order(&mut self, order: &[NodeId]) {
        self.order.clear();
        let seen = &mut self.scratch.seen;
        seen.clear();
        seen.resize(self.nodes.len(), false);
        for id in order.iter().take(MAX_NODES * 2) {
            if let Some(index) = find_index(&self.lookup, *id)
                && !seen[index]
            {
                seen[index] = true;
                self.order.push(index);
            }
        }
    }

    /// Binds DSP objects to the effect nodes and sends them the changed params.
    fn bind_effects(&mut self, graph: &Graph, previous: Option<&Graph>) {
        for (spec, node) in graph.nodes.iter().take(MAX_NODES).zip(&mut self.nodes) {
            let NodeKind::Effect { params, .. } = &spec.kind else {
                continue;
            };
            node.effect = self.effects.position(node.id);
            if let Some(position) = node.effect {
                let before = previous.and_then(|graph| previous_params(graph, self.scratch.carried(node.id)?));
                self.effects.apply_params(position, params, before);
            }
        }
    }

    /// Channel utility slots: a channel keeps the one it carried (its delay lines go on), a channel new to the mixer
    /// takes a free one, and the slots of nodes that left are freed.
    fn bind_utilities(&mut self) {
        self.utilities.begin();
        for slot in self.nodes.iter().filter_map(|node| node.utility_slot) {
            self.utilities.keep(slot);
        }
        for node in self.nodes.iter_mut().filter(|node| node.role.strip().is_some() && node.utility_slot.is_none()) {
            node.utility_slot = self.utilities.take();
        }
    }

    /// Meter slots go to buses and the master in node order; extra slots are freed.
    fn assign_meters(&mut self, meters: &Meters) {
        let mut used = 0;
        for node in &mut self.nodes {
            node.meter = None;
            if (node.role.strip().is_some() || matches!(node.role, Role::Master)) && used < MAX_METERS {
                node.meter = Some(used);
                meters.set_node(used, node.id);
                used += 1;
            }
        }
        meters.clear_from(used);
    }

    /// Binds a stage voice to every channel on the stage and runs it where the channel's insert path ends: from the node
    /// along its first regular post-fader connection while that leads into an effect with no other regular input (the
    /// rule of `Graph::insert_path` and of the mixer strips, `app/src/mixer/mixerStrips.ts`).
    fn bind_stage(&mut self) {
        let count = self.nodes.len();
        let inputs = &mut self.scratch.inputs;
        inputs.clear();
        inputs.resize(count, 0);
        for edge in self.edges.iter().filter(|e| !e.sidechain) {
            inputs[edge.to] = inputs[edge.to].saturating_add(1);
        }
        let reaches = &mut self.scratch.reaches_master;
        reaches.clear();
        reaches.resize(count, false);
        for &index in self.order.iter().rev() {
            reaches[index] = index == self.master
                || self.edges[self.starts[index]..self.starts[index + 1]].iter().any(|e| !e.sidechain && reaches[e.to]);
        }
        for node in &mut self.nodes {
            node.stage_after = None;
        }
        for node in self.nodes.iter().filter(|n| n.place.is_some()) {
            self.stage.keep(node.id);
        }
        self.stage.release_others();
        for owner in 0..count {
            if self.nodes[owner].place.is_none() {
                continue;
            }
            let Some(slot) = self.stage.bind(self.nodes[owner].id) else {
                continue;
            };
            let end = self.insert_path_end(owner);
            self.nodes[end].stage_after = Some(StageRun { owner, slot, feeds_room: self.scratch.reaches_master[end] });
        }
    }

    /// The last node of `start`'s insert path (the node itself when it has no inserts).
    fn insert_path_end(&self, start: usize) -> usize {
        let mut end = start;
        for _ in 0..self.nodes.len() {
            let Some(edge) = self.edges[self.starts[end]..self.starts[end + 1]].iter().find(|e| !e.sidechain && !e.pre_fader) else {
                break;
            };
            if !matches!(self.nodes[edge.to].role, Role::Effect { .. }) || self.scratch.inputs[edge.to] != 1 {
                break;
            }
            end = edge.to;
        }
        end
    }

    /// Which nodes are audible under solo. Solo on a node keeps the path around it audible: everything that flows into
    /// it ("upstream") and everything that flows out of it ("downstream"): solo on a group bus does not mute its
    /// instruments, solo on a track does not mute the bus it goes to. The other nodes are silent.
    fn mark_heard(&mut self) {
        let count = self.nodes.len();
        self.heard.clear();
        self.heard.resize(count, true);
        if !self.any_solo {
            return;
        }
        let solo = |node: &NodeRt| node.role.strip().is_some_and(|(_, solo)| solo);
        self.heard.fill(false);
        for &index in &self.order {
            if solo(&self.nodes[index]) {
                self.heard[index] = true;
            }
            if self.heard[index] {
                for edge in self.edges[self.starts[index]..self.starts[index + 1]].iter().filter(|e| !e.sidechain) {
                    self.heard[edge.to] = true;
                }
            }
        }
        let leads_to_solo = &mut self.scratch.seen;
        leads_to_solo.clear();
        leads_to_solo.resize(count, false);
        for &index in self.order.iter().rev() {
            let leads = solo(&self.nodes[index])
                || self.edges[self.starts[index]..self.starts[index + 1]].iter().filter(|e| !e.sidechain).any(|e| leads_to_solo[e.to]);
            if leads {
                leads_to_solo[index] = true;
                self.heard[index] = true;
            }
        }
    }
}

/// The effect params of the same node in the previous snapshot.
fn previous_params(graph: &Graph, carry: Carry) -> Option<&Params> {
    let spec = graph.nodes.get(carry.index).filter(|spec| spec.id == carry.id)?;
    match &spec.kind {
        NodeKind::Effect { params, .. } => Some(params),
        _ => None,
    }
}
