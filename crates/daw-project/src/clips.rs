//! `ProjectClips`: clips on the timeline — where clips sit on tracks, how long they are, and the loop region.
//! The rules (bounds, overlaps, copying) are pure functions in `clip_rules.rs`.

use std::sync::Arc;

use daw_contracts::codes::{clips, samples};
use daw_contracts::{
    Clip, ClipContent, ClipControl, ClipEdit, ClipId, ContractError, MAX_CLIP_TICK, MAX_CLIPS, NewNoteClip, NoteContent, Project, SampleId,
    SampleStore, TrackId, tr,
};

use crate::ProjectService;
use crate::clip_rules::{apply_edit, check_places, check_range, copy_after, copy_group, trim_start};
use crate::note_rules;
use crate::ops;
use crate::take_rules;
use crate::warp_rules::SoundFacts;

/// A new clip on a track: checks the track kind, the limit and the place (on the final state; an error rolls the edit back).
fn place(project: &mut Project, track: TrackId, start: u32, length: u32, content: ClipContent) -> Result<Clip, ContractError> {
    ops::limit(project.clips.len(), MAX_CLIPS, clips::TOO_MANY)?;
    ops::check_clip_fits_track(ops::track(project, track)?, &content)?;
    check_range(start, length)?;
    let clip = Clip {
        id: project.alloc_id(),
        track,
        start_tick: start,
        length_ticks: length,
        content,
        gain: 1.0,
        fade_in_ticks: 0,
        fade_out_ticks: 0,
        fade_in_shape: Default::default(),
        fade_out_shape: Default::default(),
        offset_ticks: 0,
        offset_frames: 0,
        warp: None,
        pitch_semitones: 0.0,
    };
    project.clips.push(clip.clone());
    check_places(project, &[clip.id])?;
    Ok(clip)
}

/// A note clip with its content (`ClipControl::add_note_clip`) inside a core edit.
fn place_note_clip(project: &mut Project, clip: NewNoteClip) -> Result<Clip, ContractError> {
    let NewNoteClip { track, start_tick, length_ticks, notes, loop_ticks } = clip;
    if let Some(loop_ticks) = loop_ticks {
        note_rules::check_loop_ticks(loop_ticks)?;
    }
    // The same order as "empty clip → replace notes → set length": notes are checked with the automatic length,
    // then a manual length drops and trims what does not fit.
    // The bar follows the time signature at the clip: both the content length and the default clip length (one bar).
    let bar = project.tempo_map.bar_length_at(start_tick);
    let mut content = NoteContent::empty();
    note_rules::replace_notes(&mut content, notes)?;
    note_rules::fit_length(&mut content, bar);
    if let Some(loop_ticks) = loop_ticks {
        note_rules::set_loop_ticks(&mut content, loop_ticks);
    }
    place(project, track, start_tick, length_ticks.unwrap_or(bar), ClipContent::Notes(content))
}

pub struct ProjectClips {
    project: Arc<ProjectService>,
    samples: Arc<dyn SampleStore>,
}

impl ProjectClips {
    pub fn new(project: Arc<ProjectService>, samples: Arc<dyn SampleStore>) -> Self {
        Self { project, samples }
    }

    /// Sounds for the tempo fitting rules (`warp_rules`).
    fn facts(&self) -> SoundFacts<'_> {
        SoundFacts { samples: &*self.samples }
    }
}

impl ClipControl for ProjectClips {
    fn clips(&self) -> Vec<Clip> {
        self.project.read(|p| p.clips.clone())
    }

    fn loop_region(&self) -> Option<(u32, u32)> {
        self.project.read(|p| p.loop_region)
    }

    fn add_note_clip(&self, clip: NewNoteClip) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("Add clip", "Добавить клип"), |project| place_note_clip(project, clip))
    }

    fn add_note_clips(&self, clips: Vec<NewNoteClip>) -> Result<Vec<Clip>, ContractError> {
        // One core edit gives one undo step and all-or-nothing: an error in any clip rolls back the whole edit.
        self.project.edit(&tr!("Add clips", "Добавить клипы"), |project| {
            clips.into_iter().map(|clip| place_note_clip(project, clip)).collect()
        })
    }

    fn add_audio_clip(&self, track: TrackId, start_tick: u32, sample: SampleId) -> Result<Clip, ContractError> {
        let data = self.samples.get(sample).ok_or_else(|| samples::NOT_FOUND.with("id", sample))?;
        self.project.edit(&tr!("Add audio clip", "Добавить аудиоклип"), |project| {
            ops::check_sample(project, sample)?;
            // The sound plays in real time: its length in ticks follows the tempo map from the clip position.
            let start = f64::from(start_tick);
            let ticks = project.tempo_map.tick_after_frames(start, data.frame_count() as f64, data.sample_rate) - start;
            let length = (ticks.round() as u32).clamp(1, MAX_CLIP_TICK);
            place(project, track, start_tick, length, ClipContent::audio(sample))
        })
    }

    fn edit_clip(&self, id: ClipId, edit: ClipEdit) -> Result<Clip, ContractError> {
        let facts = self.facts();
        self.project.edit(&tr!("Edit clip", "Изменить клип"), |project| {
            apply_edit(project, id, &edit, &facts)?;
            check_places(project, &[id])?;
            Ok(ops::clip(project, id)?.clone())
        })
    }

    fn split_clip(&self, id: ClipId, tick: u32) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("Split clip", "Разрезать клип"), |project| {
            ops::limit(project.clips.len(), MAX_CLIPS, clips::TOO_MANY)?;
            let original = ops::clip(project, id)?.clone();
            let end = original.start_tick + original.length_ticks;
            if tick <= original.start_tick || tick >= end {
                return Err(clips::SPLIT_OUTSIDE_CLIP.with("tick", tick).with("id", id));
            }
            let left_length = tick - original.start_tick;
            let mut right = original.clone();
            right.id = project.alloc_id();
            right.start_tick = tick;
            right.length_ticks = end - tick;
            right.fade_in_ticks = 0;
            match &original.content {
                // The right half continues the loop phase where the left one ended.
                ClipContent::Notes(content) => {
                    right.offset_ticks = (original.offset_ticks + left_length) % content.loop_ticks;
                }
                ClipContent::Audio { sample, .. } => {
                    // Without the sound its rate is unknown, and a guessed one would shift the right part to the wrong place.
                    let data = self.samples.get(*sample).ok_or_else(|| samples::NOT_FOUND.with("id", sample))?;
                    let frames = original.ticks_to_source_frames(f64::from(left_length), &project.tempo_map, data.sample_rate);
                    right.offset_frames = original.offset_frames + frames.round() as u64;
                }
            }
            let left = ops::clip_mut(project, id)?;
            left.length_ticks = left_length;
            left.fade_out_ticks = 0;
            take_rules::split_comp(&mut left.content, &mut right.content, left_length);
            project.clips.push(right.clone());
            Ok(right)
        })
    }

    fn trim_start(&self, id: ClipId, start_tick: u32) -> Result<Clip, ContractError> {
        // The sound's rate turns ticks into file frames; without the sound the offset would land in the wrong place.
        let sample = self.project.read(|p| ops::clip(p, id).map(|c| c.content.sample()))?;
        let sample_rate = match sample {
            Some(sample) => self.samples.get(sample).ok_or_else(|| samples::NOT_FOUND.with("id", sample))?.sample_rate,
            None => 0,
        };
        self.project.edit(&tr!("Trim clip start", "Обрезать начало клипа"), |project| {
            let trimmed = trim_start(ops::clip(project, id)?, start_tick, &project.tempo_map, sample_rate)?;
            *ops::clip_mut(project, id)? = trimmed.clone();
            check_places(project, &[id])?;
            Ok(trimmed)
        })
    }

    fn duplicate_clip(&self, id: ClipId) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("Duplicate clip", "Дублировать клип"), |project| copy_after(project, id))
    }

    fn remove_clip(&self, id: ClipId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete clip", "Удалить клип"), |project| {
            ops::clip(project, id)?;
            project.clips.retain(|c| c.id != id);
            Ok(())
        })
    }

    fn edit_clips(&self, changes: Vec<(ClipId, ClipEdit)>) -> Result<Vec<Clip>, ContractError> {
        let facts = self.facts();
        self.project.edit(&tr!("Edit clips", "Изменить клипы"), |project| {
            for (id, edit) in &changes {
                apply_edit(project, *id, edit, &facts)?;
            }
            // Places are checked once all edits are applied: clips may swap places.
            let ids: Vec<ClipId> = changes.iter().map(|(id, _)| *id).collect();
            check_places(project, &ids)?;
            ids.iter().map(|&id| ops::clip(project, id).cloned()).collect()
        })
    }

    fn remove_clips(&self, ids: Vec<ClipId>) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete clips", "Удалить клипы"), |project| {
            for &id in &ids {
                ops::clip(project, id)?;
            }
            project.clips.retain(|c| !ids.contains(&c.id));
            Ok(())
        })
    }

    fn copy_clips(&self, ids: Vec<ClipId>, at_tick: u32) -> Result<Vec<Clip>, ContractError> {
        self.project.edit(&tr!("Copy clips", "Копировать клипы"), |project| copy_group(project, &ids, at_tick))
    }

    fn set_loop_region(&self, region: Option<(u32, u32)>) -> Result<(), ContractError> {
        self.project.edit(&tr!("Loop", "Цикл"), |project| {
            if let Some((start, end)) = region
                && start >= end
            {
                return Err(clips::LOOP_REGION_BACKWARDS.with("start", start).with("end", end));
            }
            project.loop_region = region;
            Ok(())
        })
    }
}
