//! `ProjectClipProcessing`: normalize and reverse of audio clips. It decides what the clip changes (its gain, the
//! sample it plays and the offset); the audio itself is measured and written by the sample editor, asked through its
//! contract (`SampleEditor::peak`, `SampleEditor::reverse_copy`).

use std::sync::Arc;

use daw_contracts::codes::{clips, samples};
use daw_contracts::{
    Clip, ClipContent, ClipId, ClipProcessing, ContractError, HistoryControl, MAX_CLIP_GAIN, MIN_NORMALIZE_PEAK_DB, SampleEditor, SampleId,
    SampleStore, TempoMap, tr,
};

use crate::{ProjectService, ops};

pub struct ProjectClipProcessing {
    project: Arc<ProjectService>,
    samples: Arc<dyn SampleStore>,
    editor: Arc<dyn SampleEditor>,
}

impl ProjectClipProcessing {
    pub fn new(project: Arc<ProjectService>, samples: Arc<dyn SampleStore>, editor: Arc<dyn SampleEditor>) -> Self {
        Self { project, samples, editor }
    }

    /// The clip and the tempo map (they turn the clip length into source frames).
    fn clip(&self, id: ClipId) -> Result<(Clip, TempoMap), ContractError> {
        self.project.read(|p| Ok((ops::clip(p, id)?.clone(), p.tempo_map.clone())))
    }

    /// The source frames `[from, to)` of `take` that the clip plays (to is not clamped to the sound).
    fn played_frames(&self, clip: &Clip, take: SampleId, map: &TempoMap) -> Result<(usize, usize), ContractError> {
        let data = self.samples.get(take).ok_or_else(|| samples::NOT_FOUND.with("id", take))?;
        let from = usize::try_from(clip.offset_frames).unwrap_or(usize::MAX);
        let frames = clip.ticks_to_source_frames(f64::from(clip.length_ticks), map, data.sample_rate).ceil().max(0.0) as usize;
        Ok((from, from.saturating_add(frames)))
    }

    /// The loudest sample the clip plays, over all its takes.
    fn played_peak(&self, clip: &Clip, map: &TempoMap) -> Result<f32, ContractError> {
        let ClipContent::Audio { sample, takes, .. } = &clip.content else {
            return Err(clips::NEEDS_AUDIO_CLIP.with("id", clip.id));
        };
        let mut peak = 0.0f32;
        for &take in std::iter::once(sample).chain(takes) {
            let (from, to) = self.played_frames(clip, take, map)?;
            peak = peak.max(self.editor.peak(take, from, to)?);
        }
        Ok(peak)
    }
}

impl ClipProcessing for ProjectClipProcessing {
    fn normalize(&self, ids: Vec<ClipId>, peak_db: f32) -> Result<Vec<Clip>, ContractError> {
        let peak_db = ops::finite(peak_db, clips::NORMALIZE_LEVEL_NOT_A_NUMBER)?;
        if !(MIN_NORMALIZE_PEAK_DB..=0.0).contains(&peak_db) {
            return Err(clips::NORMALIZE_LEVEL_OUT_OF_RANGE.with("db", peak_db).with("min", MIN_NORMALIZE_PEAK_DB));
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let target = 10f32.powf(peak_db / 20.0);
        let gains = ids
            .iter()
            .map(|&id| {
                let (clip, map) = self.clip(id)?;
                let peak = self.played_peak(&clip, &map)?;
                Ok((peak > 0.0).then(|| (target / peak).min(MAX_CLIP_GAIN)))
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        self.project.edit(&tr!("Normalize", "Нормализовать"), |project| {
            ids.iter()
                .zip(&gains)
                .map(|(&id, gain)| {
                    let clip = ops::clip_mut(project, id)?;
                    if let Some(gain) = gain {
                        clip.gain = *gain;
                    }
                    Ok(clip.clone())
                })
                .collect()
        })
    }

    fn reverse(&self, id: ClipId) -> Result<Clip, ContractError> {
        let (clip, map) = self.clip(id)?;
        let ClipContent::Audio { sample, takes, .. } = &clip.content else {
            return Err(clips::NEEDS_AUDIO_CLIP.with("id", id));
        };
        if takes.len() > 1 {
            return Err(clips::REVERSE_NEEDS_ONE_TAKE.with("id", id));
        }
        let total = self.samples.get(*sample).ok_or_else(|| samples::NOT_FOUND.with("id", sample))?.frame_count();
        let (_, to) = self.played_frames(&clip, *sample, &map)?;
        let mirrored = total.saturating_sub(to.min(total)) as u64;
        let label = tr!("Reverse clip", "Развернуть клип");
        self.project.begin_group(&label);
        let result = self.editor.reverse_copy(*sample).and_then(|reversed| {
            self.project.edit(&label, |project| {
                let clip = ops::clip_mut(project, id)?;
                clip.content = ClipContent::audio(reversed.id);
                clip.offset_frames = mirrored;
                Ok(clip.clone())
            })
        });
        if result.is_ok() {
            self.project.end_group();
        } else {
            self.project.cancel_group();
        }
        result
    }
}
