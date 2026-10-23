//! `ProjectPluginStates`: keeps the plugin state of an effect or an instrument in the project (`PluginStates`).
//! Writes it into the model through the core as one undo step; recreating the sounding instance with the new state is
//! the job of DSP sync (`service/sync.rs`: the state is part of the instrument and effect keys).

use std::sync::Arc;

use daw_contracts::codes::{graph, tracks};
use daw_contracts::{ContractError, EditorTarget, L10n, NodeKind, PluginState, PluginStates, Project};

use crate::ProjectService;
use crate::ops;

const LABEL: L10n = L10n { en: "Plugin state", ru: "Состояние плагина" };

pub struct ProjectPluginStates {
    project: Arc<ProjectService>,
}

impl ProjectPluginStates {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

/// The target's state slot in the model.
fn slot(project: &mut Project, target: EditorTarget) -> Result<&mut Option<Arc<PluginState>>, ContractError> {
    match target {
        EditorTarget::Effect { node } => match &mut ops::node_mut(project, node)?.kind {
            NodeKind::Effect { state, .. } => Ok(state),
            _ => Err(graph::NODE_NOT_EFFECT.with("id", node)),
        },
        EditorTarget::Instrument { track } => match &mut ops::track_mut(project, track)?.instrument {
            Some(spec) => Ok(&mut spec.state),
            None => Err(tracks::NO_INSTRUMENT.with("id", track)),
        },
    }
}

impl PluginStates for ProjectPluginStates {
    fn get(&self, target: EditorTarget) -> Result<Option<Arc<PluginState>>, ContractError> {
        self.project.read(|project| match target {
            EditorTarget::Effect { node } => match &ops::node(project, node)?.kind {
                NodeKind::Effect { state, .. } => Ok(state.clone()),
                _ => Err(graph::NODE_NOT_EFFECT.with("id", node)),
            },
            EditorTarget::Instrument { track } => ops::track(project, track)?
                .instrument
                .as_ref()
                .map(|spec| spec.state.clone())
                .ok_or_else(|| tracks::NO_INSTRUMENT.with("id", track)),
        })
    }

    fn set(&self, target: EditorTarget, state: PluginState) -> Result<(), ContractError> {
        let state = Arc::new(state);
        self.project.edit(LABEL.get(), |project| {
            *slot(project, target)? = Some(state);
            Ok(())
        })
    }
}
