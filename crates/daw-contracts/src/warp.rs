//! Fitting audio clips to the tempo (warp) and pitch shifting: the clip settings (`Warp`), a sound processing job
//! (`ProcessJob`) and the contract for ready processed audio (`ProcessedAudio`). Processing runs outside the audio
//! thread: the engine plays the clip's ready sound (`ClipAudio` in the snapshot) like a plain sample.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Clip, ClipContent, ContractError, PPQ, SampleData, SampleId, TempoMap};

/// Largest pitch shift of an audio clip in semitones (either direction).
pub const MAX_PITCH_SEMITONES: f32 = 24.0;
/// How far processing may stretch or squeeze a sound: beyond this the sound degrades, and a file stretched tens of
/// times would take gigabytes of memory. Jobs outside the limits are rejected with a `Limit`-kind error.
pub const MIN_STRETCH: f64 = 0.25;
pub const MAX_STRETCH: f64 = 4.0;

/// Fits an audio clip to the project tempo: the sound was recorded at `source_bpm` and is stretched by
/// `source_bpm / project tempo`, so a loop stays on the beats at any tempo.
/// The clip's length in ticks does not depend on the tempo: it comes from the sound's length in beats of `source_bpm`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Warp {
    /// The sound's original tempo, within [`Bpm::MIN`]..=[`Bpm::MAX`].
    pub source_bpm: f64,
}

/// Turning warp on or editing it in `ClipEdit`. `source_bpm: None` means guess from the file name ("…128bpm…",
/// "… 140 …"), otherwise from the sound's length: a whole number of bars (1, 2, 4, 8, 16) at a plausible 70–180 BPM.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct WarpEdit {
    pub source_bpm: Option<f64>,
}

impl Clip {
    /// Whether warp or pitch shift is on: the engine plays such an audio clip only from `EngineSnapshot::clip_audio`
    /// (even if at the current tempo the sound matches the file).
    pub fn has_sound_processing(&self) -> bool {
        matches!(self.content, ClipContent::Audio { .. }) && (self.warp.is_some() || self.pitch_semitones != 0.0)
    }

    /// How many source frames (at `sample_rate`) pass during the first `ticks` ticks of the clip: for a warped clip, by
    /// its source tempo (the map does not matter); for a plain one, by the tempo map from the clip start (the sound
    /// plays in real time, so its length in ticks depends on where the clip sits).
    pub fn ticks_to_source_frames(&self, ticks: f64, map: &TempoMap, sample_rate: u32) -> f64 {
        let start = f64::from(self.start_tick);
        if let Some(warp) = self.warp {
            ticks / warp_ticks_per_frame(warp, sample_rate)
        } else {
            map.frames_between(start, start + ticks, sample_rate)
        }
    }

    /// The inverse of [`Clip::ticks_to_source_frames`]: how many ticks from the clip start `frames` source frames take.
    pub fn source_frames_to_ticks(&self, frames: f64, map: &TempoMap, sample_rate: u32) -> f64 {
        let start = f64::from(self.start_tick);
        if let Some(warp) = self.warp {
            frames * warp_ticks_per_frame(warp, sample_rate)
        } else {
            map.tick_after_frames(start, frames, sample_rate) - start
        }
    }
}

fn warp_ticks_per_frame(warp: Warp, sample_rate: u32) -> f64 {
    warp.source_bpm / 60.0 * f64::from(PPQ) / f64::from(sample_rate.max(1))
}

/// Spacing of stretch anchors inside a tempo ramp (ticks): stretching is linear between anchors, and at this spacing
/// the error against the exact map is a fraction of a millisecond.
const RAMP_ANCHOR_TICKS: f64 = 120.0;

/// A stretch anchor along the clip: source frame `source` (from the file start) sounds at frame `output` of the
/// processed sound (from the clip start, at the source sample rate). Linear between anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StretchAnchor {
    pub source: u64,
    pub output: u64,
}

/// A sound processing job: the sample, a time stretch (how many times longer the sound becomes) and a pitch shift.
/// Values are stored rounded (stretch to millionths, pitch to hundredths of a semitone), so jobs compare exactly and
/// serve as cache keys. If the tempo changes inside a warped clip there is no single stretch: the job then has a
/// curve (`curve`) of anchors for just the clip's part, and the anchors themselves are the cache key (they depend only
/// on the tempo points inside the clip and the tempo at its start, the clip position, offset and source tempo).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessJob {
    pub sample: SampleId,
    stretch_ppm: u32,
    cents: i32,
    curve: Option<Arc<[StretchAnchor]>>,
}

impl ProcessJob {
    pub fn new(sample: SampleId, stretch: f64, semitones: f32) -> Self {
        Self { sample, stretch_ppm: ppm(stretch), cents: (semitones * 100.0).round() as i32, curve: None }
    }

    /// A job stretched along a curve (`anchors` ascending, the first at the clip start with `output` = 0).
    /// The job's `stretch` is the curve's average stretch (for the log and checks).
    pub fn with_curve(sample: SampleId, anchors: Vec<StretchAnchor>, semitones: f32) -> Self {
        let (first, last) = (anchors.first().copied(), anchors.last().copied());
        let stretch = match (first, last) {
            (Some(a), Some(b)) if b.source > a.source => (b.output - a.output) as f64 / (b.source - a.source) as f64,
            _ => 1.0,
        };
        Self { sample, stretch_ppm: ppm(stretch), cents: (semitones * 100.0).round() as i32, curve: Some(anchors.into()) }
    }

    /// What to do with take `take` of the clip under tempo map `map` (`sample_rate` is the take's sample rate); `None`
    /// for a note clip or when the sound plays as is (no warp or at its own tempo, no pitch shift). If the tempo does
    /// not change inside a warped clip, there is one stretch (by the tempo at the clip start); otherwise a curve: each
    /// source beat lands on the tick its source tempo puts it at, and the tick's time comes from the map.
    pub fn for_clip(clip: &Clip, take: SampleId, map: &TempoMap, sample_rate: u32) -> Option<Self> {
        let ClipContent::Audio { .. } = clip.content else { return None };
        let start = f64::from(clip.start_tick);
        let tempo = map.tempo_at(start);
        if clip.warp.is_some() && tempo_varies(map, start, start + f64::from(clip.length_ticks)) {
            return Some(Self::with_curve(take, curve_anchors(clip, map, sample_rate), clip.pitch_semitones));
        }
        let stretch = clip.warp.map_or(1.0, |warp| warp.source_bpm / tempo.get());
        let job = Self::new(take, stretch, clip.pitch_semitones);
        (job.stretch_ppm != 1_000_000 || job.cents != 0).then_some(job)
    }

    pub fn stretch(&self) -> f64 {
        f64::from(self.stretch_ppm) / 1e6
    }

    pub fn semitones(&self) -> f32 {
        self.cents as f32 / 100.0
    }

    /// The stretch curve (`None` means one stretch for the whole sound).
    pub fn curve(&self) -> Option<&[StretchAnchor]> {
        self.curve.as_deref()
    }

    /// The processed frame that sounds at the clip start: with a single stretch the whole sound is processed
    /// (`offset_frames × stretch`); with a curve only the clip's part is, from its start.
    pub fn start_frame(&self, clip: &Clip) -> f64 {
        if self.curve.is_some() { 0.0 } else { clip.offset_frames as f64 * self.stretch() }
    }
}

fn ppm(stretch: f64) -> u32 {
    (stretch.max(0.0) * 1e6).round().min(f64::from(u32::MAX)) as u32
}

/// Whether the tempo changes between `start` and `end`: a point with a different tempo inside, or a start in a ramp.
fn tempo_varies(map: &TempoMap, start: f64, end: f64) -> bool {
    let base = map.tempo_at(start).get();
    let inside = map.tempos().iter().any(|c| f64::from(c.tick) > start && f64::from(c.tick) < end && c.bpm.get() != base);
    inside || (map.in_ramp(start) && map.tempo_at(end).get() != base)
}

/// Anchors of the clip's stretch curve: the start, every tempo point inside, steps of [`RAMP_ANCHOR_TICKS`] inside
/// ramps, and the end. The tempo is constant (or nearly) between them, so a linear frame mapping is accurate.
fn curve_anchors(clip: &Clip, map: &TempoMap, sample_rate: u32) -> Vec<StretchAnchor> {
    let start = f64::from(clip.start_tick);
    let end = start + f64::from(clip.length_ticks);
    let anchor = |tick: f64| StretchAnchor {
        source: clip.offset_frames + clip.ticks_to_source_frames(tick - start, map, sample_rate).round().max(0.0) as u64,
        output: map.frames_between(start, tick, sample_rate).round().max(0.0) as u64,
    };
    let mut anchors = vec![anchor(start)];
    let mut tick = start;
    while tick < end {
        let mut next = map.next_change(tick).unwrap_or(end).min(end);
        if map.in_ramp(tick) {
            next = next.min(tick + RAMP_ANCHOR_TICKS);
        }
        tick = next;
        anchors.push(anchor(tick));
    }
    anchors.dedup_by(|b, a| b.output <= a.output || b.source <= a.source);
    anchors
}

/// Ready processed audio of a clip take in the engine snapshot: `data` is the processed sound, `start_frame` its
/// (fractional, at `data`'s rate) frame that sounds at the clip start; from there the sound plays in real time.
#[derive(Debug, Clone)]
pub struct ClipAudio {
    pub data: Arc<SampleData>,
    pub start_frame: f64,
}

/// Processed clip audio. Job: keep stretched and pitch-shifted sound ready for the engine, computed outside the audio
/// thread. The source sound comes from the sample store by `ProcessJob::sample`.
pub trait ProcessedAudio: Send + Sync {
    /// The job's ready sound if already computed; otherwise queues the work in the background and returns `None` at
    /// once (does not wait).
    fn ready(&self, job: &ProcessJob) -> Option<Arc<SampleData>>;
    /// Computes and waits (offline export); ready results come from the cache. A missing sample is
    /// `samples::NOT_FOUND`; a stretch outside [`MIN_STRETCH`]..=[`MAX_STRETCH`] is a `Limit`-kind error.
    fn compute(&self, job: &ProcessJob) -> Result<Arc<SampleData>, ContractError>;
    /// How many background jobs have finished (successfully or not) since creation: growth means the engine snapshot
    /// should be republished. Failed jobs are logged and not retried until the job leaves `retain`.
    fn finished(&self) -> u64;
    /// Only these jobs are needed: other ready results are dropped from memory, and work not yet started is cancelled.
    fn retain(&self, keep: &[ProcessJob]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bpm, NoteContent};

    fn audio_clip(warp: Option<Warp>, pitch: f32) -> Clip {
        Clip {
            id: 1,
            track: 1,
            start_tick: 0,
            length_ticks: 3840,
            content: ClipContent::audio(7),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp,
            pitch_semitones: pitch,
        }
    }

    #[test]
    fn job_exists_only_when_the_sound_changes() {
        let tempo = &TempoMap::constant(Bpm::new(120.0).unwrap(), crate::TimeSignature::FOUR_FOUR);
        assert_eq!(ProcessJob::for_clip(&audio_clip(None, 0.0), 7, tempo, 48_000), None);
        assert_eq!(ProcessJob::for_clip(&audio_clip(Some(Warp { source_bpm: 120.0 }), 0.0), 7, tempo, 48_000), None, "tempo matches");
        let job = ProcessJob::for_clip(&audio_clip(Some(Warp { source_bpm: 90.0 }), 0.0), 7, tempo, 48_000).unwrap();
        assert!((job.stretch() - 0.75).abs() < 1e-9 && job.semitones() == 0.0 && job.sample == 7);
        let pitched = ProcessJob::for_clip(&audio_clip(None, -3.5), 7, tempo, 48_000).unwrap();
        assert!((pitched.stretch() - 1.0).abs() < 1e-9 && pitched.semitones() == -3.5);
        let mut notes = audio_clip(None, 2.0);
        notes.content = ClipContent::Notes(NoteContent::empty());
        assert_eq!(ProcessJob::for_clip(&notes, 7, tempo, 48_000), None);
    }

    #[test]
    fn plain_clip_counts_frames_by_the_map_from_its_start() {
        // 120 BPM until bar 2, then 60: a clip from tick 1920 gets 1920 ticks at 0.5 s per beat, then a second per
        // beat.
        let changes = vec![
            crate::TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
            crate::TempoChange { tick: 3840, bpm: Bpm::new(60.0).unwrap(), ramp: false },
        ];
        let map = TempoMap::new(changes, vec![crate::SignatureChange { tick: 0, signature: crate::TimeSignature::FOUR_FOUR }]).unwrap();
        let mut plain = audio_clip(None, 0.0);
        plain.start_tick = 1920;
        assert!((plain.ticks_to_source_frames(3840.0, &map, 48_000) - 3.0 * 48_000.0).abs() < 1e-6);
        assert!((plain.source_frames_to_ticks(3.0 * 48_000.0, &map, 48_000) - 3840.0).abs() < 1e-6);
        let mut warped = audio_clip(Some(Warp { source_bpm: 90.0 }), 0.0);
        warped.start_tick = 3840;
        assert!((ProcessJob::for_clip(&warped, 7, &map, 48_000).unwrap().stretch() - 1.5).abs() < 1e-9, "tempo at the clip start");
    }

    #[test]
    fn tempo_change_inside_a_warped_clip_gives_a_curve() {
        // 120 BPM until bar 2, then 60: source beats (120 BPM) run twice as slow after the change.
        let changes = vec![
            crate::TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
            crate::TempoChange { tick: 3840, bpm: Bpm::new(60.0).unwrap(), ramp: false },
        ];
        let map = TempoMap::new(changes, vec![crate::SignatureChange { tick: 0, signature: crate::TimeSignature::FOUR_FOUR }]).unwrap();
        let mut clip = audio_clip(Some(Warp { source_bpm: 120.0 }), 0.0);
        clip.length_ticks = 7680;
        clip.offset_frames = 100;
        let job = ProcessJob::for_clip(&clip, 7, &map, 48_000).unwrap();
        let anchors = [
            StretchAnchor { source: 100, output: 0 },
            StretchAnchor { source: 96_100, output: 96_000 },
            StretchAnchor { source: 192_100, output: 288_000 },
        ];
        assert_eq!(job.curve(), Some(&anchors[..]));
        assert!((job.stretch() - 1.5).abs() < 1e-9 && job.start_frame(&clip) == 0.0);
        // A clip entirely before the change gets a single stretch, as before.
        clip.length_ticks = 3840;
        let constant = ProcessJob::for_clip(&clip, 7, &map, 48_000);
        assert_eq!(constant, None, "120 → 120: no processing needed");
        // A ramp: anchors inside it are no further apart than the step.
        let ramp = map
            .with_tempos(vec![
                crate::TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
                crate::TempoChange { tick: 3840, bpm: Bpm::new(60.0).unwrap(), ramp: true },
            ])
            .unwrap();
        let ramped = ProcessJob::for_clip(&clip, 7, &ramp, 48_000).unwrap();
        assert_eq!(ramped.curve().unwrap().len(), 3840 / 120 + 1);
    }

    #[test]
    fn jobs_compare_after_rounding() {
        assert_eq!(ProcessJob::new(1, 1.000_000_1, 0.001), ProcessJob::new(1, 1.0, 0.0));
        assert_ne!(ProcessJob::new(1, 1.001, 0.0), ProcessJob::new(1, 1.0, 0.0));
    }

    #[test]
    fn warped_clip_counts_frames_by_its_own_tempo() {
        let tempo = &TempoMap::constant(Bpm::new(120.0).unwrap(), crate::TimeSignature::FOUR_FOUR);
        // A beat at 60 BPM is one second: 960 ticks = 48 000 frames regardless of the project tempo.
        let warped = audio_clip(Some(Warp { source_bpm: 60.0 }), 0.0);
        assert!((warped.ticks_to_source_frames(960.0, tempo, 48_000) - 48_000.0).abs() < 1e-6);
        let plain = audio_clip(None, 0.0);
        assert!((plain.ticks_to_source_frames(960.0, tempo, 48_000) - 24_000.0).abs() < 1e-6);
        assert!((plain.source_frames_to_ticks(24_000.0, tempo, 48_000) - 960.0).abs() < 1e-6);
    }

    #[test]
    fn clip_edit_tells_null_warp_from_absent() {
        let absent: crate::ClipEdit = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.warp, None);
        let off: crate::ClipEdit = serde_json::from_str(r#"{"warp":null}"#).unwrap();
        assert_eq!(off.warp, Some(None));
        let guess: crate::ClipEdit = serde_json::from_str(r#"{"warp":{},"pitchSemitones":1.5}"#).unwrap();
        assert_eq!(guess.warp, Some(Some(WarpEdit { source_bpm: None })));
        assert_eq!(guess.pitch_semitones, Some(1.5));
        let set: crate::ClipEdit = serde_json::from_str(r#"{"warp":{"sourceBpm":128}}"#).unwrap();
        assert_eq!(set.warp, Some(Some(WarpEdit { source_bpm: Some(128.0) })));
    }
}
