//! Compressor with a soft knee and an external key signal (sidechain). The curve and the attack/release are the shared
//! dynamics blocks (`dynamics.rs`); the graph in the interface draws the same curve (`app/src/effects/compressorCurve.ts`).

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::dynamics::{Follower, GainComputer, peak_db};
use crate::{EffectKind, clamp_param, db_to_gain};

const PARAMS: [ParamInfo; 7] = [
    ParamInfo {
        name: "threshold",
        label: L10n { en: "Threshold, dB", ru: "Порог, дБ" },
        min: -60.0,
        max: 0.0,
        default: -18.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo { name: "ratio", label: L10n::same("Ratio"), min: 1.0, max: 20.0, default: 4.0, kind: ParamKind::Number, choices: &[] },
    ParamInfo {
        name: "attack",
        label: L10n { en: "Attack, ms", ru: "Attack, мс" },
        min: 0.1,
        max: 100.0,
        default: 10.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "release",
        label: L10n { en: "Release, ms", ru: "Release, мс" },
        min: 10.0,
        max: 1_000.0,
        default: 120.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "knee",
        label: L10n { en: "Knee, dB", ru: "Колено, дБ" },
        min: 0.0,
        max: 12.0,
        default: 6.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "makeup",
        label: L10n { en: "Makeup, dB", ru: "Makeup, дБ" },
        min: 0.0,
        max: 24.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "sidechain",
        label: L10n::same("Sidechain"),
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Switch,
        choices: &[],
    },
];

pub struct Compressor {
    sample_rate: f32,
    curve: GainComputer,
    attack_ms: f32,
    release_ms: f32,
    makeup: f32,
    use_sidechain: bool,
    /// Current gain reduction in dB (a positive number).
    reduction: Follower,
}

impl Compressor {
    fn update_times(&mut self) {
        self.reduction.set_times(self.attack_ms, self.release_ms, self.sample_rate);
    }
}

impl EffectKind for Compressor {
    const KIND: &'static str = "compressor";
    const LABEL: L10n = L10n { en: "Compressor", ru: "Компрессор" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut c = Self {
            sample_rate: config.sample_rate as f32,
            curve: GainComputer { threshold: 0.0, ratio: 1.0, knee: 0.0 },
            attack_ms: 0.0,
            release_ms: 0.0,
            makeup: 0.0,
            use_sidechain: false,
            reduction: Follower::default(),
        };
        c.update_times();
        c
    }
}

impl Effect for Compressor {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update_times();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "threshold" => self.curve.threshold = value,
            "ratio" => self.curve.ratio = value,
            "attack" => self.attack_ms = value,
            "release" => self.release_ms = value,
            "knee" => self.curve.knee = value,
            "makeup" => self.makeup = value,
            "sidechain" => self.use_sidechain = value >= 0.5,
            _ => {}
        }
        self.update_times();
    }

    fn reset(&mut self) {
        self.reduction.reset();
    }

    fn process(&mut self, interleaved: &mut [f32], sidechain: Option<&[f32]>) {
        // A key shorter than the buffer (should not happen): past its end the signal itself drives the compression, no panic.
        let mut key = sidechain.filter(|_| self.use_sidechain).map(|key| key.chunks_exact(2));
        let makeup = db_to_gain(self.makeup);
        for frame in interleaved.chunks_exact_mut(2) {
            let source = key.as_mut().and_then(Iterator::next).unwrap_or(&frame[..]);
            let reduction = self.reduction.follow(self.curve.reduction(peak_db(source[0], source[1])));
            let gain = db_to_gain(-reduction) * makeup;
            frame[0] *= gain;
            frame[1] *= gain;
        }
    }

    fn gain_reduction_db(&self) -> Option<f32> {
        Some(self.reduction.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{rms, sine};

    #[test]
    fn quiet_signal_passes_and_loud_signal_is_reduced() {
        let mut c = build::<Compressor>([("threshold", -20.0), ("ratio", 8.0), ("knee", 0.0)]);
        let mut quiet = sine(440.0, 0.02, 24_000);
        let before = rms(&quiet);
        c.process(&mut quiet, None);
        assert!((rms(&quiet[12_000..]) / before - 1.0).abs() < 0.05);

        let mut loud = sine(440.0, 0.9, 48_000);
        let before = rms(&loud[24_000..]);
        c.reset();
        c.process(&mut loud, None);
        let after = rms(&loud[24_000..]);
        // 0.9 ≈ -0.9 dBFS peak → ~21 dB over the threshold, at 8:1 the reduction is ≈ 18 dB.
        assert!(after < before * 0.3, "{before} → {after}");
    }

    #[test]
    fn sidechain_ducks_a_quiet_signal() {
        let mut c = build::<Compressor>([("sidechain", 1.0), ("threshold", -30.0), ("ratio", 10.0), ("knee", 0.0), ("attack", 0.1)]);
        let input = sine(220.0, 0.1, 24_000);
        let key = sine(60.0, 0.9, 24_000);
        let mut ducked = input.clone();
        c.process(&mut ducked, Some(&key));
        assert!(rms(&ducked[12_000..]) < rms(&input[12_000..]) * 0.2);

        // A quiet key leaves the signal alone.
        c.reset();
        let silent_key = vec![0.0; input.len()];
        let mut passed = input.clone();
        c.process(&mut passed, Some(&silent_key));
        assert!((rms(&passed) / rms(&input) - 1.0).abs() < 0.01);
    }

    #[test]
    fn a_key_shorter_than_the_buffer_does_not_panic() {
        let mut c = build::<Compressor>([("sidechain", 1.0), ("threshold", -30.0)]);
        let mut signal = sine(220.0, 0.1, 480);
        let key = sine(60.0, 0.9, 100);
        c.process(&mut signal, Some(&key));
        assert!(signal.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn reports_the_gain_reduction() {
        let mut c = build::<Compressor>([("threshold", -20.0), ("ratio", 8.0), ("knee", 0.0)]);
        assert_eq!(c.gain_reduction_db(), Some(0.0));
        let mut loud = sine(440.0, 0.9, 48_000);
        c.process(&mut loud, None);
        let reduction = c.gain_reduction_db().unwrap();
        assert!(reduction > 6.0 && reduction < 19.0, "up to ≈ 18 dB at the peaks: {reduction}");
    }

    #[test]
    fn makeup_raises_the_level() {
        let mut c = build::<Compressor>([("threshold", 0.0), ("makeup", 6.0)]);
        let mut signal = sine(440.0, 0.1, 4_800);
        let before = rms(&signal);
        c.process(&mut signal, None);
        assert!((rms(&signal) / before - 2.0).abs() < 0.05);
    }
}
