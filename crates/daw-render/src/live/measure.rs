//! Turning the master's frames into the live readings: the shared BS.1770 meters (`crate::loudness`) plus what only a
//! live meter shows, the true peak and the phase correlation of the newest 400 ms (kept per 100 ms step, in step with
//! the loudness) and the newest frames for the vectorscope.

use std::collections::VecDeque;

use daw_contracts::{CLIP_DBTP, MasterReading, VECTOR_FRAMES};

use crate::loudness::{Loudness, TruePeak, decibels, round_tenth};

/// The newest 100 ms steps the true peak and the correlation are shown over: 400 ms, the momentary window.
const RECENT_STEPS: usize = 4;
/// Below this mean square (−90 dBFS RMS) a channel counts as silent for the correlation.
const SILENT_POWER: f64 = 1e-9;

/// The peak and the phase sums of one 100 ms step.
#[derive(Default, Clone, Copy)]
struct Step {
    /// True peak, linear.
    peak: f32,
    /// Sums of the left and right squares and of their products.
    left: f64,
    right: f64,
    product: f64,
    frames: u32,
}

impl Step {
    fn add(&mut self, left: f32, right: f32) {
        let (left, right) = (f64::from(left), f64::from(right));
        self.left += left * left;
        self.right += right * right;
        self.product += left * right;
        self.frames += 1;
    }
}

/// The measurement of one stream: its filters are made for the stream's sample rate.
pub(super) struct Measurement {
    sample_rate: u32,
    loudness: Loudness,
    peak: TruePeak,
    /// The step under way and the newest finished ones (at most `RECENT_STEPS`, oldest first).
    current: Step,
    recent: VecDeque<Step>,
    /// The highest true peak since the reset, linear.
    max_peak: f32,
    /// Frames measured since the reset.
    frames: u64,
    /// The readings as of the last finished step (without the vector): they change only when a step finishes, and the
    /// gating runs over the whole history, so they are computed then rather than on every read.
    shown: MasterReading,
    /// The newest frames for the vectorscope, interleaved, at most `VECTOR_FRAMES`.
    newest: VecDeque<f32>,
}

impl Measurement {
    pub(super) fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            loudness: Loudness::new(sample_rate),
            peak: TruePeak::new(),
            current: Step::default(),
            recent: VecDeque::with_capacity(RECENT_STEPS),
            max_peak: 0.0,
            frames: 0,
            shown: nothing_yet(sample_rate),
            newest: VecDeque::with_capacity(2 * VECTOR_FRAMES),
        }
    }

    pub(super) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Measures interleaved stereo frames.
    pub(super) fn push(&mut self, samples: &[f32]) {
        for frame in samples.chunks_exact(2) {
            let (left, right) = (frame[0], frame[1]);
            self.peak.push(left, right);
            self.current.add(left, right);
            self.frames += 1;
            if self.loudness.push(left, right) {
                self.finish_step();
            }
        }
        let keep = samples.len().min(2 * VECTOR_FRAMES);
        self.newest.extend(&samples[samples.len() - keep..]);
        let over = self.newest.len().saturating_sub(2 * VECTOR_FRAMES);
        self.newest.drain(..over);
    }

    /// Starts the integrated loudness, the range and the held peak over; the newest windows run on.
    pub(super) fn restart(&mut self) {
        self.loudness.restart();
        self.max_peak = 0.0;
        self.frames = 0;
        self.refresh();
    }

    pub(super) fn reading(&self) -> MasterReading {
        MasterReading { seconds: self.seconds(), vector: self.newest.iter().copied().collect(), ..self.shown.clone() }
    }

    fn seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate)
    }

    fn finish_step(&mut self) {
        let mut step = std::mem::take(&mut self.current);
        step.peak = self.peak.take();
        self.max_peak = self.max_peak.max(step.peak);
        if self.recent.len() == RECENT_STEPS {
            self.recent.pop_front();
        }
        self.recent.push_back(step);
        self.refresh();
    }

    fn refresh(&mut self) {
        let recent_peak = self.recent.iter().map(|step| step.peak).fold(0.0, f32::max);
        let max_true_peak_db = decibels(self.max_peak).map(round_tenth);
        self.shown = MasterReading {
            momentary_lufs: self.loudness.momentary(),
            short_term_lufs: self.loudness.short_term(),
            integrated_lufs: self.loudness.integrated(),
            range_lu: self.loudness.range(),
            true_peak_db: decibels(recent_peak).map(round_tenth),
            max_true_peak_db,
            clipped: max_true_peak_db.is_some_and(|db| db >= CLIP_DBTP),
            correlation: correlation(&self.recent),
            seconds: self.seconds(),
            vector: Vec::new(),
            sample_rate: self.sample_rate,
        };
    }
}

/// The readings before anything was measured.
pub(super) fn nothing_yet(sample_rate: u32) -> MasterReading {
    MasterReading {
        momentary_lufs: None,
        short_term_lufs: None,
        integrated_lufs: None,
        range_lu: None,
        true_peak_db: None,
        max_true_peak_db: None,
        clipped: false,
        correlation: None,
        seconds: 0.0,
        vector: Vec::new(),
        sample_rate,
    }
}

/// Phase correlation of the steps: 1 the same in both channels, −1 opposite. A channel alone (the other silent) reads 0:
/// nothing cancels in mono; both silent read nothing.
fn correlation(steps: &VecDeque<Step>) -> Option<f32> {
    let (mut left, mut right, mut product, mut frames) = (0.0, 0.0, 0.0, 0u32);
    for step in steps {
        left += step.left;
        right += step.right;
        product += step.product;
        frames += step.frames;
    }
    let silent = |squares: f64| frames == 0 || squares / f64::from(frames) < SILENT_POWER;
    match (silent(left), silent(right)) {
        (true, true) => None,
        (true, false) | (false, true) => Some(0.0),
        (false, false) => Some((product / (left * right).sqrt()).clamp(-1.0, 1.0) as f32),
    }
}
