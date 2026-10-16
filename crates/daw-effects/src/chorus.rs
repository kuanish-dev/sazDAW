//! Chorus: two modulated delay lines with an LFO phase offset between the channels.
//! The graph in the interface draws the voices' sweep from the same LFO (`app/src/domain/chorusSweep.ts`): a change
//! here must be mirrored there.

use std::f32::consts::TAU;

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::{EffectKind, clamp_param, mix_over_dry};

const MAX_DELAY_MS: f32 = 50.0;

const PARAMS: [ParamInfo; 4] = [
    ParamInfo {
        name: "rate",
        label: L10n { en: "Rate, Hz", ru: "Скорость, Гц" },
        min: 0.05,
        max: 5.0,
        default: 0.8,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "depth",
        label: L10n { en: "Depth, ms", ru: "Глубина, мс" },
        min: 0.0,
        max: 10.0,
        default: 3.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "delay",
        label: L10n { en: "Delay, ms", ru: "Задержка, мс" },
        min: 5.0,
        max: 30.0,
        default: 12.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "mix",
        label: L10n { en: "Mix", ru: "Микс" },
        min: 0.0,
        max: 1.0,
        default: 0.4,
        kind: ParamKind::Number,
        choices: &[],
    },
];

pub struct Chorus {
    sample_rate: f32,
    lines: [Vec<f32>; 2],
    write: usize,
    phase: f32,
    rate: f32,
    depth_ms: f32,
    delay_ms: f32,
    mix: f32,
}

impl Chorus {
    fn resize(&mut self) {
        let len = (MAX_DELAY_MS * 0.001 * self.sample_rate) as usize + 4;
        self.lines = [vec![0.0; len], vec![0.0; len]];
        self.write = 0;
    }

    fn read(&self, channel: usize, delay_samples: f32) -> f32 {
        let line = &self.lines[channel];
        let len = line.len();
        let position = self.write as f32 - delay_samples;
        let position = position.rem_euclid(len as f32);
        let index = position as usize;
        let fraction = position - index as f32;
        let a = line[index % len];
        let b = line[(index + 1) % len];
        a + (b - a) * fraction
    }
}

impl EffectKind for Chorus {
    const KIND: &'static str = "chorus";
    const LABEL: L10n = L10n { en: "Chorus", ru: "Хорус" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut chorus = Self {
            sample_rate: config.sample_rate as f32,
            lines: [Vec::new(), Vec::new()],
            write: 0,
            phase: 0.0,
            rate: 0.0,
            depth_ms: 0.0,
            delay_ms: 0.0,
            mix: 0.0,
        };
        chorus.resize();
        chorus
    }
}

impl Effect for Chorus {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.resize();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "rate" => self.rate = value,
            "depth" => self.depth_ms = value,
            "delay" => self.delay_ms = value,
            "mix" => self.mix = value,
            _ => {}
        }
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.phase = 0.0;
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        let len = self.lines[0].len();
        let phase_step = TAU * self.rate / self.sample_rate;
        let ms_to_samples = self.sample_rate * 0.001;
        for frame in interleaved.chunks_exact_mut(2) {
            self.lines[0][self.write] = frame[0];
            self.lines[1][self.write] = frame[1];

            for channel in 0..2 {
                let lfo = (self.phase + channel as f32 * TAU / 4.0).sin();
                let delay = (self.delay_ms + self.depth_ms * lfo) * ms_to_samples;
                let wet = self.read(channel, delay.max(1.0));
                frame[channel] = mix_over_dry(frame[channel], wet, self.mix);
            }

            self.write = (self.write + 1) % len;
            self.phase = (self.phase + phase_step) % TAU;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{peak, rms, sine};

    #[test]
    fn modulates_and_stays_bounded() {
        let original = sine(440.0, 0.5, 48_000);
        let mut signal = original.clone();
        build::<Chorus>([("mix", 0.5)]).process(&mut signal, None);
        assert_ne!(signal, original);
        assert!(peak(&signal) <= 1.0);
        assert!(rms(&signal[4_800..]) > 0.2);
    }

    #[test]
    fn zero_mix_is_dry() {
        let original = sine(440.0, 0.5, 4_800);
        let mut signal = original.clone();
        build::<Chorus>([("mix", 0.0)]).process(&mut signal, None);
        assert_eq!(signal, original);
    }

    #[test]
    fn channels_differ_because_of_lfo_phase_offset() {
        let mut signal = sine(300.0, 0.5, 24_000);
        build::<Chorus>([("mix", 1.0), ("depth", 8.0)]).process(&mut signal, None);
        let differing = signal.chunks_exact(2).filter(|f| (f[0] - f[1]).abs() > 1e-3).count();
        assert!(differing > 1_000);
    }
}
