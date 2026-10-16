//! Loudness of the export result: measured while writing (`OutputLevels`: BS.1770 integrated loudness and true peak,
//! the meters the analysis and the live master meter use too, `crate::loudness`) and the gain that reaches the target
//! (`gain_db`).
//!
//! Normalization is one constant gain for the whole file: loudness and true peak rise by exactly that much (the
//! filters are linear). If there is not enough headroom below the peak ceiling to reach the target, the gain is
//! reduced to the ceiling rather than limited: clipped peaks sound like distortion. The file is then quieter than the
//! target, which shows in `ExportReport::loudness_lufs`.

use daw_contracts::codes::export;
use daw_contracts::{ContractError, Normalize};

use crate::loudness::{Loudness, TruePeak, round_tenth};

/// Loudness (LUFS) and true peak (dBTP); `None` means silence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Levels {
    pub lufs: Option<f32>,
    pub peak_db: Option<f64>,
}

impl Levels {
    /// The peak for the report, to tenths like the loudness.
    pub(crate) fn peak_rounded(&self) -> Option<f32> {
        self.peak_db.map(round_tenth)
    }
}

pub(crate) struct OutputLevels {
    loudness: Loudness,
    peak: TruePeak,
}

impl OutputLevels {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self { loudness: Loudness::new(sample_rate), peak: TruePeak::new() }
    }

    pub(crate) fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(2) {
            self.loudness.push(frame[0], frame[1]);
            self.peak.push(frame[0], frame[1]);
        }
    }

    pub(crate) fn finish(&self) -> Levels {
        Levels { lufs: self.loudness.integrated(), peak_db: self.peak.peak_db() }
    }
}

/// The target and the ceiling are within their ranges (checked before rendering).
pub(crate) fn check(target: &Normalize) -> Result<(), ContractError> {
    let range = Normalize::LUFS_RANGE;
    if !range.contains(&target.lufs) {
        return Err(export::TARGET_LOUDNESS_OUT_OF_RANGE.with("lufs", target.lufs).with("min", range.start()).with("max", range.end()));
    }
    let range = Normalize::TRUE_PEAK_RANGE;
    if !range.contains(&target.ceiling_db()) {
        return Err(export::PEAK_CEILING_OUT_OF_RANGE.with("db", target.ceiling_db()).with("min", range.start()).with("max", range.end()));
    }
    Ok(())
}

/// Gain in dB: up to the loudness target, but not above the true peak ceiling.
pub(crate) fn gain_db(target: &Normalize, measured: Levels) -> Result<f64, ContractError> {
    let (Some(lufs), Some(peak)) = (measured.lufs, measured.peak_db) else {
        return Err(export::TOO_QUIET_TO_NORMALIZE.err());
    };
    let to_target = target.lufs - f64::from(lufs);
    let to_ceiling = target.ceiling_db() - peak;
    Ok(to_target.min(to_ceiling))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(lufs: f64, ceiling: Option<f64>) -> Normalize {
        Normalize { lufs, true_peak_db: ceiling }
    }

    fn measured(lufs: f32, peak_db: f64) -> Levels {
        Levels { lufs: Some(lufs), peak_db: Some(peak_db) }
    }

    #[test]
    fn gain_reaches_the_target_when_there_is_headroom() {
        assert!((gain_db(&target(-14.0, None), measured(-20.0, -8.0)).unwrap() - 6.0).abs() < 1e-9);
        assert!((gain_db(&target(-14.0, None), measured(-10.0, -2.0)).unwrap() + 4.0).abs() < 1e-9, "louder than the target gets quieter");
    }

    /// -9 LUFS with the peak 10 dB above the loudness hits the -1 dBTP ceiling: gain up to the ceiling (+9), not the
    /// target (+11).
    #[test]
    fn gain_stops_at_the_ceiling() {
        assert!((gain_db(&target(-9.0, None), measured(-20.0, -10.0)).unwrap() - 9.0).abs() < 1e-9);
        assert!((gain_db(&target(-9.0, Some(-3.0)), measured(-20.0, -10.0)).unwrap() - 7.0).abs() < 1e-9);
    }

    #[test]
    fn silence_and_targets_out_of_range_are_refused() {
        let silence = gain_db(&target(-14.0, None), Levels { lufs: None, peak_db: Some(-60.0) });
        assert!(matches!(silence, Err(e) if e.is(export::TOO_QUIET_TO_NORMALIZE)));
        assert!(check(&target(-14.0, None)).is_ok());
        assert!(matches!(check(&target(3.0, None)), Err(e) if e.is(export::TARGET_LOUDNESS_OUT_OF_RANGE)));
        assert!(matches!(check(&target(-14.0, Some(1.0))), Err(e) if e.is(export::PEAK_CEILING_OUT_OF_RANGE)));
    }

    #[test]
    fn output_levels_measure_a_tone() {
        let mut levels = OutputLevels::new(44_100);
        let amplitude = 10f32.powf(-20.0 / 20.0);
        let tone: Vec<f32> = (0..44_100 * 5)
            .flat_map(|i| {
                let v = amplitude * (std::f32::consts::TAU * 1000.0 * i as f32 / 44_100.0).sin();
                [v, v]
            })
            .collect();
        levels.push(&tone);
        let result = levels.finish();
        assert!((result.lufs.unwrap() + 20.0).abs() <= 0.1, "{result:?}");
        assert!((result.peak_db.unwrap() + 20.0).abs() <= 0.1, "{result:?}");
    }
}
