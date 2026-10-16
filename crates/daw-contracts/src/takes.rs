//! Takes of an audio clip (`TakeControl`): several recordings of the same spot; the clip plays one of them whole or a
//! comp of pieces from different takes (comping by segments, `CompSegment`). How a comp sounds is `Clip::take_span`:
//! one formula for the engine, export and tests.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Clip, ClipContent, ClipId, ContractError, SampleId, TempoMap};

/// Maximum number of comp segments in a clip.
pub const MAX_COMP_SEGMENTS: usize = 256;
/// Length of the transition between takes at a comp segment boundary: a short equal-power crossfade so the joint does
/// not click.
pub const COMP_CROSSFADE_SECONDS: f64 = 0.01;

/// A comp segment: from `start_ticks` (relative to the clip start) to the start of the next segment (or the clip end)
/// take `take` plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompSegment {
    pub start_ticks: u32,
    pub take: SampleId,
}

/// What sounds in one piece of a clip: take `take` from `start` to `end` (project ticks, already including the halves
/// of the transitions to its neighbours) and the lengths of the equal-power transitions at the edges (`fade_in`/
/// `fade_out`, ticks; 0 at a clip edge).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeSpan {
    pub take: SampleId,
    pub start: f64,
    pub end: f64,
    pub fade_in: f64,
    pub fade_out: f64,
}

/// Takes of an audio clip. Job: which of the sounds recorded at this spot the clip plays in each of its pieces.
///
/// The clip does not move or change length when the take or comp changes: offset, tempo warp, pitch, gain and fades
/// are shared by all takes, and crossfades with neighbouring clips stay as they were. A comp (`content.comp`) is a list
/// of segments in increasing start order, the first at 0, neighbours with different takes; empty means the active take
/// (`sample`) plays the whole clip. Every mutating method is one undo step; on error the project does not change.
/// An unknown clip or sample is `clips::NOT_FOUND` / `samples::NOT_FOUND`; a note clip is `ErrorKind::Invalid`.
pub trait TakeControl: Send + Sync {
    /// A new take: `sample` becomes active and plays the whole clip (the comp is cleared); the clip's previous sound
    /// stays in the take list. A sample that is already a take is `ErrorKind::Invalid`.
    fn add_take(&self, clip: ClipId, sample: SampleId) -> Result<Clip, ContractError>;
    /// The whole clip plays take `take` (the comp is cleared). Not among the takes is `ErrorKind::Invalid`; already
    /// active with no comp changes nothing (no undo step).
    fn set_active_take(&self, clip: ClipId, take: SampleId) -> Result<Clip, ContractError>;
    /// Removes a take from the clip (the sample stays in the project). If it was active, the clip switches to the last
    /// remaining take; its comp segments go to the previous segment (for the first one, to the next); with one take
    /// left, the take list and comp become empty (a plain clip). The clip's only sound or a foreign sample is
    /// `ErrorKind::Invalid`.
    fn remove_take(&self, clip: ClipId, take: SampleId) -> Result<Clip, ContractError>;
    /// Replaces the whole comp. `segments` go in increasing start order, the first at tick 0, all starts inside the
    /// clip (`< length_ticks`), takes from the clip's takes; otherwise `ErrorKind::Invalid`. More than
    /// [`MAX_COMP_SEGMENTS`] is `ErrorKind::Limit`. Neighbouring segments of the same take merge; if empty or only
    /// one take remains, the comp is cleared and that take (if it is the only one) becomes active.
    fn set_comp(&self, clip: ClipId, segments: Vec<CompSegment>) -> Result<Clip, ContractError>;
    /// Piece `[from, to)` (ticks from the clip start, clamped to the clip) plays take `take`, the rest stays as it was
    /// (without a comp, the active take). This is how the interface picks a piece by dragging over a take lane.
    /// `from >= to` after clamping is `ErrorKind::Invalid`; the result follows the same rules as `set_comp`.
    fn choose_take_range(&self, clip: ClipId, from: u32, to: u32, take: SampleId) -> Result<Clip, ContractError>;
}

impl Clip {
    /// The clip's comp (empty when there is no comp or the clip holds notes).
    pub fn comp(&self) -> &[CompSegment] {
        match &self.content {
            ClipContent::Audio { comp, .. } => comp,
            ClipContent::Notes(_) => &[],
        }
    }

    /// How many pieces with different takes sound in the clip (one without a comp; zero for a note clip).
    pub fn take_span_count(&self) -> usize {
        match &self.content {
            ClipContent::Audio { comp, .. } => comp.len().max(1),
            ClipContent::Notes(_) => 0,
        }
    }

    /// Piece `index` (from zero) of the comp under tempo map `map`. The transition between takes is
    /// [`COMP_CROSSFADE_SECONDS`] at the tempo at the boundary, half on each side, but no longer than half of the
    /// shorter neighbouring segment. Allocation-free: called from the audio thread.
    pub fn take_span(&self, index: usize, map: &TempoMap) -> Option<TakeSpan> {
        let ClipContent::Audio { sample, comp, .. } = &self.content else { return None };
        let clip_start = f64::from(self.start_tick);
        let clip_end = clip_start + f64::from(self.length_ticks);
        if comp.is_empty() {
            return (index == 0).then_some(TakeSpan { take: *sample, start: clip_start, end: clip_end, fade_in: 0.0, fade_out: 0.0 });
        }
        let segment = comp.get(index)?;
        let edge = |i: usize| comp.get(i).map_or(clip_end, |s| clip_start + f64::from(s.start_ticks).min(f64::from(self.length_ticks)));
        let (start, end) = (edge(index), edge(index + 1));
        // Half of the transition at the boundary before segment `i` (between `i - 1` and `i`).
        let half = |i: usize| {
            if i == 0 || i >= comp.len() {
                return 0.0;
            }
            let boundary = edge(i);
            let wanted = COMP_CROSSFADE_SECONDS / 2.0 * map.tempo_at(boundary).get() / 60.0 * f64::from(crate::PPQ);
            let shorter = (boundary - edge(i - 1)).min(edge(i + 1) - boundary);
            wanted.min(shorter / 2.0).max(0.0)
        };
        let (before, after) = (half(index), half(index + 1));
        Some(TakeSpan { take: segment.take, start: start - before, end: end + after, fade_in: before * 2.0, fade_out: after * 2.0 })
    }

    /// Sounds the clip plays now (the active take or the comp's takes, without repeats, in comp order).
    pub fn sounding_takes(&self) -> Vec<SampleId> {
        match &self.content {
            ClipContent::Audio { sample, comp, .. } if comp.is_empty() => vec![*sample],
            ClipContent::Audio { comp, .. } => {
                let mut takes: Vec<SampleId> = Vec::with_capacity(comp.len());
                for segment in comp {
                    if !takes.contains(&segment.take) {
                        takes.push(segment.take);
                    }
                }
                takes
            }
            ClipContent::Notes(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bpm, TimeSignature};

    fn comped(comp: Vec<CompSegment>) -> Clip {
        Clip {
            id: 1,
            track: 1,
            start_tick: 960,
            length_ticks: 3840,
            content: ClipContent::Audio { sample: 1, takes: vec![1, 2], comp },
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        }
    }

    #[test]
    fn spans_overlap_by_a_short_equal_crossfade() {
        let map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        let clip = comped(vec![CompSegment { start_ticks: 0, take: 1 }, CompSegment { start_ticks: 1920, take: 2 }]);
        assert_eq!(clip.take_span_count(), 2);
        // 10 ms at 120 BPM = 19.2 ticks, 9.6 on each side of the boundary at 960 + 1920.
        let first = clip.take_span(0, &map).unwrap();
        let second = clip.take_span(1, &map).unwrap();
        assert_eq!((first.take, first.start, first.fade_in), (1, 960.0, 0.0));
        assert!((first.end - 2889.6).abs() < 1e-9 && (first.fade_out - 19.2).abs() < 1e-9);
        assert!((second.start - 2870.4).abs() < 1e-9 && (second.fade_in - 19.2).abs() < 1e-9);
        assert_eq!((second.take, second.end, second.fade_out), (2, 4800.0, 0.0));
        assert!(clip.take_span(2, &map).is_none());
        assert_eq!(clip.sounding_takes(), vec![1, 2]);
    }

    #[test]
    fn without_comp_the_active_take_plays_the_whole_clip() {
        let map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        let clip = comped(Vec::new());
        assert_eq!(clip.take_span(0, &map), Some(TakeSpan { take: 1, start: 960.0, end: 4800.0, fade_in: 0.0, fade_out: 0.0 }));
        assert_eq!(clip.sounding_takes(), vec![1]);
    }

    #[test]
    fn crossfade_never_eats_a_short_segment() {
        let map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        let clip = comped(vec![
            CompSegment { start_ticks: 0, take: 1 },
            CompSegment { start_ticks: 1000, take: 2 },
            CompSegment { start_ticks: 1004, take: 1 },
        ]);
        let middle = clip.take_span(1, &map).unwrap();
        assert!((middle.fade_in - 4.0).abs() < 1e-9 && (middle.fade_out - 4.0).abs() < 1e-9, "the transition is at most half a segment");
    }

    #[test]
    fn comp_json_is_absent_when_empty() {
        let json = serde_json::to_value(ClipContent::audio(3)).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "audio", "sample": 3 }));
        let comped: ClipContent = serde_json::from_value(
            serde_json::json!({ "type": "audio", "sample": 1, "takes": [1, 2], "comp": [{ "startTicks": 0, "take": 2 }] }),
        )
        .unwrap();
        assert_eq!(comped, ClipContent::Audio { sample: 1, takes: vec![1, 2], comp: vec![CompSegment { start_ticks: 0, take: 2 }] });
    }
}
