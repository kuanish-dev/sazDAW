//! Measuring audio while it renders (pure math, no engine): peak and RMS of the whole range and of each section
//! with the integrated loudness (`Meter`, for `levels`), and the loudness of the whole range for balancing
//! (`LoudnessMeter`, for `balance`), both on the shared BS.1770 meters (`crate::loudness`). Audio arrives in chunks
//! and never piles up in memory.

use daw_contracts::{Level, LoudnessLevels, SILENCE_DB};

use crate::loudness::{Loudness, TruePeak, round_tenth};

/// Sample rate of the project level analysis.
pub(crate) const ANALYSIS_SAMPLE_RATE: u32 = 48_000;

/// Peak and sum of squares of a piece of audio.
#[derive(Default, Clone, Copy)]
struct Sum {
    peak: f32,
    squares: f64,
    samples: u64,
}

impl Sum {
    fn add(&mut self, sample: f32) {
        self.peak = self.peak.max(sample.abs());
        self.squares += f64::from(sample) * f64::from(sample);
        self.samples += 1;
    }

    /// Level in dBFS rounded to tenths; `None` when quieter than [`SILENCE_DB`].
    fn level(&self) -> Option<Level> {
        let peak_db = 20.0 * self.peak.log10();
        if self.samples == 0 || peak_db.is_nan() || peak_db < SILENCE_DB {
            return None;
        }
        let rms_db = 10.0 * (self.squares / self.samples as f64).log10();
        Some(Level { peak_db: round_tenth(f64::from(peak_db)), rms_db: round_tenth(rms_db) })
    }
}

/// What was measured over the range.
pub(crate) struct Measured {
    pub level: Option<Level>,
    pub loudness_lufs: Option<f32>,
    pub sections: Vec<Option<Level>>,
}

/// A stereo meter by sections. `ends` are the end frames of each section from the range start, ascending; audio past
/// the last end is ignored.
pub(crate) struct Meter {
    ends: Vec<u64>,
    frame: u64,
    total: Sum,
    current: Sum,
    sections: Vec<Option<Level>>,
    loudness: Loudness,
}

impl Meter {
    pub(crate) fn new(ends: Vec<u64>) -> Self {
        Self {
            ends,
            frame: 0,
            total: Sum::default(),
            current: Sum::default(),
            sections: Vec::new(),
            loudness: Loudness::new(ANALYSIS_SAMPLE_RATE),
        }
    }

    /// Takes interleaved stereo frames.
    pub(crate) fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(2) {
            let Some(&end) = self.ends.get(self.sections.len()) else { return };
            for &sample in frame {
                self.total.add(sample);
                self.current.add(sample);
            }
            self.loudness.push(frame[0], frame[1]);
            self.frame += 1;
            if self.frame == end {
                self.sections.push(self.current.level());
                self.current = Sum::default();
            }
        }
    }

    /// The result; sections the audio never reached are silent.
    pub(crate) fn finish(mut self) -> Measured {
        self.sections.resize(self.ends.len(), None);
        Measured { level: self.total.level(), loudness_lufs: self.loudness.integrated(), sections: self.sections }
    }
}

/// The integrated loudness, the loudest short-term loudness and the true peak of the whole range.
pub(crate) struct LoudnessMeter {
    loudness: Loudness,
    peak: TruePeak,
    max_short_term: Option<f32>,
}

impl LoudnessMeter {
    pub(crate) fn new() -> Self {
        Self { loudness: Loudness::new(ANALYSIS_SAMPLE_RATE), peak: TruePeak::new(), max_short_term: None }
    }

    /// Takes interleaved stereo frames.
    pub(crate) fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(2) {
            self.peak.push(frame[0], frame[1]);
            // The short-term loudness changes only when a 100 ms step ends.
            if self.loudness.push(frame[0], frame[1])
                && let Some(short_term) = self.loudness.short_term()
            {
                self.max_short_term = Some(self.max_short_term.map_or(short_term, |max| max.max(short_term)));
            }
        }
    }

    pub(crate) fn finish(&self) -> LoudnessLevels {
        LoudnessLevels {
            loudness_lufs: self.loudness.integrated(),
            max_short_term_lufs: self.max_short_term,
            true_peak_db: self.peak.peak_db().map(round_tenth),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loudness::tests::sine;

    #[test]
    fn meter_gives_peak_and_rms_per_section_and_null_for_silence() {
        // Section 1 is a -6 dBFS sine (RMS 3 dB below the peak), section 2 silence, section 3 a constant 0.5.
        let mut audio = sine(1000.0, -6.0, 1.0, true, true);
        audio.extend(vec![0.0; 48_000 * 2]);
        audio.extend(vec![0.5; 48_000 * 2]);
        let mut meter = Meter::new(vec![48_000, 96_000, 144_000, 192_000]);
        for chunk in audio.chunks(4096 * 2 + 2) {
            meter.push(chunk);
        }
        let measured = meter.finish();
        let first = measured.sections[0].unwrap();
        assert_eq!((first.peak_db, first.rms_db), (-6.0, -9.0));
        assert_eq!(measured.sections[1], None);
        assert_eq!(measured.sections[2], Some(Level { peak_db: -6.0, rms_db: -6.0 }));
        assert_eq!(measured.sections[3], None, "the audio never reached the fourth section");
        assert_eq!(measured.level.unwrap().peak_db, -6.0);
        assert_eq!(measured.level.unwrap().rms_db, -9.0, "mean power of three sections: (0.126 + 0 + 0.25) / 3");
    }

    #[test]
    fn loudness_meter_holds_the_loudest_three_seconds_and_the_true_peak() {
        // 4 s of a quiet sine, then 4 s of a sine 20 dB louder: the loudest 3 s are the loud part only, the integrated
        // loudness gates the quiet part out (more than 10 LU below; the blocks across the change pull it down a little),
        // the true peak is the loud sine's.
        let mut audio = sine(1000.0, -40.0, 4.0, true, true);
        audio.extend(sine(1000.0, -20.0, 4.0, true, true));
        let mut meter = LoudnessMeter::new();
        for chunk in audio.chunks(4096 * 2 + 2) {
            meter.push(chunk);
        }
        let levels = meter.finish();
        let mut alone = LoudnessMeter::new();
        alone.push(&sine(1000.0, -20.0, 4.0, true, true));
        let loud = alone.finish().loudness_lufs.unwrap();
        let near = |value: Option<f32>, tolerance: f32| value.is_some_and(|v| (v - loud).abs() <= tolerance);
        assert!(near(levels.max_short_term_lufs, 0.1), "the loud part: {levels:?}, alone {loud}");
        assert!(near(levels.loudness_lufs, 0.3), "the quiet part is gated out: {levels:?}, alone {loud}");
        assert!(levels.true_peak_db.is_some_and(|peak| (peak + 20.0).abs() <= 0.2), "{levels:?}");
    }

    #[test]
    fn loudness_meter_reads_silence_and_short_ranges_as_none() {
        let mut silent = LoudnessMeter::new();
        silent.push(&vec![0.0; 48_000 * 2 * 4]);
        assert_eq!(silent.finish(), LoudnessLevels { loudness_lufs: None, max_short_term_lufs: None, true_peak_db: None });
        let mut short = LoudnessMeter::new();
        short.push(&sine(1000.0, -20.0, 1.0, true, true));
        let levels = short.finish();
        assert!(levels.loudness_lufs.is_some() && levels.true_peak_db.is_some(), "{levels:?}");
        assert_eq!(levels.max_short_term_lufs, None, "a second is shorter than the 3 s window");
    }
}
