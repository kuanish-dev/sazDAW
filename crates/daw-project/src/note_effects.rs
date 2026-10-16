//! `ProjectNoteEffects`: what happens to a track's notes before the instrument — the track's MIDI effect chain
//! (`Track::note_effects`): add, remove, reorder, params, bypass. The core creates the engine objects during sync
//! (`service/sync.rs`); params and bypass travel with the snapshot.

use std::sync::Arc;

use daw_contracts::codes::notes;
use daw_contracts::{
    ContractError, MAX_NOTE_EFFECTS, NoteEffectCatalog, NoteEffectControl, NoteEffectId, NoteEffectSpec, Params, Project, TrackId, tr,
};

use crate::{ProjectService, ops};

pub struct ProjectNoteEffects {
    project: Arc<ProjectService>,
    catalog: Arc<dyn NoteEffectCatalog>,
}

impl ProjectNoteEffects {
    pub fn new(project: Arc<ProjectService>, catalog: Arc<dyn NoteEffectCatalog>) -> Self {
        Self { project, catalog }
    }

    /// The value of parameter `name` of kind `kind`, clamped to the catalog range; an unknown name is `NT-121`.
    fn fit(&self, kind: &str, name: &str, value: f32) -> Result<f32, ContractError> {
        let info = ops::param_info(self.catalog.kinds(), kind, name, notes::UNKNOWN_NOTE_EFFECT_PARAM)?;
        Ok(info.fit(ops::finite(value, notes::NOTE_EFFECT_PARAM_NOT_A_NUMBER)?))
    }
}

/// The track's chain; an audio track has none.
fn chain(project: &Project, track: TrackId) -> Result<&[NoteEffectSpec], ContractError> {
    let track = ops::track(project, track)?;
    if track.is_audio() {
        return Err(audio_track(&track.name));
    }
    Ok(&track.note_effects)
}

fn chain_mut(project: &mut Project, track: TrackId) -> Result<&mut Vec<NoteEffectSpec>, ContractError> {
    let track = ops::track_mut(project, track)?;
    if track.is_audio() {
        return Err(audio_track(&track.name));
    }
    Ok(&mut track.note_effects)
}

fn audio_track(name: &str) -> ContractError {
    notes::NOTE_EFFECTS_NEED_INSTRUMENT.with("name", name)
}

fn position(chain: &[NoteEffectSpec], effect: NoteEffectId) -> Result<usize, ContractError> {
    chain.iter().position(|e| e.id == effect).ok_or_else(|| notes::NOTE_EFFECT_NOT_FOUND.with("id", effect))
}

fn effect_mut(project: &mut Project, track: TrackId, effect: NoteEffectId) -> Result<&mut NoteEffectSpec, ContractError> {
    let chain = chain_mut(project, track)?;
    let index = position(chain, effect)?;
    Ok(&mut chain[index])
}

impl NoteEffectControl for ProjectNoteEffects {
    fn add(&self, track: TrackId, kind: &str, overrides: Params) -> Result<NoteEffectId, ContractError> {
        let mut params = self.catalog.defaults(kind)?;
        for (name, value) in overrides {
            let value = self.fit(kind, &name, value)?;
            params.insert(name, value);
        }
        self.project.edit(&tr!("Add MIDI effect", "Добавить MIDI-эффект"), |project| {
            ops::limit(chain_mut(project, track)?.len(), MAX_NOTE_EFFECTS, notes::TOO_MANY_NOTE_EFFECTS)?;
            let id = project.alloc_id();
            chain_mut(project, track)?.push(NoteEffectSpec { id, kind: kind.to_string(), params, bypass: false });
            Ok(id)
        })
    }

    fn remove(&self, track: TrackId, effect: NoteEffectId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Remove MIDI effect", "Убрать MIDI-эффект"), |project| {
            let chain = chain_mut(project, track)?;
            let index = position(chain, effect)?;
            chain.remove(index);
            Ok(())
        })
    }

    fn move_to(&self, track: TrackId, effect: NoteEffectId, index: usize) -> Result<(), ContractError> {
        self.project.edit(&tr!("Move MIDI effect", "Переставить MIDI-эффект"), |project| {
            let chain = chain_mut(project, track)?;
            let spec = chain.remove(position(chain, effect)?);
            chain.insert(index.min(chain.len()), spec);
            Ok(())
        })
    }

    fn set_param(&self, track: TrackId, effect: NoteEffectId, name: &str, value: f32) -> Result<(), ContractError> {
        let kind = self.project.read(|project| {
            let chain = chain(project, track)?;
            Ok::<_, ContractError>(chain[position(chain, effect)?].kind.clone())
        })?;
        let value = self.fit(&kind, name, value)?;
        self.project.edit_keyed(
            &tr!("MIDI effect parameter", "Параметр MIDI-эффекта"),
            Some(format!("note-effect:{effect}:{name}")),
            |project| {
                effect_mut(project, track, effect)?.params.insert(name.to_string(), value);
                Ok(())
            },
        )
    }

    fn set_bypass(&self, track: TrackId, effect: NoteEffectId, bypass: bool) -> Result<(), ContractError> {
        self.project.edit(&tr!("Bypass MIDI effect", "Обход MIDI-эффекта"), |project| {
            effect_mut(project, track, effect)?.bypass = bypass;
            Ok(())
        })
    }
}
