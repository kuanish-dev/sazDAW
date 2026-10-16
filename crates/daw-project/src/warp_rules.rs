//! Tempo fitting and pitch shift of an audio clip as pure rules: checking the edit, guessing the source tempo and
//! recomputing the clip length so that it keeps the same piece of sound. `clip_rules::apply_edit` applies them.

use daw_contracts::codes::{clips, samples};
use daw_contracts::{Bpm, Clip, ClipContent, ClipEdit, ContractError, MAX_CLIP_TICK, MAX_PITCH_SEMITONES, Project, SampleStore, Warp};

use crate::ops;

/// Loop lengths in bars tried when guessing the tempo from the length.
const LOOP_BARS: [f64; 5] = [1.0, 2.0, 4.0, 8.0, 16.0];
/// Plausible loop tempo when guessing from the length.
const GUESS_RANGE: std::ops::RangeInclusive<f64> = 70.0..=180.0;
/// The tempo preferred when several fit the length: the middle of the usual range.
const TYPICAL_BPM: f64 = 120.0;
/// A number in the name without "bpm" counts as a tempo only within these bounds (otherwise "kick 01" or "take 3"
/// would become a tempo).
const NAME_RANGE: std::ops::RangeInclusive<f64> = 60.0..=200.0;

/// What fitting needs to know about the clip sound: the sample store (the tempo comes from the project map).
pub(crate) struct SoundFacts<'a> {
    pub samples: &'a dyn SampleStore,
}

/// Applies fitting and pitch from `edit` to `clip`; returns the clip length in ticks that keeps the same piece of
/// sound (an explicit `length_ticks` of the edit may still override it). A note clip with such an edit is `CL-119`.
pub(crate) fn apply(project: &Project, clip: &mut Clip, edit: &ClipEdit, facts: &SoundFacts) -> Result<u32, ContractError> {
    if edit.warp.is_none() && edit.pitch_semitones.is_none() {
        return Ok(clip.length_ticks);
    }
    let ClipContent::Audio { sample, .. } = clip.content else {
        return Err(clips::WARP_NEEDS_AUDIO_CLIP.with("id", clip.id));
    };
    if let Some(semitones) = edit.pitch_semitones {
        let semitones = ops::finite(semitones, clips::PITCH_NOT_A_NUMBER)?;
        if semitones.abs() > MAX_PITCH_SEMITONES {
            return Err(clips::PITCH_OUT_OF_RANGE.with("semitones", semitones).with("max", MAX_PITCH_SEMITONES));
        }
        clip.pitch_semitones = semitones;
    }
    let Some(warp) = edit.warp else { return Ok(clip.length_ticks) };
    let data = facts.samples.get(sample).ok_or_else(|| samples::NOT_FOUND.with("id", sample))?;
    let rate = data.sample_rate;
    let remaining = data.frame_count().saturating_sub(clip.offset_frames as usize) as f64;
    let map = &project.tempo_map;
    let covered = clip.ticks_to_source_frames(f64::from(clip.length_ticks), map, rate).min(remaining);
    clip.warp = match warp {
        None => None,
        Some(edit) => {
            let source_bpm = if let Some(bpm) = edit.source_bpm {
                Bpm::new(bpm)?.get()
            } else {
                let name = project.samples.iter().find(|s| s.id == sample).map_or("", |s| s.name.as_str());
                guess_bpm(name, data.frame_count(), rate).unwrap_or(map.tempo_at(f64::from(clip.start_tick)).get())
            };
            Some(Warp { source_bpm })
        }
    };
    let ticks = clip.source_frames_to_ticks(covered, map, rate).round();
    Ok((ticks as u32).clamp(1, MAX_CLIP_TICK))
}

/// The source tempo of a sound: from the file name ("loop_128bpm", "Drums 140 Am"), otherwise from the length — a whole
/// number of 4/4 bars (1, 2, 4, 8, 16) at 70–180 BPM, the one closest to 120 among those that fit. `None` — no clue.
pub(crate) fn guess_bpm(name: &str, frames: usize, sample_rate: u32) -> Option<f64> {
    bpm_from_name(name).or_else(|| bpm_from_length(frames, sample_rate))
}

fn bpm_from_name(name: &str) -> Option<f64> {
    let lower = name.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut standalone = None;
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))) {
            i += 1;
        }
        let Ok(value) = chars[start..i].iter().collect::<String>().parse::<f64>() else { continue };
        let rest: String = chars[i..].iter().skip_while(|c| matches!(c, ' ' | '_' | '-')).take(3).collect();
        if rest == "bpm" && Bpm::new(value).is_ok() {
            return Some(value);
        }
        let before_ok = start == 0 || !chars[start - 1].is_alphanumeric();
        let after_ok = chars.get(i).is_none_or(|c| !c.is_alphanumeric());
        if standalone.is_none() && before_ok && after_ok && NAME_RANGE.contains(&value) {
            standalone = Some(value);
        }
    }
    standalone
}

fn bpm_from_length(frames: usize, sample_rate: u32) -> Option<f64> {
    let seconds = frames as f64 / f64::from(sample_rate.max(1));
    if seconds <= 0.0 {
        return None;
    }
    LOOP_BARS
        .iter()
        .map(|bars| bars * 4.0 * 60.0 / seconds)
        .filter(|bpm| GUESS_RANGE.contains(bpm))
        .min_by(|a, b| (a / TYPICAL_BPM).log2().abs().total_cmp(&(b / TYPICAL_BPM).log2().abs()))
        .map(|bpm| (bpm * 1000.0).round() / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_with_bpm_wins() {
        assert_eq!(bpm_from_name("Loop_128bpm_Am"), Some(128.0));
        assert_eq!(bpm_from_name("drums 92 BPM"), Some(92.0));
        assert_eq!(bpm_from_name("break-174_bpm"), Some(174.0));
        assert_eq!(bpm_from_name("Pad 01 128bpm"), Some(128.0), "\"01\" is not a tempo, \"128bpm\" is");
        assert_eq!(bpm_from_name("vox 87.5bpm"), Some(87.5));
    }

    #[test]
    fn standalone_number_in_a_plausible_range() {
        assert_eq!(bpm_from_name("Drums 140 Am"), Some(140.0));
        assert_eq!(bpm_from_name("guitar_95_take2"), Some(95.0));
        assert_eq!(bpm_from_name("kick 01"), None);
        assert_eq!(bpm_from_name("loop128"), None, "a number glued to a word without \"bpm\" is not a tempo");
        assert_eq!(bpm_from_name("808 bass"), None, "out of bounds");
    }

    #[test]
    fn length_fits_whole_bars() {
        // 2 seconds: 4 bars would be 480 BPM — no; 1 bar = 120 BPM.
        assert_eq!(bpm_from_length(96_000, 48_000), Some(120.0));
        // 4 bars at 128 BPM = 7.5 s.
        assert_eq!(bpm_from_length(360_000, 48_000), Some(128.0));
        // 3.2 s: 1 bar = 75, 2 bars = 150 → 150 is closer to 120 in octaves.
        assert_eq!(bpm_from_length(153_600, 48_000), Some(150.0));
        assert_eq!(bpm_from_length(48_000 * 100, 48_000), None, "too long for 16 bars at 70+");
        assert_eq!(bpm_from_length(0, 48_000), None);
        assert_eq!(guess_bpm("Drums 140", 96_000, 48_000), Some(140.0), "the name wins over the length");
    }
}
