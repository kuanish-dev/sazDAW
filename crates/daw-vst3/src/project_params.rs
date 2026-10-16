//! Target of a plugin window in the project: which plugin sits on the effect node or track, and its params there.
//! Reads and writes go only through the contracts (`GraphControl`, `TrackControl`): the project state is changed by its owner.

use std::sync::Arc;

use daw_contracts::codes::{graph, plugins, tracks};
use daw_contracts::{
    ContractError, EditorTarget, ErrorCode, GraphControl, InstrumentSpec, NodeId, NodeKind, Params, TrackControl, TrackId,
};

use crate::param_bridge::ProjectParams;

/// Plugin kind of the target (`vst3:…`), its params in the project and the failure for a built-in kind without a window.
pub struct Target {
    pub kind: String,
    pub params: Arc<dyn ProjectParams>,
    pub no_window: ErrorCode,
}

pub fn resolve(target: EditorTarget, graph: &Arc<dyn GraphControl>, tracks: &Arc<dyn TrackControl>) -> Result<Target, ContractError> {
    Ok(match target {
        EditorTarget::Effect { node } => {
            let (kind, _) = effect_of(&**graph, node)?;
            Target { kind, params: Arc::new(EffectParams { graph: graph.clone(), node }), no_window: plugins::BUILTIN_EFFECT_HAS_NO_WINDOW }
        }
        EditorTarget::Instrument { track } => {
            let kind = instrument_of(&**tracks, track)?.kind;
            Target {
                kind,
                params: Arc::new(InstrumentParams { tracks: tracks.clone(), track }),
                no_window: plugins::BUILTIN_INSTRUMENT_HAS_NO_WINDOW,
            }
        }
    })
}

/// Kind and params of the node's effect: `GR-001` for a missing node, `GR-160` for a node that is not an effect.
fn effect_of(graph: &dyn GraphControl, node: NodeId) -> Result<(String, Params), ContractError> {
    match graph.graph().node(node).map(|n| &n.kind) {
        Some(NodeKind::Effect { effect, params, .. }) => Ok((effect.clone(), params.clone())),
        Some(_) => Err(graph::NODE_NOT_EFFECT.with("id", node)),
        None => Err(graph::NODE_NOT_FOUND.with("id", node)),
    }
}

fn instrument_of(tracks: &dyn TrackControl, track: TrackId) -> Result<InstrumentSpec, ContractError> {
    let found = tracks.tracks().into_iter().find(|t| t.id == track).ok_or_else(|| tracks::NOT_FOUND.with("id", track))?;
    found.instrument.ok_or_else(|| tracks::NO_INSTRUMENT.with("id", track))
}

struct EffectParams {
    graph: Arc<dyn GraphControl>,
    node: NodeId,
}

impl ProjectParams for EffectParams {
    fn read(&self) -> Result<Params, ContractError> {
        effect_of(&*self.graph, self.node).map(|(_, params)| params)
    }

    fn write(&self, name: &str, value: f32) -> Result<(), ContractError> {
        self.graph.set_effect_param(self.node, name, value)
    }
}

struct InstrumentParams {
    tracks: Arc<dyn TrackControl>,
    track: TrackId,
}

impl ProjectParams for InstrumentParams {
    fn read(&self) -> Result<Params, ContractError> {
        instrument_of(&*self.tracks, self.track).map(|spec| spec.params)
    }

    fn write(&self, name: &str, value: f32) -> Result<(), ContractError> {
        self.tracks.set_instrument_param(self.track, name, value)
    }
}
