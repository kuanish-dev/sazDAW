//! Gate: mutes the signal while it (or the sidechain key) is below the threshold. It opens above the threshold and
//! closes `HYSTERESIS_DB` below it, only after the hold time (`hold`), so it does not chatter on a signal near the threshold.
//! The graph in the interface draws its transfer curve (threshold, hysteresis, depth) in `app/src/effects/gateCurve.ts`:
//! a change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::{EffectKind, clamp_param, db_to_gain, gain_to_reduction, time_coefficient};

/// How far below the threshold the signal must fall for the gate to close.
const HYSTERESIS_DB: f32 = 6.0;
/// Level detector decay: bridges a sine's zero crossings so the gate does not close within a period.
const DETECTOR_MS: f32 = 10.0;

const PARAMS: [ParamInfo; 6] = [
    ParamInfo {
        name: "threshold",
        label: L10n { en: "Threshold, dB", ru: "Порог, дБ" },
        min: -80.0,
        max: 0.0,
        default: -40.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "attack",
        label: L10n { en: "Attack, ms", ru: "Attack, мс" },
        min: 0.1,
        max: 50.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "hold",
        label: L10n { en: "Hold, ms", ru: "Удержание, мс" },
        min: 0.0,
        max: 500.0,
        default: 50.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "release",
        label: L10n { en: "Release, ms", ru: "Release, мс" },
        min: 5.0,
        max: 2_000.0,
        default: 150.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "range",
        label: L10n { en: "Depth, dB", ru: "Глубина, дБ" },
        min: -80.0,
        max: 0.0,
        default: -80.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "sidechain",
        label: L10n::same("Sidechain"),
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Switch,
        choices: &[],
        about: L10n::EMPTY,
    },
];

pub struct Gate {
    sample_rate: f32,
    open_level: f32,
    close_level: f32,
    attack_ms: f32,
    hold_ms: f32,
    release_ms: f32,
    closed_gain: f32,
    use_sidechain: bool,
    attack_coef: f32,
    release_coef: f32,
    detector_coef: f32,
    hold_frames: u32,
    /// Detector level (instant attack, `DETECTOR_MS` decay).
    level: f32,
    open: bool,
    /// How many more frames to keep the gate open after the level fell below the closing threshold.
    hold_left: u32,
    gain: f32,
}

impl Gate {
    fn update_times(&mut self) {
        self.attack_coef = time_coefficient(self.attack_ms, self.sample_rate);
        self.release_coef = time_coefficient(self.release_ms, self.sample_rate);
        self.detector_coef = time_coefficient(DETECTOR_MS, self.sample_rate);
        self.hold_frames = (self.hold_ms * 0.001 * self.sample_rate) as u32;
    }

    /// Whether the gate is open after a frame with key signal level `level`.
    fn decide(&mut self, level: f32) -> bool {
        self.level = level.max(self.level * self.detector_coef);
        if self.level >= self.open_level {
            self.open = true;
            self.hold_left = self.hold_frames;
        } else if self.open && self.level >= self.close_level {
            self.hold_left = self.hold_frames;
        } else if self.open && self.hold_left > 0 {
            self.hold_left -= 1;
        } else {
            self.open = false;
        }
        self.open
    }
}

impl EffectKind for Gate {
    const KIND: &'static str = "gate";
    const LABEL: L10n = L10n { en: "Gate", ru: "Гейт" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut gate = Self {
            sample_rate: config.sample_rate as f32,
            open_level: 0.0,
            close_level: 0.0,
            attack_ms: 0.0,
            hold_ms: 0.0,
            release_ms: 0.0,
            closed_gain: 0.0,
            use_sidechain: false,
            attack_coef: 0.0,
            release_coef: 0.0,
            detector_coef: 0.0,
            hold_frames: 0,
            level: 0.0,
            open: false,
            hold_left: 0,
            gain: 0.0,
        };
        gate.update_times();
        gate
    }
}

impl Effect for Gate {
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
            "threshold" => {
                self.open_level = db_to_gain(value);
                self.close_level = db_to_gain(value - HYSTERESIS_DB);
            }
            "attack" => self.attack_ms = value,
            "hold" => self.hold_ms = value,
            "release" => self.release_ms = value,
            "range" => self.closed_gain = db_to_gain(value),
            "sidechain" => self.use_sidechain = value >= 0.5,
            _ => {}
        }
        self.update_times();
    }

    fn reset(&mut self) {
        self.level = 0.0;
        self.open = false;
        self.hold_left = 0;
        self.gain = self.closed_gain;
    }

    fn process(&mut self, interleaved: &mut [f32], sidechain: Option<&[f32]>) {
        let mut key = sidechain.filter(|_| self.use_sidechain).map(|key| key.chunks_exact(2));
        for frame in interleaved.chunks_exact_mut(2) {
            let source = key.as_mut().and_then(Iterator::next).unwrap_or(&frame[..]);
            let open = self.decide(source[0].abs().max(source[1].abs()));
            let (target, coef) = if open { (1.0, self.attack_coef) } else { (self.closed_gain, self.release_coef) };
            self.gain = target + coef * (self.gain - target);
            frame[0] *= self.gain;
            frame[1] *= self.gain;
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
    use crate::testing::{SR, max_step, rms, sine};

    const MS: usize = SR as usize / 1_000;

    /// Consecutive pieces of a 200 Hz sine at different levels.
    fn segments(parts: &[(f32, usize)]) -> Vec<f32> {
        parts.iter().flat_map(|&(amplitude, ms)| sine(200.0, amplitude, ms * MS)).collect()
    }

    fn window(buffer: &[f32], from_ms: usize, to_ms: usize) -> &[f32] {
        &buffer[from_ms * MS * 2..to_ms * MS * 2]
    }

    fn gate() -> Gate {
        build::<Gate>([("threshold", -30.0), ("attack", 0.5), ("hold", 50.0), ("release", 10.0), ("range", -60.0)])
    }

    #[test]
    fn opens_on_loud_signal_and_closes_after_hold_and_release() {
        let input = segments(&[(0.5, 300), (0.01, 500)]);
        let mut output = input.clone();
        gate().process(&mut output, None);
        let ratio = |from, to| rms(window(&output, from, to)) / rms(window(&input, from, to));
        assert!((ratio(10, 300) - 1.0).abs() < 0.01, "open: {}", ratio(10, 300));
        assert!((ratio(305, 330) - 1.0).abs() < 0.01, "hold: {}", ratio(305, 330));
        assert!(ratio(500, 800) < 0.002, "closed at −60 dB: {}", ratio(500, 800));
    }

    #[test]
    fn reports_the_closed_depth_as_gain_reduction() {
        let mut g = gate();
        let mut loud = segments(&[(0.5, 100)]);
        g.process(&mut loud, None);
        assert!(g.gain_reduction_db().unwrap() < 0.1, "open");
        let mut quiet = segments(&[(0.0, 500)]);
        g.process(&mut quiet, None);
        assert!((g.gain_reduction_db().unwrap() - 60.0).abs() < 0.5, "closed at −60 dB");
    }

    #[test]
    fn opens_within_the_attack_time() {
        let input = segments(&[(0.0, 100), (0.5, 100)]);
        let mut output = input.clone();
        gate().process(&mut output, None);
        assert_eq!(rms(window(&output, 0, 100)), 0.0);
        assert!(rms(window(&output, 105, 200)) / rms(window(&input, 105, 200)) > 0.99);
    }

    /// A level of −33 dB lies between the opening (−30) and closing (−36) thresholds: a closed gate stays closed, an open one open.
    #[test]
    fn hysteresis_keeps_the_current_state_between_thresholds() {
        let between = 10f32.powf(-33.0 / 20.0);
        let input = segments(&[(between, 400)]);
        let mut closed = input.clone();
        gate().process(&mut closed, None);
        assert!(rms(window(&closed, 100, 400)) / rms(window(&input, 100, 400)) < 0.002);

        let input = segments(&[(0.5, 100), (between, 400)]);
        let mut opened = input.clone();
        gate().process(&mut opened, None);
        assert!((rms(window(&opened, 200, 500)) / rms(window(&input, 200, 500)) - 1.0).abs() < 0.01);
    }

    #[test]
    fn sidechain_key_opens_the_gate() {
        let mut g = build::<Gate>([("threshold", -30.0), ("sidechain", 1.0)]);
        let input = sine(220.0, 0.01, 24_000);
        let key = sine(60.0, 0.9, 24_000);
        let mut output = input.clone();
        g.process(&mut output, Some(&key));
        assert!((rms(&output[4_800..]) / rms(&input[4_800..]) - 1.0).abs() < 0.01);

        let mut alone = input.clone();
        build::<Gate>([("threshold", -30.0), ("sidechain", 1.0)]).process(&mut alone, None);
        assert!(rms(&alone[4_800..]) < rms(&input[4_800..]) * 0.01);
    }

    #[test]
    fn range_zero_changes_nothing() {
        let original = segments(&[(0.001, 100)]);
        let mut signal = original.clone();
        build::<Gate>([("range", 0.0)]).process(&mut signal, None);
        for (a, b) in signal.iter().zip(&original) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    /// Closing and opening are smooth: the step between frames is no larger than the sine's own.
    #[test]
    fn opening_and_closing_do_not_click() {
        let input = segments(&[(0.0, 50), (0.5, 200), (0.005, 300)]);
        let mut output = input.clone();
        build::<Gate>([("threshold", -30.0), ("attack", 2.0), ("release", 20.0)]).process(&mut output, None);
        assert!(max_step(&output) < max_step(&input) * 1.2, "{} with {}", max_step(&output), max_step(&input));
    }
}
