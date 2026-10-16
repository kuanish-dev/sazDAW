//! Control of the sound processing graph: nodes, edges, faders and effect parameters (`GraphControl`).

use crate::{ContractError, Graph, NodeId, Params, Polarity};

/// The sound processing graph. Job: shape the signal flow: nodes (effects, buses) and the edges between them under the
/// graph rules (`Graph::check_edge`: a source only sends sound, the master only receives, no loops), plus the channel
/// controls (volume, pan, mute, solo, polarity, width, mono, fine time alignment) and effect parameters.
/// Sources (tracks) and the master are created with their owners and are not removed from here.
/// Sources and buses have every channel control (the master only volume, an effect none of these).
///
/// Rules for all methods: an unknown node is `graph::NODE_NOT_FOUND`; a value that is not a number (volume, level,
/// position, parameter, width, delay) is `ErrorKind::Invalid`; at most `MAX_NODES` nodes and `MAX_EDGES` edges
/// (`ErrorKind::Limit`). Each changing method is one undo step; a knob gesture (volume, pan, width, delay, edge level,
/// effect parameter, node position) merges into one step.
///
/// A new effect (`add_effect`, `insert_effect`, `insert_effect_before`) takes `params` on top of its kind's catalog
/// defaults, each clamped to its range as in `set_effect_param`: an unknown name is `graph::UNKNOWN_EFFECT_PARAM`, a
/// value that is not a number `graph::PARAM_NOT_A_NUMBER`, and on such an error nothing is created. The effect and its
/// params are one undo step, so an agent builds a configured chain without a call per knob.
pub trait GraphControl: Send + Sync {
    fn graph(&self) -> Graph;
    /// A new effect without edges (the user connects it). An unknown kind is `graph::UNKNOWN_EFFECT`.
    fn add_effect(&self, effect: &str, params: &Params, x: f32, y: f32) -> Result<NodeId, ContractError>;
    /// A new bus with volume `DEFAULT_VOLUME`, already connected to the master.
    fn add_bus(&self, name: &str, x: f32, y: f32) -> Result<NodeId, ContractError>;
    /// Removes an effect or bus together with its edges and automation lanes; sources and the master cannot be
    /// removed (`ErrorKind::Invalid`).
    fn remove_node(&self, id: NodeId) -> Result<(), ContractError>;
    /// Label of any node. For a source only the node label changes and the track name stays (that is changed by
    /// `TrackControl::rename_track`, which also updates the label).
    fn rename_node(&self, id: NodeId, name: &str) -> Result<(), ContractError>;
    fn move_node(&self, id: NodeId, x: f32, y: f32) -> Result<(), ContractError>;
    /// Connects under the graph rules (a violation is `ErrorKind::Invalid` with the reason, e.g.
    /// `graph::EDGE_MAKES_LOOP`); the level is clamped to `0..=MAX_LEVEL`. Connecting the same pair with the same kind
    /// (normal/sidechain) again only changes the level.
    fn connect(&self, from: NodeId, to: NodeId, level: f32, sidechain: bool) -> Result<(), ContractError>;
    /// Removes the edge of this pair and kind together with its automation lane; no such edge is `ErrorKind::Invalid`.
    fn disconnect(&self, from: NodeId, to: NodeId, sidechain: bool) -> Result<(), ContractError>;
    /// Makes the normal edge `from → to` a pre-fader (`true`) or post-fader (`false`) send (see `Edge::pre_fader`).
    /// No such normal edge is `ErrorKind::Invalid`; `from` that is neither a source nor a bus (an effect, the master)
    /// is `graph::PRE_FADER_NEEDS_STRIP`; an unknown node is `graph::NODE_NOT_FOUND`. The edge's level and automation
    /// lane stay. One undo step.
    fn set_edge_pre_fader(&self, from: NodeId, to: NodeId, pre_fader: bool) -> Result<(), ContractError>;
    /// Picks the sidechain (key) source of effect `effect`: removes every sidechain edge into it (with their automation
    /// lanes), then with `Some(source)` connects `source → effect` as a sidechain at level 1 and switches the effect's
    /// `sidechain` parameter on; with `None` switches it off. An effect reads the key only when its catalog has a
    /// `sidechain` parameter, so another effect kind is `graph::NO_SIDECHAIN_INPUT`; a node that is not an effect is
    /// `graph::NODE_NOT_EFFECT`; a source the graph rules reject is their error (e.g. `graph::EDGE_MAKES_LOOP`).
    /// One undo step; on error the project is unchanged.
    fn set_sidechain(&self, effect: NodeId, source: Option<NodeId>) -> Result<(), ContractError>;
    /// Volume of a source, bus or the master, clamped to `0..=MAX_LEVEL`; on an effect `ErrorKind::Invalid`.
    fn set_volume(&self, id: NodeId, volume: f32) -> Result<(), ContractError>;
    /// Pan of a source or bus, clamped to `-1..=1`; on the master or an effect `ErrorKind::Invalid`.
    fn set_pan(&self, id: NodeId, pan: f32) -> Result<(), ContractError>;
    /// Mute of a source or bus; on the master or an effect `ErrorKind::Invalid`.
    fn set_mute(&self, id: NodeId, mute: bool) -> Result<(), ContractError>;
    /// Solo of a source or bus; on the master or an effect `ErrorKind::Invalid`.
    fn set_solo(&self, id: NodeId, solo: bool) -> Result<(), ContractError>;
    /// Polarity invert of a source or bus, per side (`Strip::polarity`); on the master or an effect
    /// `graph::NO_UTILITY`.
    fn set_polarity(&self, id: NodeId, polarity: Polarity) -> Result<(), ContractError>;
    /// Stereo width of a source or bus (`Strip::width`), clamped to `0..=MAX_WIDTH`; not a number is
    /// `graph::WIDTH_NOT_A_NUMBER`, the master or an effect `graph::NO_UTILITY`.
    fn set_width(&self, id: NodeId, width: f32) -> Result<(), ContractError>;
    /// Mono switch of a source or bus (`Strip::mono`); the width stays for when it goes off. On the master or an
    /// effect `graph::NO_UTILITY`.
    fn set_mono(&self, id: NodeId, mono: bool) -> Result<(), ContractError>;
    /// Fine time alignment of a source or bus in milliseconds (`Strip::delay_ms`: positive later, negative earlier),
    /// clamped to `-MAX_CHANNEL_DELAY_MS..=MAX_CHANNEL_DELAY_MS`; not a number is `graph::DELAY_NOT_A_NUMBER`, the
    /// master or an effect `graph::NO_UTILITY`.
    fn set_delay(&self, id: NodeId, ms: f32) -> Result<(), ContractError>;
    /// An effect parameter, clamped to its catalog range; a node that is not an effect or an unknown parameter is
    /// `ErrorKind::Invalid`.
    fn set_effect_param(&self, id: NodeId, name: &str, value: f32) -> Result<(), ContractError>;
    /// Effect bypass; a node that is not an effect is `ErrorKind::Invalid`.
    fn set_effect_bypass(&self, id: NodeId, bypass: bool) -> Result<(), ContractError>;
    /// Inserts an effect right after node `after` (a source, effect or bus): the node's normal post-fader edges to its
    /// receivers move to the effect with the same levels and automation lanes (sidechain and pre-fader sends stay on
    /// the node), and the node itself connects to the effect (level 1).
    /// This is the "+ Effect" button on a track and a bus. The effect joins the track chain (`Graph::chain`).
    /// The master is `graph::MASTER_HAS_NO_OUTPUT`; an unknown kind is `graph::UNKNOWN_EFFECT`. One undo step.
    fn insert_effect(&self, after: NodeId, effect: &str, params: &Params, x: f32, y: f32) -> Result<NodeId, ContractError>;
    /// Inserts an effect right before node `node` (the master, a bus or an effect): all the node's normal inputs move
    /// to the effect with the same levels, send kind and automation lanes (sidechain inputs stay on the node), and the
    /// effect connects to the node (level 1). This is how a limiter goes on the master and a compressor on a bus.
    /// A source is `graph::SOURCE_HAS_NO_INPUT`; an unknown kind is `graph::UNKNOWN_EFFECT`; node and edge limits are
    /// `ErrorKind::Limit`. One undo step; on error the project is unchanged.
    fn insert_effect_before(&self, node: NodeId, effect: &str, params: &Params, x: f32, y: f32) -> Result<NodeId, ContractError>;
    /// Removes an effect and connects its inputs to its outputs (the sound path is not broken; a new edge takes the
    /// input's send kind, the effect edges' automation lanes are removed); a node that is not an effect is
    /// `ErrorKind::Invalid`.
    fn remove_effect_keep_path(&self, id: NodeId) -> Result<(), ContractError>;
    /// Changes the node's "Output": its single normal post-fader edge now leads to `target` (a bus, the master, an
    /// effect); sidechain and pre-fader sends stay (a send to `target` itself is replaced by the output), and the
    /// automation lanes of removed edges are removed.
    fn set_output(&self, node: NodeId, target: NodeId) -> Result<(), ContractError>;
}
