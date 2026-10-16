//! `ProjectTempo` (`TempoControl`): the song's tempo and time signature — the project tempo map. The map is checked by
//! the contract (`TempoMap::new`); here it is only edited through the core: an undo step and a snapshot for the engine
//! (the clock and export take the map from it).

use std::sync::Arc;

use daw_contracts::{Bpm, ContractError, SignatureChange, TempoChange, TempoControl, TempoMap, TimeSignature, tr};

use crate::ProjectService;

pub struct ProjectTempo {
    project: Arc<ProjectService>,
}

impl ProjectTempo {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }

    /// Replaces the map with the one `change` builds from the current one; on error the project stays as it was.
    fn edit(&self, label: &str, change: impl FnOnce(&TempoMap) -> Result<TempoMap, ContractError>) -> Result<(), ContractError> {
        self.project.edit(label, |project| {
            project.tempo_map = change(&project.tempo_map)?;
            Ok(())
        })
    }
}

impl TempoControl for ProjectTempo {
    fn tempo_map(&self) -> TempoMap {
        self.project.read(|project| project.tempo_map.clone())
    }

    fn set_tempo_changes(&self, changes: Vec<TempoChange>) -> Result<(), ContractError> {
        self.edit(&tr!("Change tempo", "Изменить темп"), |map| map.with_tempos(changes))
    }

    fn set_signature_changes(&self, changes: Vec<SignatureChange>) -> Result<(), ContractError> {
        self.edit(&tr!("Change time signature", "Изменить размер"), |map| map.with_signatures(changes))
    }

    fn set_tempo_map(&self, map: TempoMap) -> Result<(), ContractError> {
        self.edit(&tr!("Change tempo and time signature", "Изменить темп и размер"), |_| Ok(map))
    }

    fn set_tempo(&self, tempo: Bpm) -> Result<(), ContractError> {
        self.edit(&tr!("Change tempo", "Изменить темп"), |map| {
            let mut changes = map.tempos().to_vec();
            changes[0].bpm = tempo;
            map.with_tempos(changes)
        })
    }

    fn set_time_signature(&self, signature: TimeSignature) -> Result<(), ContractError> {
        self.edit(&tr!("Change time signature", "Изменить размер"), |map| {
            let mut changes = map.signatures().to_vec();
            changes[0].signature = signature;
            map.with_signatures(changes)
        })
    }
}
