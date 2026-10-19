//! Zero-latency limiter: instant attack, smooth release, hard ceiling.
//! The graph in the interface draws its transfer curve (input gain, ceiling) in `app/src/effects/limiterCurve.ts`: a
//! change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::{EffectKind, clamp_param, db_to_gain, gain_to_reduction, time_coefficient};

const PARAMS: [ParamInfo; 3] = [
    ParamInfo {
        name: "ceiling",
        label: L10n { en: "Ceiling, dB", ru: "Потолок, дБ" },
        min: -12.0,
        max: 0.0,
        default: -0.3,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "release",
        label: L10n { en: "Release, ms", ru: "Release, мс" },
        min: 10.0,
        max: 500.0,
        default: 80.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "input",
        label: L10n { en: "Input, dB", ru: "Вход, дБ" },
        min: 0.0,
        max: 24.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
];

pub struct Limiter {
    sample_rate: f32,
    ceiling: f32,
    release_ms: f32,
    input_gain: f32,
    release_coef: f32,
    gain: f32,
}

impl Limiter {
    fn update(&mut self) {
        self.release_coef = time_coefficient(self.release_ms, self.sample_rate);
    }
}

impl EffectKind for Limiter {
    const KIND: &'static str = "limiter";
    const LABEL: L10n = L10n { en: "Limiter", ru: "Лимитер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut limiter =
            Self { sample_rate: config.sample_rate as f32, ceiling: 1.0, release_ms: 0.0, input_gain: 1.0, release_coef: 0.0, gain: 1.0 };
        limiter.update();
        limiter
    }
}

impl Effect for Limiter {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "ceiling" => self.ceiling = db_to_gain(value),
            "release" => self.release_ms = value,
            "input" => self.input_gain = db_to_gain(value),
            _ => {}
        }
        self.update();
    }

    fn reset(&mut self) {
        self.gain = 1.0;
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            let left = frame[0] * self.input_gain;
            let right = frame[1] * self.input_gain;
            let peak = left.abs().max(right.abs());
            let needed = if peak > self.ceiling { self.ceiling / peak } else { 1.0 };
            // Instant attack, the return to 1.0 follows the release.
            self.gain = if needed < self.gain { needed } else { 1.0 + self.release_coef * (self.gain - 1.0) };
            let gain = self.gain.min(needed);
            frame[0] = (left * gain).clamp(-self.ceiling, self.ceiling);
            frame[1] = (right * gain).clamp(-self.ceiling, self.ceiling);
        }
    }

    fn gain_reduction_db(&self) -> Option<f32> {
        Some(gain_to_reduction(self.gain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{peak, sine};

    #[test]
    fn never_exceeds_the_ceiling() {
        let mut limiter = build::<Limiter>([("ceiling", -6.0), ("input", 20.0)]);
        let mut signal = sine(300.0, 0.9, 48_000);
        limiter.process(&mut signal, None);
        assert!(peak(&signal) <= db_to_gain(-6.0) + 1e-6, "peak = {}", peak(&signal));
        assert!(peak(&signal) > 0.4, "the sound must not vanish");
    }

    #[test]
    fn reports_the_gain_reduction() {
        let mut limiter = build::<Limiter>([("ceiling", -6.0)]);
        let mut quiet = sine(300.0, 0.1, 4_800);
        limiter.process(&mut quiet, None);
        assert_eq!(limiter.gain_reduction_db(), Some(0.0));
        // A constant level of 1.0 against a -6 dB ceiling: 6 dB of reduction.
        let mut loud = vec![1.0; 4_800];
        limiter.process(&mut loud, None);
        assert!((limiter.gain_reduction_db().unwrap() - 6.0).abs() < 0.01);
    }

    #[test]
    fn leaves_quiet_signals_alone() {
        let mut limiter = build::<Limiter>([]);
        let original = sine(300.0, 0.1, 4_800);
        let mut signal = original.clone();
        limiter.process(&mut signal, None);
        assert_eq!(signal, original);
    }
}
