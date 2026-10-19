//! The audio graph: nodes and connections. Sources (instruments, audio tracks) only produce sound, the master only
//! receives it; effects and buses (processors) connect to each other in any order as long as the graph has no cycles,
//! which allows both a compressor on a bus and a limiter on the master. The graph rules (`Graph::check_edge`) are
//! written down only here; the project enforces them, and the processing order (`Graph::order`) is computed once on
//! the control side and shipped to the engine in the snapshot.

use schemars::JsonSchema;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::codes::graph as codes;
use crate::{ContractError, Params, StagePlace, TrackId};

pub type NodeId = u32;

/// There is always exactly one master node, and it always has this id.
pub const MASTER: NodeId = 0;
/// The engine preallocates its tables to these limits.
pub const MAX_NODES: usize = 256;
pub const MAX_EDGES: usize = 1024;
/// Upper bound of connection level and volume (linear, 1.5 ≈ +3.5 dB).
pub const MAX_LEVEL: f32 = 1.5;
/// Volume of a new source, bus and master.
pub const DEFAULT_VOLUME: f32 = 0.8;
/// Upper bound of a channel's stereo width (`Strip::width`): twice the side of the sound as recorded.
pub const MAX_WIDTH: f32 = 2.0;
/// How far a channel's fine time alignment (`Strip::delay_ms`) reaches either way, in milliseconds: sound travels
/// about 3.4 m in that time, enough to line up close mics with overheads or a DI with its amp mic.
pub const MAX_CHANNEL_DELAY_MS: f32 = 10.0;

/// The mixer channel of a source or bus: volume, pan, mute, solo, the engineer's utility controls (polarity, width,
/// mono, fine time alignment) and where it stands on the virtual stage.
///
/// The utility controls work like a console's input section: the channel's sound passes them before its volume and
/// pan, so its pre-fader sends and its inserts hear the result too.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Strip {
    pub volume: f32,
    /// -1 (left) … 1 (right). On the stage: the left/right position, from the left wall to the right one.
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    /// Which sides play with their polarity inverted.
    pub polarity: Polarity,
    /// Stereo width, mid/side, `0..=MAX_WIDTH`: 0 is mono, 1 leaves the sound as it is, 2 doubles its side.
    pub width: f32,
    /// Sums the channel to mono (both sides play `(L + R) / 2`) whatever the width, which is kept for when it is off.
    pub mono: bool,
    /// Fine time alignment in milliseconds, `-MAX_CHANNEL_DELAY_MS..=MAX_CHANNEL_DELAY_MS`: positive plays the channel
    /// later, negative earlier. Earlier is reported to plugin delay compensation as the channel's latency, so the rest
    /// of the mix waits for it; later is a plain delay.
    pub delay_ms: f32,
    /// Where the channel stands on the virtual stage (`StageControl`); `None` is off the stage: plain pan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<StagePlace>,
}

impl Default for Strip {
    fn default() -> Self {
        Self {
            volume: DEFAULT_VOLUME,
            pan: 0.0,
            mute: false,
            solo: false,
            polarity: Polarity::default(),
            width: 1.0,
            mono: false,
            delay_ms: 0.0,
            stage: None,
        }
    }
}

/// A channel's polarity (often called "phase") invert, per side: an inverted side plays upside down. A mic under the
/// snare or behind an open-back cabinet picks the sound up inverted and cancels the mic on the other side; inverting
/// it makes them add up again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Polarity {
    pub invert_left: bool,
    pub invert_right: bool,
}

/// Input monitoring on an audio track: the input passes through the track's fader and effects in real time, on top of
/// its clips. Off by default: with speakers next to the microphone, monitoring causes feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum MonitorMode {
    #[default]
    Off,
    On,
    /// Only while audio recording is armed on the track.
    Auto,
}

impl MonitorMode {
    /// Whether a track in this mode hears the input, given whether recording is armed.
    pub fn hears_input(self, armed: bool) -> bool {
        match self {
            MonitorMode::Off => false,
            MonitorMode::On => true,
            MonitorMode::Auto => armed,
        }
    }
}

/// What a node does. A source only produces sound, the master only receives it, effects and buses do both. Channel
/// fields (`Strip`) sit directly in the node in JSON (`flatten`): `{ "type": "bus", "volume": …, "pan": …, … }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeKind {
    /// Source: the sound of a track's instrument. Created and removed together with the track; the track fader lives
    /// here.
    Instrument {
        track: TrackId,
        #[serde(flatten)]
        strip: Strip,
    },
    /// Source: an audio track's clips and (when monitoring) the input. Created and removed together with the track; the
    /// track fader lives here.
    Audio {
        track: TrackId,
        #[serde(flatten)]
        strip: Strip,
        #[serde(default)]
        monitor: MonitorMode,
    },
    /// A single effect. `state` is the saved plugin state (like `InstrumentSpec::state`); `params` are applied on top.
    Effect {
        effect: String,
        bypass: bool,
        params: Params,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<std::sync::Arc<crate::PluginState>>,
    },
    /// Sum of its inputs with volume, pan, mute and solo.
    Bus {
        #[serde(flatten)]
        strip: Strip,
    },
    /// The final mix; it has no outputs.
    Master { volume: f32 },
}

impl NodeKind {
    /// A sound source (instrument or audio track): it has no inputs.
    pub fn is_source(&self) -> bool {
        matches!(self, NodeKind::Instrument { .. } | NodeKind::Audio { .. })
    }

    /// Whether the node accepts sound: all but sources.
    pub fn accepts_input(&self) -> bool {
        !self.is_source()
    }

    /// Whether the node passes sound on: all but the master.
    pub fn has_output(&self) -> bool {
        !matches!(self, NodeKind::Master { .. })
    }

    /// The track a source node belongs to.
    pub fn track(&self) -> Option<TrackId> {
        match self {
            NodeKind::Instrument { track, .. } | NodeKind::Audio { track, .. } => Some(*track),
            _ => None,
        }
    }

    /// Whether the node has a volume (source, bus, master).
    pub fn has_volume(&self) -> bool {
        !matches!(self, NodeKind::Effect { .. })
    }

    /// The node's mixer channel (source and bus; the master has only a volume, an effect has nothing).
    pub fn strip(&self) -> Option<&Strip> {
        match self {
            NodeKind::Instrument { strip, .. } | NodeKind::Audio { strip, .. } | NodeKind::Bus { strip } => Some(strip),
            NodeKind::Effect { .. } | NodeKind::Master { .. } => None,
        }
    }

    pub fn strip_mut(&mut self) -> Option<&mut Strip> {
        match self {
            NodeKind::Instrument { strip, .. } | NodeKind::Audio { strip, .. } | NodeKind::Bus { strip } => Some(strip),
            NodeKind::Effect { .. } | NodeKind::Master { .. } => None,
        }
    }

    /// Whether the node has pan, mute and solo (source and bus; the master has only a volume).
    pub fn has_strip(&self) -> bool {
        self.strip().is_some()
    }

    /// The node's volume (an effect has none).
    pub fn volume_mut(&mut self) -> Option<&mut f32> {
        match self {
            NodeKind::Master { volume } => Some(volume),
            other => other.strip_mut().map(|strip| &mut strip.volume),
        }
    }
}

/// A track chain: the effects in a row after a source and where it finally goes. The interface uses it to show the
/// signal path on one line ("Synth → Compressor → Reverb → Master").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackChain {
    /// Effects in order.
    pub effects: Vec<NodeId>,
    /// Where the chain goes (bus, master); `None` when the path branches or is not connected (see the Graph view).
    pub output: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    /// Position of the card in the node editor.
    pub x: f32,
    pub y: f32,
}

/// The output of node `from` goes to the input of node `to` at level `level`.
/// `sidechain` feeds the signal into effect `to` as a control (key) input instead of the audio.
/// `pre_fader` is a pre-fader send: the sound is taken before the volume and pan of `from` (their moves and
/// automation do not affect the send), but the node's mute and another node's solo silence it like the whole node: a
/// muted track is silent entirely. Only connections from nodes with a fader (sources and buses,
/// `Graph::check_pre_fader`) can be pre-fader; the default is post-fader.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub level: f32,
    pub sidechain: bool,
    #[serde(default)]
    pub pre_fader: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn outgoing(&self, id: NodeId) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.from == id)
    }

    pub fn incoming(&self, id: NodeId) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.to == id)
    }

    /// Whether `from → to` can be connected; the error code says why not.
    pub fn check_edge(&self, from: NodeId, to: NodeId, sidechain: bool) -> Result<(), ContractError> {
        let source = self.node(from).ok_or_else(|| codes::NODE_NOT_FOUND.with("id", from))?;
        let target = self.node(to).ok_or_else(|| codes::NODE_NOT_FOUND.with("id", to))?;
        if from == to {
            return Err(codes::EDGE_TO_ITSELF.err());
        }
        if !source.kind.has_output() {
            return Err(codes::MASTER_HAS_NO_OUTPUT.err());
        }
        if !target.kind.accepts_input() {
            return Err(codes::SOURCE_HAS_NO_INPUT.err());
        }
        if sidechain && !matches!(target.kind, NodeKind::Effect { .. }) {
            return Err(codes::SIDECHAIN_NEEDS_EFFECT.err());
        }
        if self.reaches(to, from) {
            return Err(codes::EDGE_MAKES_LOOP.err());
        }
        Ok(())
    }

    /// Whether a connection from `from` can be a pre-fader send: only sources and buses have a fader.
    pub fn check_pre_fader(&self, from: NodeId) -> Result<(), ContractError> {
        let source = self.node(from).ok_or_else(|| codes::NODE_NOT_FOUND.with("id", from))?;
        if source.kind.has_strip() { Ok(()) } else { Err(codes::PRE_FADER_NEEDS_STRIP.err()) }
    }

    /// Whether `target` can be reached from `start` along connections of any kind.
    fn reaches(&self, start: NodeId, target: NodeId) -> bool {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            if current == target {
                return true;
            }
            if seen.insert(current) {
                stack.extend(self.outgoing(current).map(|e| e.to));
            }
        }
        false
    }

    /// Processing order, topological: each node comes after every node that affects it (through any connection,
    /// sidechain included). Deterministic: among ready nodes the smaller id goes first. Unconnected nodes are included;
    /// nodes on a cycle (which `check_edge` rejects) are left out.
    pub fn order(&self) -> Vec<NodeId> {
        let mut waiting: BTreeMap<NodeId, usize> = self.nodes.iter().map(|n| (n.id, 0)).collect();
        for edge in &self.edges {
            if waiting.contains_key(&edge.from)
                && let Some(count) = waiting.get_mut(&edge.to)
            {
                *count += 1;
            }
        }
        let mut ready: BTreeSet<NodeId> = waiting.iter().filter(|(_, c)| **c == 0).map(|(id, _)| *id).collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = ready.pop_first() {
            order.push(id);
            for edge in self.outgoing(id) {
                if let Some(count) = waiting.get_mut(&edge.to) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(edge.to);
                    }
                }
            }
        }
        order
    }

    /// The chain from `start`: follow the single regular post-fader connection while it leads into an effect; the first
    /// non-effect target is the output. Branching or no connection gives `output: None`. Pre-fader sends (like
    /// sidechains) are branches, not the track's path, so they are not part of the chain.
    pub fn chain(&self, start: NodeId) -> TrackChain {
        let mut chain = TrackChain::default();
        let mut current = start;
        for _ in 0..self.nodes.len() {
            let mut next = self.outgoing(current).filter(|e| !e.sidechain && !e.pre_fader);
            let (Some(edge), None) = (next.next(), next.next()) else {
                return chain;
            };
            match self.node(edge.to) {
                Some(node) if matches!(node.kind, NodeKind::Effect { .. }) => {
                    chain.effects.push(node.id);
                    current = node.id;
                }
                Some(node) => {
                    chain.output = Some(node.id);
                    return chain;
                }
                None => return chain,
            }
        }
        chain
    }

    /// The node's signal path as the mixer sees it (`app/src/mixer/mixerStrips.ts`): from the node along the FIRST
    /// regular post-fader connection (other connections are sends); an effect with no other regular inputs is an insert
    /// and the path continues; the first non-insert (bus, master, shared effect) is the output. Unlike `chain`, a send
    /// does not break the path. With no such connection, `output: None`.
    pub fn insert_path(&self, start: NodeId) -> TrackChain {
        let mut path = TrackChain::default();
        let mut current = start;
        for _ in 0..self.nodes.len() {
            let Some(edge) = self.outgoing(current).find(|e| !e.sidechain && !e.pre_fader) else {
                return path;
            };
            let is_effect = self.node(edge.to).is_some_and(|n| matches!(n.kind, NodeKind::Effect { .. }));
            if is_effect && self.incoming(edge.to).filter(|e| !e.sidechain).count() == 1 {
                path.effects.push(edge.to);
                current = edge.to;
            } else {
                path.output = Some(edge.to);
                return path;
            }
        }
        path
    }

    /// The track's source node.
    pub fn source_of(&self, track: TrackId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.kind.track() == Some(track))
    }

    /// Nodes whose sound reaches the master through regular (not sidechain) edges, the master included.
    pub fn reaching_master(&self) -> BTreeSet<NodeId> {
        let mut found = BTreeSet::from([MASTER]);
        let mut stack = vec![MASTER];
        while let Some(current) = stack.pop() {
            for edge in self.incoming(current).filter(|e| !e.sidechain) {
                if found.insert(edge.from) {
                    stack.push(edge.from);
                }
            }
        }
        found
    }

    /// All nodes whose sound reaches `id` (excluding `id` itself), e.g. for stems.
    pub fn upstream(&self, id: NodeId) -> BTreeSet<NodeId> {
        let mut found = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            for edge in self.incoming(current) {
                if found.insert(edge.from) {
                    stack.push(edge.from);
                }
            }
        }
        found
    }
}

#[cfg(test)]
mod tests;
