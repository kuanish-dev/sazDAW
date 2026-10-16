//! The real-time graph mixer: processes the project graph for one segment.
//!
//! Sources (instruments, audio tracks) write into their node buffers; then the nodes go strictly in the order of
//! `EngineSnapshot.order` (computed by the control side; no sorting here): an effect processes its buffer in place
//! (with a sidechain key if one is fed), sources and buses run their channel utility first (polarity, width, mono, fine
//! time alignment: `utility.rs`), then sources, buses and the master apply volume/pan/mute/solo, after which the node
//! output is added with each connection's level to the receiver's buffer (or its sidechain buffer). A "pre-fader" send
//! takes the node sound before volume and pan: before the fader, a node with such sends copies its buffer into the
//! preallocated `pre_fader` (mute/solo apply to it). A connection level can be overridden by automation for a segment;
//! its change is smoothed over the segment. The result is the master buffer. The peaks of tracks, buses and the master
//! go to `Meters`, and so do the gain reduction and the band gains of effects that report them. Every node's input (before the effect
//! or the fader) and output are offered to the `NodeTap`, which copies only the watched nodes (the live pictures of the
//! effects). A connection bypassing a longer path goes through a delay line (plugin delay compensation, `latency.rs`).
//! A channel standing on the virtual stage (`crate::stage`) plays its volume at its fader but not its pan: its voice
//! places it in the room where its sound leaves its insert path (after its effects, so they shape the instrument's own
//! sound and a compressor cannot undo the distance), and the shared room tail joins the master before its fader.
//!
//! All tables are preallocated for `MAX_NODES` / `MAX_EDGES`; a new snapshot only refills them within the capacity
//! (extra nodes and connections are silently ignored), nothing grows in `process`.

mod compile;
mod delay;
mod effects;
mod latency;
mod utility;

use daw_contracts::{
    BlockTiming, Effect, MAX_EDGES, MAX_LEVEL, MAX_NODES, MAX_WIDTH, MonitorMode, NodeId, NodeKind, Polarity, StagePlace, StreamConfig,
    TrackId,
};

use crate::SEGMENT_FRAMES;
use crate::meters::Meters;
use crate::scope::NodeTap;
use crate::stage::{Spot, Stage};
use delay::DelayLines;
#[cfg(test)]
pub use delay::{MAX_DELAY_LINES, MAX_PDC_FRAMES};
use effects::EffectPool;
use utility::{ChannelUtilities, Utility};

#[derive(Clone, Copy)]
enum Role {
    Instrument { track: TrackId, mute: bool, solo: bool },
    Audio { track: TrackId, mute: bool, solo: bool, monitor: MonitorMode },
    Effect { bypass: bool },
    Bus { mute: bool, solo: bool },
    Master,
}

#[derive(Clone, Copy)]
struct EdgeRt {
    to: usize,
    level: f32,
    sidechain: bool,
    /// A "pre-fader" send (only a regular connection from a node with a fader: the snapshot lets nothing else in here).
    pre_fader: bool,
    /// The compensation delay line, if the connection sound must be delayed (`latency.rs`).
    line: Option<usize>,
    /// The level from automation for this segment.
    level_override: Option<f32>,
    /// The level the connection had at the end of the previous segment (`NaN` means it has not sounded yet): there is a
    /// smooth transition from it to the new level, so send automation does not step.
    applied: f32,
}

impl EdgeRt {
    fn new(to: usize, level: f32, sidechain: bool, pre_fader: bool) -> Self {
        Self { to, level, sidechain, pre_fader, line: None, level_override: None, applied: f32::NAN }
    }

    /// The `(from, to)` levels of the connection in this segment; remembers the end one.
    fn ramp(&mut self) -> (f32, f32) {
        let end = self.level_override.unwrap_or(self.level);
        let start = if self.applied.is_nan() { end } else { self.applied };
        self.applied = end;
        (start, end)
    }
}

/// Adds `source` to `target` with a level going smoothly from `start` to `end` over the segment.
fn mix_ramped(source: &[f32], target: &mut [f32], (start, end): (f32, f32)) {
    let step = (end - start) / (source.len() / 2).max(1) as f32;
    let mut level = start;
    for (output, input) in target.chunks_exact_mut(2).zip(source.chunks_exact(2)) {
        level += step;
        output[0] += input[0] * level;
        output[1] += input[1] * level;
    }
}

/// The current smoothed channel gains of a node.
#[derive(Clone, Copy, Default)]
struct StereoGain {
    left: f32,
    right: f32,
    /// The gain of the "pre-fader" tap: 1, or 0 under mute or another node's solo (the transition is smooth, like the fader).
    pre_fader: f32,
    /// The target values are already set (the node's first segment has no smooth ramp from zero).
    ready: bool,
}

/// A stage voice that runs after a node: the voice of the channel `owner` (a node index) in stage slot `slot`.
#[derive(Clone, Copy)]
struct StageRun {
    owner: usize,
    slot: usize,
    /// The channel's sound reaches the master, so it plays in the room tail too.
    feeds_room: bool,
}

struct NodeRt {
    id: NodeId,
    role: Role,
    volume: f32,
    pan: f32,
    /// The channel utility settings (sources and buses; neutral on other nodes, which never run it).
    polarity: Polarity,
    width: f32,
    mono: bool,
    delay_ms: f32,
    /// The channel utility slot (only sources and buses).
    utility_slot: Option<usize>,
    /// Where the node stands on the virtual stage (sources and buses); `None` is off the stage, plain pan.
    place: Option<StagePlace>,
    /// The stage voice to run after this node: it is where a placed channel's insert path ends.
    stage_after: Option<StageRun>,
    /// The DSP object position in the effect store (only for effects with an object).
    effect: Option<usize>,
    /// The meter slot (only for buses and the master).
    meter: Option<usize>,
    gain: StereoGain,
    /// Automation values for this segment.
    volume_override: Option<f32>,
    pan_override: Option<f32>,
    width_override: Option<f32>,
    depth_override: Option<f32>,
    height_override: Option<f32>,
}

pub struct GraphMixer {
    nodes: Vec<NodeRt>,
    /// `(node id, index)` by ascending id: nodes are found by binary search.
    lookup: Vec<(NodeId, usize)>,
    /// Outgoing connections of all nodes in a row; the connections of node `i` are `edges[starts[i]..starts[i + 1]]`.
    edges: Vec<EdgeRt>,
    starts: Vec<usize>,
    /// Node indices in processing order.
    order: Vec<usize>,
    /// Node buffers; the last one is a "void" for a project without a master.
    buffers: Vec<Vec<f32>>,
    sidechains: Vec<Vec<f32>>,
    has_sidechain: Vec<bool>,
    /// The current node's sound before the fader (nodes go one at a time, so one buffer is enough).
    pre_fader: Vec<f32>,
    effects: EffectPool,
    delay_lines: DelayLines,
    utilities: ChannelUtilities,
    master: usize,
    any_solo: bool,
    /// Whether a node is audible under solo: solo itself, everything flowing into it and everything flowing out of it.
    heard: Vec<bool>,
    scratch: compile::Scratch,
    stage: Stage,
}

fn balance(pan: f32) -> (f32, f32) {
    if pan >= 0.0 { (1.0 - pan, 1.0) } else { (1.0, 1.0 + pan) }
}

impl Role {
    /// `(mute, solo)` of nodes with a fader: sources and buses.
    fn strip(self) -> Option<(bool, bool)> {
        match self {
            Role::Instrument { mute, solo, .. } | Role::Audio { mute, solo, .. } | Role::Bus { mute, solo } => Some((mute, solo)),
            Role::Effect { .. } | Role::Master => None,
        }
    }

    fn of(kind: &NodeKind) -> Role {
        match kind {
            NodeKind::Instrument { track, strip } => Role::Instrument { track: *track, mute: strip.mute, solo: strip.solo },
            NodeKind::Audio { track, strip, monitor } => {
                Role::Audio { track: *track, mute: strip.mute, solo: strip.solo, monitor: *monitor }
            }
            NodeKind::Effect { bypass, .. } => Role::Effect { bypass: *bypass },
            NodeKind::Bus { strip } => Role::Bus { mute: strip.mute, solo: strip.solo },
            NodeKind::Master { .. } => Role::Master,
        }
    }
}

impl NodeRt {
    /// Where the node stands on the stage this segment (automation included), if it is placed.
    fn spot(&self) -> Option<Spot> {
        let place = self.place?;
        let place = StagePlace { depth: self.depth_override.unwrap_or(place.depth), height: self.height_override.unwrap_or(place.height) };
        Some(Spot::new(self.pan_override.unwrap_or(self.pan), place.clamped()))
    }

    /// What the channel utility does this segment (automation included).
    fn utility(&self) -> Utility {
        let side = if self.mono { 0.0 } else { self.width_override.unwrap_or(self.width) };
        Utility { polarity: self.polarity, side, delay_ms: self.delay_ms }
    }

    /// Volume and pan with a smooth transition over the segment; returns the peaks after the gain. A channel on the
    /// stage keeps its pan for its stage voice.
    fn apply_gain(&mut self, buffer: &mut [f32], frames: usize, silent: bool) -> (f32, f32) {
        let volume = self.volume_override.unwrap_or(self.volume);
        let pan = if self.place.is_some() { 0.0 } else { self.pan_override.unwrap_or(self.pan) };
        let (balance_left, balance_right) = balance(pan);
        let (target_left, target_right) = if silent { (0.0, 0.0) } else { (volume * balance_left, volume * balance_right) };
        if !self.gain.ready {
            self.gain = StereoGain { left: target_left, right: target_right, ready: true, ..self.gain };
        }
        let step_left = (target_left - self.gain.left) / frames as f32;
        let step_right = (target_right - self.gain.right) / frames as f32;
        let (mut gain_left, mut gain_right) = (self.gain.left, self.gain.right);
        let (mut peak_left, mut peak_right) = (0.0f32, 0.0f32);
        for frame in buffer.chunks_exact_mut(2) {
            gain_left += step_left;
            gain_right += step_right;
            frame[0] *= gain_left;
            frame[1] *= gain_right;
            peak_left = peak_left.max(frame[0].abs());
            peak_right = peak_right.max(frame[1].abs());
        }
        self.gain.left = target_left;
        self.gain.right = target_right;
        (peak_left, peak_right)
    }

    /// Copies the node sound before the fader into `out`: volume and pan do not apply, mute and another node's solo do.
    /// Called before `apply_gain` (which clears the "first segment" flag).
    fn tap_pre_fader(&mut self, buffer: &[f32], out: &mut [f32], silent: bool) {
        let target = if silent { 0.0 } else { 1.0 };
        let start = if self.gain.ready { self.gain.pre_fader } else { target };
        out.fill(0.0);
        mix_ramped(buffer, out, (start, target));
        self.gain.pre_fader = target;
    }
}

/// The node index from the sorted `(id, index)` table.
fn find_index(lookup: &[(NodeId, usize)], id: NodeId) -> Option<usize> {
    let found = lookup.binary_search_by_key(&id, |(node, _)| *node).ok()?;
    Some(lookup[found].1)
}

/// The readable buffer `from` and the writable buffer `to` (different indices).
fn pair(buffers: &mut [Vec<f32>], from: usize, to: usize) -> (&[f32], &mut [f32]) {
    if from < to {
        let (left, right) = buffers.split_at_mut(to);
        (&left[from], &mut right[0])
    } else {
        let (left, right) = buffers.split_at_mut(from);
        (&right[0], &mut left[to])
    }
}

impl GraphMixer {
    pub fn new() -> Self {
        Self {
            nodes: Vec::with_capacity(MAX_NODES),
            lookup: Vec::with_capacity(MAX_NODES),
            edges: Vec::with_capacity(MAX_EDGES),
            starts: Vec::with_capacity(MAX_NODES + 1),
            order: Vec::with_capacity(MAX_NODES),
            buffers: (0..=MAX_NODES).map(|_| vec![0.0; SEGMENT_FRAMES * 2]).collect(),
            sidechains: (0..MAX_NODES).map(|_| vec![0.0; SEGMENT_FRAMES * 2]).collect(),
            has_sidechain: vec![false; MAX_NODES],
            pre_fader: vec![0.0; SEGMENT_FRAMES * 2],
            effects: EffectPool::new(),
            delay_lines: DelayLines::new(),
            utilities: ChannelUtilities::new(StreamConfig::DEFAULT.sample_rate as f32),
            master: MAX_NODES,
            any_solo: false,
            heard: vec![true; MAX_NODES],
            scratch: compile::Scratch::new(),
            stage: Stage::new(StreamConfig::DEFAULT.sample_rate as f32),
        }
    }

    /// Runs outside the audio callback (the stream is being opened): the stage and the channel utilities allocate
    /// their lines for the rate here.
    pub fn prepare(&mut self, config: StreamConfig) {
        self.effects.prepare(config);
        self.stage.prepare(config.sample_rate as f32);
        self.utilities.prepare(config.sample_rate as f32);
    }

    fn index_of(&self, id: NodeId) -> Option<usize> {
        find_index(&self.lookup, id)
    }

    /// The track instrument node (an index in the node table).
    pub fn instrument_node(&self, track: TrackId) -> Option<usize> {
        self.nodes.iter().position(|n| matches!(n.role, Role::Instrument { track: t, .. } if t == track))
    }

    /// The node buffer (stereo, interleaved) for `frames` frames.
    pub fn buffer_mut(&mut self, index: usize, frames: usize) -> Option<&mut [f32]> {
        if index >= self.nodes.len() {
            return None;
        }
        Some(&mut self.buffers[index][..frames * 2])
    }

    /// Nodes in the table (indices `0..node_count`).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The track and monitoring mode, if node `index` is an audio track.
    pub fn audio_source(&self, index: usize) -> Option<(TrackId, MonitorMode)> {
        match self.nodes.get(index)?.role {
            Role::Audio { track, monitor, .. } => Some((track, monitor)),
            _ => None,
        }
    }

    /// The buffer of node `Audio { track }`.
    pub fn audio_buffer(&mut self, track: TrackId, frames: usize) -> Option<&mut [f32]> {
        let index = self.nodes.iter().position(|n| matches!(n.role, Role::Audio { track: t, .. } if t == track))?;
        self.buffer_mut(index, frames)
    }

    pub fn master(&self, frames: usize) -> &[f32] {
        &self.buffers[self.master][..frames * 2]
    }

    /// The master buffer for sound that goes past the graph (sample preview).
    pub fn master_mut(&mut self, frames: usize) -> &mut [f32] {
        &mut self.buffers[self.master][..frames * 2]
    }

    /// Sets or replaces a node's effect; the previous one (or one that did not fit) is returned to be freed outside the audio thread.
    pub fn add_effect(&mut self, node: NodeId, effect: Box<dyn Effect>) -> Option<Box<dyn Effect>> {
        self.effects.add(node, effect)
    }

    pub fn remove_effect(&mut self, node: NodeId) -> Option<Box<dyn Effect>> {
        self.effects.remove(node)
    }

    /// An effect param of node `node` (automation).
    pub fn set_effect_param(&mut self, node: NodeId, name: &str, value: f32) {
        if let Some(position) = self.index_of(node).and_then(|index| self.nodes[index].effect) {
            self.effects.set_param(position, name, value);
        }
    }

    /// The node volume for the segment from automation (effects have no volume); the snapshot value is left alone.
    pub fn override_volume(&mut self, node: NodeId, value: f32) {
        if let Some(index) = self.index_of(node)
            && !matches!(self.nodes[index].role, Role::Effect { .. })
        {
            self.nodes[index].volume_override = Some(value.clamp(0.0, MAX_LEVEL));
        }
    }

    /// The pan of a node with a fader (a source or a bus) for the segment.
    pub fn override_pan(&mut self, node: NodeId, value: f32) {
        if let Some(index) = self.index_of(node)
            && self.nodes[index].role.strip().is_some()
        {
            self.nodes[index].pan_override = Some(value.clamp(-1.0, 1.0));
        }
    }

    /// The stereo width of a node with a fader (a source or a bus) for the segment.
    pub fn override_width(&mut self, node: NodeId, value: f32) {
        if let Some(index) = self.index_of(node)
            && self.nodes[index].role.strip().is_some()
        {
            self.nodes[index].width_override = Some(value.clamp(0.0, MAX_WIDTH));
        }
    }

    /// How far from the listener a node on the stage stands, for the segment (automation).
    pub fn override_stage_depth(&mut self, node: NodeId, value: f32) {
        if let Some(index) = self.index_of(node) {
            self.nodes[index].depth_override = Some(value.clamp(0.0, 1.0));
        }
    }

    /// How high a node on the stage stands, for the segment (automation).
    pub fn override_stage_height(&mut self, node: NodeId, value: f32) {
        if let Some(index) = self.index_of(node) {
            self.nodes[index].height_override = Some(value.clamp(-1.0, 1.0));
        }
    }

    /// The level of connection `from → to` of this kind for the segment from automation; the snapshot value is left alone.
    pub fn override_edge_level(&mut self, from: NodeId, to: NodeId, sidechain: bool, value: f32) {
        let (Some(from), Some(to)) = (self.index_of(from), self.index_of(to)) else {
            return;
        };
        for edge in &mut self.edges[self.starts[from]..self.starts[from + 1]] {
            if edge.to == to && edge.sidechain == sidechain {
                edge.level_override = Some(value.clamp(0.0, MAX_LEVEL));
            }
        }
    }

    pub fn clear_overrides(&mut self) {
        for node in &mut self.nodes {
            node.volume_override = None;
            node.pan_override = None;
            node.width_override = None;
            node.depth_override = None;
            node.height_override = None;
        }
        for edge in &mut self.edges {
            edge.level_override = None;
        }
    }

    /// The segment time for effects (before `process`).
    pub fn set_timing(&mut self, timing: &BlockTiming) {
        self.effects.set_timing(timing);
    }

    /// Zeroes the buffers of all nodes for `frames` frames.
    pub fn clear(&mut self, frames: usize) {
        let len = frames * 2;
        for index in 0..self.nodes.len() {
            self.buffers[index][..len].fill(0.0);
            self.sidechains[index][..len].fill(0.0);
            self.has_sidechain[index] = false;
        }
        self.buffers[MAX_NODES][..len].fill(0.0);
        self.stage.clear(frames);
    }

    /// Goes through the nodes in snapshot order and mixes everything into the master.
    pub fn process(&mut self, frames: usize, meters: &Meters, tap: &NodeTap) {
        let len = frames * 2;
        for position in 0..self.order.len() {
            let index = self.order[position];
            let tapped = self.edges[self.starts[index]..self.starts[index + 1]].iter().any(|e| e.pre_fader);
            let id = self.nodes[index].id;
            if index == self.master {
                // Every channel on the stage that reaches the master has fed the room by now (processing order).
                self.stage.render_room(&mut self.buffers[index][..len]);
            }
            tap.input(id, &self.buffers[index][..len]);
            let node = &mut self.nodes[index];
            if let Role::Effect { bypass } = node.role {
                if !bypass && let Some(effect) = node.effect {
                    let key = self.has_sidechain[index].then(|| &self.sidechains[index][..len]);
                    if let Some(db) = self.effects.process(effect, &mut self.buffers[index][..len], key) {
                        meters.report_reduction(effect, node.id, db);
                    }
                    meters.report_bands(effect, node.id, self.effects.band_gains(effect));
                }
            } else {
                // The master is never muted; sources and buses are muted by their own mute and another node's solo.
                let silent = node.role.strip().is_some_and(|(mute, _)| mute || (self.any_solo && !self.heard[index]));
                if let Some(slot) = node.utility_slot {
                    self.utilities.process(slot, &mut self.buffers[index][..len], node.utility());
                }
                if tapped {
                    node.tap_pre_fader(&self.buffers[index][..len], &mut self.pre_fader[..len], silent);
                }
                let (left, right) = node.apply_gain(&mut self.buffers[index][..len], frames, silent);
                if let Some(meter) = node.meter {
                    meters.report_peak(meter, left, right);
                }
            }
            tap.output(id, &self.buffers[index][..len]);
            if let Some(run) = self.nodes[index].stage_after
                && let Some(spot) = self.nodes[run.owner].spot()
            {
                self.stage.process(run.slot, &mut self.buffers[index][..len], spot, run.feeds_room);
            }
            self.send_to_edges(index, len);
        }
    }

    /// Adds the node output (or its pre-fader sound) with each connection's level to the receiver's buffer or its sidechain input.
    fn send_to_edges(&mut self, index: usize, len: usize) {
        for edge in &mut self.edges[self.starts[index]..self.starts[index + 1]] {
            let levels = edge.ramp();
            let (source, target) = if edge.sidechain {
                self.has_sidechain[edge.to] = true;
                (&self.buffers[index][..len], &mut self.sidechains[edge.to][..len])
            } else if edge.pre_fader {
                (&self.pre_fader[..len], &mut self.buffers[edge.to][..len])
            } else {
                let (source, target) = pair(&mut self.buffers, index, edge.to);
                (&source[..len], &mut target[..len])
            };
            match edge.line {
                Some(line) => self.delay_lines.run(line, source, target, levels),
                None => mix_ramped(source, target, levels),
            }
        }
    }
}

#[cfg(test)]
mod tests;
