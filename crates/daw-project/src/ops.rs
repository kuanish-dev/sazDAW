//! Shared lookups and small model edits with coded errors; the project's entities share them.

use daw_contracts::codes::{clips, graph, samples, tracks};
use daw_contracts::{
    Clip, ClipContent, ClipId, ContractError, Edge, ErrorCode, KindInfo, MASTER, MAX_EDGES, MAX_NODES, Node, NodeId, NodeKind, ParamInfo,
    ParamTarget, Params, Project, SampleId, StagePlace, Track, TrackId,
};

pub fn track(project: &Project, id: TrackId) -> Result<&Track, ContractError> {
    project.track(id).ok_or_else(|| tracks::NOT_FOUND.with("id", id))
}

pub fn track_mut(project: &mut Project, id: TrackId) -> Result<&mut Track, ContractError> {
    project.tracks.iter_mut().find(|t| t.id == id).ok_or_else(|| tracks::NOT_FOUND.with("id", id))
}

/// The kind of the track's instrument; an audio track has none (`TR-160`).
pub fn instrument_kind(project: &Project, id: TrackId) -> Result<String, ContractError> {
    track(project, id)?.instrument.as_ref().map(|i| i.kind.clone()).ok_or_else(|| tracks::NO_INSTRUMENT.with("id", id))
}

/// The catalog description of parameter `name` of kind `kind`; an unknown parameter is `missing` (with `kind` and
/// `param`), since instruments, effects and MIDI effects each have their own code.
pub fn param_info(kinds: Vec<KindInfo>, kind: &str, name: &str, missing: ErrorCode) -> Result<ParamInfo, ContractError> {
    kinds
        .into_iter()
        .find(|k| k.kind == kind)
        .and_then(|k| k.params.into_iter().find(|p| p.name == name))
        .ok_or_else(|| missing.with("kind", kind).with("param", name))
}

pub fn clip(project: &Project, id: ClipId) -> Result<&Clip, ContractError> {
    project.clip(id).ok_or_else(|| clips::NOT_FOUND.with("id", id))
}

pub fn clip_mut(project: &mut Project, id: ClipId) -> Result<&mut Clip, ContractError> {
    project.clips.iter_mut().find(|c| c.id == id).ok_or_else(|| clips::NOT_FOUND.with("id", id))
}

pub fn check_sample(project: &Project, id: SampleId) -> Result<(), ContractError> {
    if project.samples.iter().any(|s| s.id == id) { Ok(()) } else { Err(samples::NOT_FOUND.with("id", id)) }
}

pub fn node(project: &Project, id: NodeId) -> Result<&Node, ContractError> {
    project.graph.node(id).ok_or_else(|| graph::NODE_NOT_FOUND.with("id", id))
}

pub fn node_mut(project: &mut Project, id: NodeId) -> Result<&mut Node, ContractError> {
    project.graph.node_mut(id).ok_or_else(|| graph::NODE_NOT_FOUND.with("id", id))
}

/// Bypass flag and params of an effect node; any other node is `GR-160`.
pub fn effect_mut(project: &mut Project, id: NodeId) -> Result<(&mut bool, &mut Params), ContractError> {
    match &mut node_mut(project, id)?.kind {
        NodeKind::Effect { bypass, params, .. } => Ok((bypass, params)),
        _ => Err(graph::NODE_NOT_EFFECT.with("id", id)),
    }
}

/// The track's source node (the track fader lives in it).
pub fn source_node(project: &Project, track: TrackId) -> Option<NodeId> {
    project.graph.source_of(track).map(|n| n.id)
}

/// Whether the track suits the clip content: note clips go on instrument tracks, audio clips on audio tracks.
pub fn check_clip_fits_track(track: &Track, content: &ClipContent) -> Result<(), ContractError> {
    match (content, track.is_audio()) {
        (ClipContent::Notes(_), true) => Err(clips::NOTE_CLIP_NEEDS_INSTRUMENT_TRACK.err()),
        (ClipContent::Audio { .. }, false) => Err(clips::AUDIO_CLIP_NEEDS_AUDIO_TRACK.err()),
        _ => Ok(()),
    }
}

/// A new node already connected to the master (a track source, a bus).
pub fn add_node_to_master(project: &mut Project, name: &str, kind: NodeKind, x: f32, y: f32) -> Result<NodeId, ContractError> {
    limit(project.graph.nodes.len(), MAX_NODES, graph::TOO_MANY_NODES)?;
    limit(project.graph.edges.len(), MAX_EDGES, graph::TOO_MANY_EDGES)?;
    let id = project.alloc_id();
    project.graph.nodes.push(Node { id, name: name.to_string(), kind, x, y });
    project.graph.edges.push(Edge { from: id, to: MASTER, level: 1.0, sidechain: false, pre_fader: false });
    Ok(id)
}

/// Removes the node together with its edges and the automation lanes that target it.
pub fn drop_node(project: &mut Project, id: NodeId) {
    project.graph.nodes.retain(|n| n.id != id);
    project.graph.edges.retain(|e| e.from != id && e.to != id);
    project.automation.retain(|lane| match &lane.target {
        ParamTarget::NodeVolume { node }
        | ParamTarget::NodePan { node }
        | ParamTarget::NodeWidth { node }
        | ParamTarget::StageDepth { node }
        | ParamTarget::StageHeight { node }
        | ParamTarget::EffectParam { node, .. } => *node != id,
        ParamTarget::InstrumentParam { .. } | ParamTarget::EdgeLevel { .. } => true,
    });
    drop_stale_edge_lanes(project);
}

/// Drops the stage depth and height lanes of a channel that leaves the stage: they live as long as it stands there.
pub fn drop_stage_lanes(project: &mut Project, id: NodeId) {
    project
        .automation
        .retain(|lane| !matches!(lane.target, ParamTarget::StageDepth { node } | ParamTarget::StageHeight { node } if node == id));
}

/// Where a channel on the stage stands: its pan and place; a node off the stage is `graph::NOT_ON_STAGE`, one that
/// cannot stand there `graph::NO_STAGE_PLACE`.
pub fn stage_spot(project: &Project, id: NodeId) -> Result<(f32, StagePlace), ContractError> {
    let strip = node(project, id)?.kind.strip().ok_or_else(|| graph::NO_STAGE_PLACE.with("id", id))?;
    let place = strip.stage.ok_or_else(|| graph::NOT_ON_STAGE.with("id", id))?;
    Ok((strip.pan, place))
}

/// The edge an "edge level" target points at.
pub fn edge_of(project: &Project, from: NodeId, to: NodeId, sidechain: bool) -> Option<&Edge> {
    project.graph.edges.iter().find(|e| e.from == from && e.to == to && e.sidechain == sidechain)
}

/// Drops edge level lanes whose edge is gone. Called after every edit that removes edges, so the rule "a lane lives
/// as long as its edge" is kept in one place.
pub fn drop_stale_edge_lanes(project: &mut Project) {
    let graph = &project.graph;
    project.automation.retain(|lane| match lane.target {
        ParamTarget::EdgeLevel { from, to, sidechain } => {
            graph.edges.iter().any(|e| e.from == from && e.to == to && e.sidechain == sidechain)
        }
        _ => true,
    });
}

/// An edge level lane follows its edge (inserting an effect moves the edge to another node with the same level).
pub fn move_edge_lane(project: &mut Project, old: &Edge, new_from: NodeId, new_to: NodeId) {
    for lane in &mut project.automation {
        if lane.target == (ParamTarget::EdgeLevel { from: old.from, to: old.to, sidechain: old.sidechain }) {
            lane.target = ParamTarget::EdgeLevel { from: new_from, to: new_to, sidechain: old.sidechain };
        }
    }
}

/// Drops the automation lanes of the track's instrument parameters.
pub fn drop_instrument_lanes(project: &mut Project, id: TrackId) {
    project.automation.retain(|lane| !matches!(&lane.target, ParamTarget::InstrumentParam { track, .. } if *track == id));
}

/// The value if it is a finite number, otherwise `not_a_number` with the `value` param.
pub fn finite(value: f32, not_a_number: ErrorCode) -> Result<f32, ContractError> {
    if value.is_finite() { Ok(value) } else { Err(not_a_number.with("value", value)) }
}

/// Whether one more item fits next to `count` items under the limit `max`; otherwise `too_many` with the `max` param.
pub fn limit(count: usize, max: usize, too_many: ErrorCode) -> Result<(), ContractError> {
    if count >= max { Err(too_many.with("max", max)) } else { Ok(()) }
}

/// The current project value of the target's param; `None` for a target that is not in the project.
pub fn target_value(project: &Project, target: &ParamTarget) -> Option<f32> {
    let node = |id: NodeId| project.graph.node(id).map(|n| &n.kind);
    match target {
        ParamTarget::NodeVolume { node: id } => match node(*id)? {
            NodeKind::Master { volume } => Some(*volume),
            kind => kind.strip().map(|s| s.volume),
        },
        ParamTarget::NodePan { node: id } => node(*id)?.strip().map(|s| s.pan),
        ParamTarget::NodeWidth { node: id } => node(*id)?.strip().map(|s| s.width),
        ParamTarget::StageDepth { node: id } => node(*id)?.strip()?.stage.map(|place| place.depth),
        ParamTarget::StageHeight { node: id } => node(*id)?.strip()?.stage.map(|place| place.height),
        ParamTarget::EdgeLevel { from, to, sidechain } => edge_of(project, *from, *to, *sidechain).map(|e| e.level),
        ParamTarget::EffectParam { node: id, param } => match node(*id)? {
            NodeKind::Effect { params, .. } => params.get(param).copied(),
            _ => None,
        },
        ParamTarget::InstrumentParam { track, param } => project.track(*track)?.instrument.as_ref()?.params.get(param).copied(),
    }
}

/// The target exists in the project and has this parameter (shared by automation and MIDI controller mappings):
/// an unknown node or track is not found (`GR-001`, `TR-001`), the rest is invalid.
pub fn validate_target(project: &Project, target: &ParamTarget) -> Result<(), ContractError> {
    match target {
        ParamTarget::NodeVolume { node } => {
            if !self::node(project, *node)?.kind.has_volume() {
                return Err(graph::NO_VOLUME.with("id", node));
            }
        }
        ParamTarget::NodePan { node } => {
            if !self::node(project, *node)?.kind.has_strip() {
                return Err(graph::NO_PAN.with("id", node));
            }
        }
        ParamTarget::NodeWidth { node } => {
            if !self::node(project, *node)?.kind.has_strip() {
                return Err(graph::NO_UTILITY.with("id", node));
            }
        }
        ParamTarget::StageDepth { node } | ParamTarget::StageHeight { node } => {
            stage_spot(project, *node)?;
        }
        ParamTarget::EffectParam { node, param } => match &self::node(project, *node)?.kind {
            NodeKind::Effect { params, .. } if params.contains_key(param) => {}
            NodeKind::Effect { effect, .. } => return Err(graph::UNKNOWN_EFFECT_PARAM.with("kind", effect).with("param", param)),
            _ => return Err(graph::NODE_NOT_EFFECT.with("id", node)),
        },
        ParamTarget::EdgeLevel { from, to, sidechain } => {
            self::node(project, *from)?;
            self::node(project, *to)?;
            if edge_of(project, *from, *to, *sidechain).is_none() {
                return Err(graph::NO_SUCH_EDGE.with("from", from).with("to", to));
            }
        }
        ParamTarget::InstrumentParam { track, param } => {
            let instrument = self::track(project, *track)?.instrument.as_ref().ok_or_else(|| tracks::NO_INSTRUMENT.with("id", track))?;
            if !instrument.params.contains_key(param) {
                return Err(tracks::UNKNOWN_INSTRUMENT_PARAM.with("kind", &instrument.kind).with("param", param));
            }
        }
    }
    Ok(())
}
