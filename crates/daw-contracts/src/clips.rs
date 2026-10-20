//! Clips on the timeline: position, length, gain and fades of clips, and the loop region (`ClipControl`).

use std::f64::consts::{FRAC_PI_2, PI};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Clip, ClipId, ContractError, NewNote, NewStepClip, PPQ, SampleId, TrackId, WarpEdit};

/// Maximum number of clips in a project.
pub const MAX_CLIPS: usize = 4096;
/// A clip cannot start after this tick or be longer than it (about 4 hours at 130 BPM), so their sum never overflows
/// `u32`.
pub const MAX_CLIP_TICK: u32 = PPQ * 60 * 500;
/// Maximum clip gain (linear, ≈ +12 dB).
pub const MAX_CLIP_GAIN: f32 = 4.0;

/// Curve of an audio clip fade. Every shape goes from silence (progress 0, the clip edge) to full level (progress 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FadeShape {
    #[default]
    Linear,
    /// Sine quarter: the level at the middle is −3 dB, so a fade between two sounds keeps their sum's power.
    EqualPower,
    /// Slow at both ends, fast in the middle.
    SCurve,
    /// Slow start, fast end (close to a straight line in decibels).
    Exponential,
}

/// Steepness of `FadeShape::Exponential`: the curve is `(e^(k·x) − 1) / (e^k − 1)`.
const EXPONENTIAL_STEEPNESS: f64 = 5.0;

impl FadeShape {
    /// Gain at `progress` through the fade (clamped to 0..=1). Allocation-free: the engine calls it per frame. The
    /// interface draws the curves with the same formulas (`app/src/tracks/fades.ts`).
    pub fn gain(self, progress: f64) -> f64 {
        let x = progress.clamp(0.0, 1.0);
        match self {
            FadeShape::Linear => x,
            FadeShape::EqualPower => (x * FRAC_PI_2).sin(),
            FadeShape::SCurve => 0.5 - 0.5 * (x * PI).cos(),
            FadeShape::Exponential => (EXPONENTIAL_STEEPNESS * x).exp_m1() / EXPONENTIAL_STEEPNESS.exp_m1(),
        }
    }
}

/// Partial clip change; `None` (field omitted) leaves the value unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ClipEdit {
    pub track: Option<TrackId>,
    pub start_tick: Option<u32>,
    pub length_ticks: Option<u32>,
    pub gain: Option<f32>,
    pub fade_in_ticks: Option<u32>,
    pub fade_out_ticks: Option<u32>,
    pub fade_in_shape: Option<FadeShape>,
    pub fade_out_shape: Option<FadeShape>,
    /// Warping an audio clip to the tempo: `Some(Some(..))` turns it on or changes the source tempo
    /// (`sourceBpm: null` guesses it), `Some(None)` (JSON `null`) turns it off. See `ClipControl::edit_clip`.
    #[serde(deserialize_with = "present")]
    #[allow(clippy::option_option, reason = "three JSON cases: field absent, `null` (turn off) and a value")]
    pub warp: Option<Option<WarpEdit>>,
    /// Pitch shift of an audio clip in semitones.
    pub pitch_semitones: Option<f32>,
}

/// A field present in JSON (even `null`) becomes `Some(..)`, so `null` differs from an omitted field (`default`).
#[allow(clippy::option_option, reason = "three JSON cases: field absent, `null` and a value")]
pub(crate) fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// A new note clip together with its content, so a clip with notes is created in one call and one undo step.
/// Empty `notes` and `None` everywhere give an empty one-bar clip with automatic content length.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewNoteClip {
    pub track: TrackId,
    pub start_tick: u32,
    /// Clip length on the timeline; `None` means one bar of the time signature in effect at `start_tick`.
    pub length_ticks: Option<u32>,
    /// Content notes (times are relative to the clip start).
    #[serde(default)]
    pub notes: Vec<NewNote>,
    /// Length of the repeating content; `None` means automatic length (fitted to the notes in whole bars of the time
    /// signature at the clip start, at least one bar).
    pub loop_ticks: Option<u32>,
}

impl NewNoteClip {
    /// An empty clip without notes.
    pub fn empty(track: TrackId, start_tick: u32, length_ticks: Option<u32>) -> Self {
        Self { track, start_tick, length_ticks, notes: Vec::new(), loop_ticks: None }
    }
}

/// Clips on the timeline. Job: position and length of clips on tracks, and the loop region.
///
/// Rules for all methods: note clips on one track do not overlap. Audio clips on an audio track may overlap; the
/// overlap plays as an automatic equal-power crossfade as long as the overlap (`Clip::crossfades`), but no point has
/// more than two clips and no clip lies entirely inside another (including starting at the same tick). A violation
/// is `ErrorKind::Invalid`. Note clips live only on instrument tracks, audio clips only on audio tracks (otherwise
/// `ErrorKind::Invalid`); start and length are within [`MAX_CLIP_TICK`] and the length is non-zero (otherwise
/// `ErrorKind::Limit`); at most [`MAX_CLIPS`] clips (`ErrorKind::Limit`). An unknown clip or track is
/// `clips::NOT_FOUND` or `tracks::NOT_FOUND`. Each changing method is one undo step; on error the project is unchanged.
pub trait ClipControl: Send + Sync {
    fn clips(&self) -> Vec<Clip>;
    fn loop_region(&self) -> Option<(u32, u32)>;
    /// A note clip with content: the same as an empty clip followed by `NoteControl::replace_notes(notes)` and (if
    /// given) `NoteControl::set_loop_ticks(loop_ticks)`, but as one undo step and all or nothing: on any error no clip
    /// is created. Notes are validated as in `NoteControl` (`NewNote::validate` with the `MAX_CLIP_LOOP_TICKS` limit,
    /// at most `MAX_NOTES_PER_CLIP`, otherwise `ErrorKind::Limit`); without `loop_ticks` the content length is fitted
    /// to the notes (up to the end of the last occupied bar, at least one bar). `loop_ticks` must be a multiple of
    /// `STEP_TICKS`, from one step to `MAX_CLIP_LOOP_TICKS` (otherwise `ErrorKind::Limit`) and turns automatic length
    /// off; notes starting after it are dropped, longer ones are cut.
    fn add_note_clip(&self, clip: NewNoteClip) -> Result<Clip, ContractError>;
    /// Several note clips (for example, all sections of a part), each as in `add_note_clip`, but as one undo step and
    /// all or nothing: an error in any clip (the place is taken, including by another clip of the same list, or a bad
    /// note) creates none. Returns the clips in `clips` order; an empty list does nothing.
    fn add_note_clips(&self, clips: Vec<NewNoteClip>) -> Result<Vec<Clip>, ContractError>;
    /// A note clip written as a melodic step line (`step_line`): the line becomes notes (`NewStepClip::notes`, scale
    /// degrees in the project key `Project::key`) and the clip is made as by `add_note_clip` with `loop_ticks` = the
    /// line's length, so a longer clip repeats the line; without `length_ticks` the clip is one pass. One undo step,
    /// all or nothing. Errors: the line's (`NewStepClip::notes`), then those of `add_note_clip` (a line whose length
    /// is not a multiple of `STEP_TICKS` is `notes::LOOP_LENGTH_OUT_OF_RANGE`).
    fn add_step_clip(&self, clip: NewStepClip) -> Result<Clip, ContractError>;
    /// An audio clip covering the whole sample: its length in ticks comes from the sound length and the CURRENT tempo
    /// (later tempo changes do not stretch the clip). A sample missing from the project is `samples::NOT_FOUND`.
    fn add_audio_clip(&self, track: TrackId, start_tick: u32, sample: SampleId) -> Result<Clip, ContractError>;
    /// Gain is from 0 to [`MAX_CLIP_GAIN`] (outside the range `ErrorKind::Limit`, not a number `ErrorKind::Invalid`);
    /// fades are clamped to the clip length. Fade shapes may be set on any clip; only audio clips sound them.
    ///
    /// Warp and pitch apply only to audio clips (on note clips `ErrorKind::Invalid`). Pitch is fractional, from
    /// −[`crate::MAX_PITCH_SEMITONES`] to + (outside `ErrorKind::Limit`). Warp: the source tempo is within `Bpm`
    /// (otherwise `tempo::INVALID_TEMPO`), `None` guesses it (`WarpEdit`). When warp changes, the clip keeps the same
    /// piece of sound: its length in ticks is recalculated (warped by beats of the source tempo, unwarped by the
    /// CURRENT tempo); an explicit `length_ticks` in the same edit wins. A clip whose sample is not in memory is
    /// `samples::NOT_FOUND`.
    fn edit_clip(&self, id: ClipId, edit: ClipEdit) -> Result<Clip, ContractError>;
    /// Splits the clip in two at `tick`, strictly inside the clip (otherwise `ErrorKind::Invalid`); returns the right
    /// part. The right part continues the content from the same point (`offset_ticks`/`offset_frames`); the right
    /// part's fade-in and the left part's fade-out are reset. An audio clip whose sample is gone from memory is
    /// `samples::NOT_FOUND`.
    fn split_clip(&self, id: ClipId, tick: u32) -> Result<Clip, ContractError>;
    /// Moves the clip's left edge to `start_tick` and keeps its end, while the content stays where it is on the
    /// timeline (trimming or revealing the beginning): the content offset moves together with the start (an audio
    /// clip's `offset_frames` by the frames those ticks play, a note clip's `offset_ticks` around its loop). An audio
    /// clip cannot reveal sound before the file start: the start is clamped to the tick where `offset_frames` reaches
    /// 0. The comp follows the content, the fade-in is clamped to the new length. `start_tick` at or after the clip end
    /// is `clips::TRIM_PAST_END`; an audio clip whose sample is not in memory is `samples::NOT_FOUND`; placement as in
    /// every method. Returns the changed clip.
    fn trim_start(&self, id: ClipId, start_tick: u32) -> Result<Clip, ContractError>;
    /// A copy of the clip right after it; returns the copy. If the place is taken, `ErrorKind::Invalid`.
    fn duplicate_clip(&self, id: ClipId) -> Result<Clip, ContractError>;
    fn remove_clip(&self, id: ClipId) -> Result<(), ContractError>;
    /// Edits several clips in one undo step. Overlaps and track kind are checked against the FINAL state, so a group
    /// can move tightly packed and swap places. On any error the project is unchanged. Returns the changed clips in
    /// `changes` order; an empty list does nothing and returns an empty list.
    fn edit_clips(&self, changes: Vec<(ClipId, ClipEdit)>) -> Result<Vec<Clip>, ContractError>;
    /// Removes clips in one undo step; an unknown id is an error and nothing is removed. An empty list does nothing.
    fn remove_clips(&self, ids: Vec<ClipId>) -> Result<(), ContractError>;
    /// Copies of clips (with content, loop and offset; new ids) on the same tracks in one undo step. The leftmost copy
    /// lands at `at_tick`, the others keep their distances to it. Space is checked against the final state (the
    /// originals stay); a taken place is an error. An empty list is `ErrorKind::Invalid`.
    fn copy_clips(&self, ids: Vec<ClipId>, at_tick: u32) -> Result<Vec<Clip>, ContractError>;
    /// Loop region `[start, end)` in ticks; `None` removes the loop. `start >= end` is `ErrorKind::Invalid`. The region
    /// is passed to the transport immediately.
    fn set_loop_region(&self, region: Option<(u32, u32)>) -> Result<(), ContractError>;
}

#[cfg(test)]
mod tests {
    use super::FadeShape;

    const SHAPES: [FadeShape; 4] = [FadeShape::Linear, FadeShape::EqualPower, FadeShape::SCurve, FadeShape::Exponential];

    #[test]
    fn every_fade_shape_rises_from_silence_to_full_level() {
        for shape in SHAPES {
            assert!(shape.gain(0.0).abs() < 1e-9, "{shape:?} starts silent");
            assert!((shape.gain(1.0) - 1.0).abs() < 1e-9, "{shape:?} ends at full level");
            let mut previous = 0.0;
            for step in 1..=100 {
                let gain = shape.gain(f64::from(step) / 100.0);
                assert!(gain >= previous, "{shape:?} never falls");
                previous = gain;
            }
        }
    }

    #[test]
    fn fade_shapes_differ_in_the_middle_and_clamp_progress() {
        assert!((FadeShape::Linear.gain(0.5) - 0.5).abs() < 1e-9);
        assert!((FadeShape::EqualPower.gain(0.5) - 0.5f64.sqrt()).abs() < 1e-9, "−3 dB at the middle");
        assert!((FadeShape::SCurve.gain(0.5) - 0.5).abs() < 1e-9);
        assert!(FadeShape::Exponential.gain(0.5) < 0.2, "slow start");
        assert!(FadeShape::SCurve.gain(0.1) < FadeShape::Linear.gain(0.1));
        assert_eq!(FadeShape::EqualPower.gain(-1.0), 0.0);
        assert_eq!(FadeShape::Linear.gain(2.0), 1.0);
    }

    #[test]
    fn fade_shapes_use_camel_case_names_and_default_to_linear() {
        assert_eq!(serde_json::to_string(&FadeShape::EqualPower).unwrap(), "\"equalPower\"");
        assert_eq!(serde_json::from_str::<FadeShape>("\"sCurve\"").unwrap(), FadeShape::SCurve);
        assert_eq!(FadeShape::default(), FadeShape::Linear);
    }
}
