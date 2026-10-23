//! `ProjectNotes`: the notes of a clip — notes inside a note clip and the length of its content.
//! Single operations are a special case of batch ones, so the rules are written once; content rules live in
//! `note_rules.rs`, musical edits of a group of notes (quantize, legato, groove, variation…) in `note_transforms.rs`.

use std::sync::Arc;

use daw_contracts::codes::notes;
use daw_contracts::{
    Clip, ClipContent, ClipId, ContractError, ControllerKind, ControllerPoint, MAX_NOTES_PER_CLIP, NewNote, Note, NoteContent, NoteControl,
    NoteId, NoteTransform, Project, Variation, tr,
};

use crate::ProjectService;
use crate::clip_rules::copy_after;
use crate::groove::extract_groove;
use crate::note_rules::{check_loop_ticks, create_notes, fit_length, note_limit, replace_notes, set_controller_lane, set_loop_ticks};
use crate::note_transforms::{Surroundings, transform_notes};
use crate::ops;

fn content(project: &Project, clip: ClipId) -> Result<&NoteContent, ContractError> {
    match &ops::clip(project, clip)?.content {
        ClipContent::Notes(content) => Ok(content),
        ClipContent::Audio { .. } => Err(notes::AUDIO_CLIP_HAS_NO_NOTES.with("id", clip)),
    }
}

fn content_mut(project: &mut Project, clip: ClipId) -> Result<&mut NoteContent, ContractError> {
    match &mut ops::clip_mut(project, clip)?.content {
        ClipContent::Notes(content) => Ok(content),
        ClipContent::Audio { .. } => Err(notes::AUDIO_CLIP_HAS_NO_NOTES.with("id", clip)),
    }
}

fn single<T>(mut items: Vec<T>) -> Result<T, ContractError> {
    items.pop().ok_or_else(|| notes::NO_RESULT.err())
}

pub struct ProjectNotes {
    project: Arc<ProjectService>,
}

impl ProjectNotes {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }

    /// An edit of the clip content as one undo step; afterwards the length is fitted to the notes.
    fn edit<R>(&self, label: &str, clip: ClipId, f: impl FnOnce(&mut NoteContent) -> Result<R, ContractError>) -> Result<R, ContractError> {
        self.project.edit(label, |project| {
            let bar = project.tempo_map.bar_length_at(ops::clip(project, clip)?.start_tick);
            let content = content_mut(project, clip)?;
            let result = f(content)?;
            fit_length(content, bar);
            Ok(result)
        })
    }
}

impl NoteControl for ProjectNotes {
    fn notes(&self, clip: ClipId) -> Result<Vec<Note>, ContractError> {
        self.project.read(|p| Ok(content(p, clip)?.notes.clone()))
    }

    fn add_note(&self, clip: ClipId, note: NewNote) -> Result<Note, ContractError> {
        single(self.edit(&tr!("Add note", "Добавить ноту"), clip, |content| add(content, vec![note]))?)
    }

    fn update_note(&self, clip: ClipId, id: NoteId, note: NewNote) -> Result<Note, ContractError> {
        single(self.edit(&tr!("Edit note", "Изменить ноту"), clip, |content| update(content, vec![(id, note)]))?)
    }

    fn remove_note(&self, clip: ClipId, id: NoteId) -> Result<(), ContractError> {
        self.edit(&tr!("Delete note", "Удалить ноту"), clip, |content| remove(content, &[id]))
    }

    fn clear_notes(&self, clip: ClipId) -> Result<(), ContractError> {
        self.edit(&tr!("Clear clip", "Очистить клип"), clip, |content| {
            content.notes.clear();
            Ok(())
        })
    }

    fn add_notes(&self, clip: ClipId, notes: Vec<NewNote>) -> Result<Vec<Note>, ContractError> {
        self.edit(&tr!("Add notes", "Добавить ноты"), clip, |content| add(content, notes))
    }

    fn update_notes(&self, clip: ClipId, changes: Vec<(NoteId, NewNote)>) -> Result<Vec<Note>, ContractError> {
        self.edit(&tr!("Edit notes", "Изменить ноты"), clip, |content| update(content, changes))
    }

    fn remove_notes(&self, clip: ClipId, ids: Vec<NoteId>) -> Result<(), ContractError> {
        self.edit(&tr!("Delete notes", "Удалить ноты"), clip, |content| remove(content, &ids))
    }

    fn replace_notes(&self, clip: ClipId, notes: Vec<NewNote>) -> Result<usize, ContractError> {
        self.edit(&tr!("Replace notes", "Заменить ноты"), clip, |content| replace_notes(content, notes))
    }

    fn set_loop_ticks(&self, clip: ClipId, loop_ticks: u32) -> Result<(), ContractError> {
        check_loop_ticks(loop_ticks)?;
        self.project.edit(&tr!("Clip content length", "Длина содержимого клипа"), |project| {
            set_loop_ticks(content_mut(project, clip)?, loop_ticks);
            ops::clip_mut(project, clip)?.offset_ticks %= loop_ticks;
            Ok(())
        })
    }

    fn set_controller_lane(&self, clip: ClipId, kind: ControllerKind, points: Vec<ControllerPoint>) -> Result<(), ContractError> {
        self.edit(&tr!("Controller lane", "Линия контроллера"), clip, |content| set_controller_lane(content, kind, points))
    }

    fn transform_notes(&self, clip: ClipId, ids: Option<Vec<NoteId>>, transform: NoteTransform) -> Result<Vec<Note>, ContractError> {
        let (label, transform) = match transform {
            NoteTransform::Quantize { .. } => (tr!("Quantize notes", "Квантовать ноты"), transform),
            NoteTransform::Humanize { timing_ticks, velocity, seed } => {
                let seed = Some(seed.unwrap_or_else(random_seed));
                (tr!("Humanize notes", "Очеловечить ноты"), NoteTransform::Humanize { timing_ticks, velocity, seed })
            }
            NoteTransform::Transpose { .. } => (tr!("Transpose notes", "Транспонировать ноты"), transform),
            NoteTransform::FitToScale { .. } => (tr!("Fit notes to scale", "Ноты в лад"), transform),
            NoteTransform::Legato => (tr!("Legato", "Легато"), transform),
            NoteTransform::Groove { .. } => (tr!("Groove", "Грув"), transform),
            NoteTransform::GrooveFromClip { .. } => (tr!("Groove from clip", "Грув из клипа"), transform),
            NoteTransform::Vary(variation) => (tr!("Note variation", "Вариация нот"), NoteTransform::Vary(with_seed(variation))),
        };
        self.project.edit(&label, |project| transform_in(project, clip, ids.as_deref(), transform))
    }

    fn vary_to_new_clip(&self, clip: ClipId, ids: Option<Vec<NoteId>>, variation: Variation) -> Result<Clip, ContractError> {
        let transform = NoteTransform::Vary(with_seed(variation));
        self.project.edit(&tr!("Variation to new clip", "Вариация в новый клип"), |project| {
            let copy = copy_after(project, clip)?;
            transform_in(project, copy.id, ids.as_deref(), transform)?;
            ops::clip(project, copy.id).cloned()
        })
    }
}

/// Without a seed each call gets its own: varying again must not repeat the same pattern.
fn with_seed(variation: Variation) -> Variation {
    Variation { seed: Some(variation.seed.unwrap_or_else(random_seed)), ..variation }
}

/// A musical edit of the clip's notes inside an open project edit; afterwards the length is fitted to the notes.
fn transform_in(project: &mut Project, clip: ClipId, ids: Option<&[NoteId]>, transform: NoteTransform) -> Result<Vec<Note>, ContractError> {
    let bar = project.tempo_map.bar_length_at(ops::clip(project, clip)?.start_tick);
    let groove = match transform {
        NoteTransform::GrooveFromClip { source, .. } => {
            let source_bar = project.tempo_map.bar_length_at(ops::clip(project, source)?.start_tick);
            Some(extract_groove(content(project, source)?, source_bar).ok_or_else(|| notes::GROOVE_SOURCE_EMPTY.with("id", source))?)
        }
        _ => None,
    };
    let content = content_mut(project, clip)?;
    let result = transform_notes(content, ids, transform, &Surroundings { bar, groove })?;
    fit_length(content, bar);
    Ok(result)
}

/// A seed from the clock: random enough, no cryptographic strength needed.
fn random_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() ^ d.as_secs() as u32)
}

fn add(content: &mut NoteContent, notes: Vec<NewNote>) -> Result<Vec<Note>, ContractError> {
    if content.notes.len() + notes.len() > MAX_NOTES_PER_CLIP {
        return Err(notes::TOO_MANY_NOTES.with("max", MAX_NOTES_PER_CLIP));
    }
    let created = create_notes(content, notes)?;
    content.notes.extend(created.iter().copied());
    Ok(created)
}

fn update(content: &mut NoteContent, changes: Vec<(NoteId, NewNote)>) -> Result<Vec<Note>, ContractError> {
    let limit = note_limit(content);
    changes
        .into_iter()
        .map(|(id, note)| {
            note.validate(limit)?;
            let slot = content.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| notes::NOT_FOUND.with("id", id))?;
            *slot = Note::from_new(id, note);
            Ok(*slot)
        })
        .collect()
}

fn remove(content: &mut NoteContent, ids: &[NoteId]) -> Result<(), ContractError> {
    if let Some(missing) = ids.iter().find(|id| !content.notes.iter().any(|n| n.id == **id)) {
        return Err(notes::NOT_FOUND.with("id", missing));
    }
    content.notes.retain(|n| !ids.contains(&n.id));
    Ok(())
}
