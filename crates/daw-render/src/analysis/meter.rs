//! Measuring audio while it renders (pure math, no engine): peak and RMS of the whole range and of each section
//! (`Meter`) and the integrated loudness (the shared BS.1770 meter, `crate::loudness`). Audio arrives in chunks and
//! never piles up in memory.

use daw_contracts::{Level, SILENCE_DB};

use crate::loudness::{Loudness, round_tenth};

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
}
