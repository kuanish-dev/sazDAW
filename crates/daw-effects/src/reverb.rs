//! Reverb after the Freeverb design: parallel comb filters and serial allpasses. Before them the (mono) input waits
//! `predelay` ms — the gap between a hit and its room that keeps the hit clear — and passes a low cut, so a bass or a
//! kick does not muddy the tail (at 20 Hz it is off).
//! The graph in the interface works out the tail's decay from the same tuning, feedback and damping
//! (`app/src/effects/reverbTail.ts`): a change here must be mirrored there. The pre-delay and the low cut do not change
//! how fast the tail dies, so the graph leaves them out.

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::biquad::{BUTTERWORTH_Q, Biquad, Coefficients};
use crate::{EffectKind, clamp_param, number};

// Settings of the classic Freeverb (Jezar): line lengths in frames at 44.1 kHz and knob scales.
const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const REFERENCE_RATE: f32 = 44_100.0;
/// Comb filter feedback: `ROOM_OFFSET + room × ROOM_SCALE` (never reaches 1, so the tail always decays).
const ROOM_OFFSET: f32 = 0.7;
const ROOM_SCALE: f32 = 0.28;
const DAMP_SCALE: f32 = 0.4;
/// The input is attenuated before the eight parallel comb filters, and the output is raised back.
const INPUT_GAIN: f32 = 0.015;
const OUTPUT_GAIN: f32 = 3.0;
const ALLPASS_FEEDBACK: f32 = 0.5;
const MAX_PREDELAY_MS: f32 = 250.0;
/// The input low cut at this frequency is off.
const LOW_CUT_OFF_HZ: f32 = 20.0;

const PARAMS: [ParamInfo; 6] = [
    ParamInfo {
        name: "room",
        label: L10n { en: "Room size", ru: "Комната" },
        min: 0.0,
        max: 1.0,
        default: 0.6,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "damp",
        label: L10n { en: "Damping", ru: "Демпфирование" },
        min: 0.0,
        max: 1.0,
        default: 0.5,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "width",
        label: L10n { en: "Width", ru: "Ширина" },
        min: 0.0,
        max: 1.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "mix",
        label: L10n { en: "Mix", ru: "Микс" },
        min: 0.0,
        max: 1.0,
        default: 0.25,
        kind: ParamKind::Number,
        choices: &[],
    },
    number("predelay", L10n { en: "Pre-delay, ms", ru: "Предзадержка, мс" }, 0.0, MAX_PREDELAY_MS, 0.0),
    number("low_cut", L10n { en: "Low cut, Hz", ru: "Срез низа, Гц" }, LOW_CUT_OFF_HZ, 1_000.0, LOW_CUT_OFF_HZ),
];

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    filter: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self { buffer: vec![0.0; len.max(1)], index: 0, filter: 0.0 }
    }

    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter = output * (1.0 - damp) + self.filter * damp;
        self.buffer[self.index] = input + self.filter * feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.filter = 0.0;
    }
}

struct Allpass {
    buffer: Vec<f32>,
    index: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buffer: vec![0.0; len.max(1)], index: 0 }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = buffered - input;
        self.buffer[self.index] = input + buffered * ALLPASS_FEEDBACK;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

struct Channel {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl Channel {
    fn new(scale: f32, spread: usize) -> Self {
        let size = |n: usize| ((n + spread) as f32 * scale) as usize;
        Self {
            combs: COMB_TUNING.iter().map(|n| Comb::new(size(*n))).collect(),
            allpasses: ALLPASS_TUNING.iter().map(|n| Allpass::new(size(*n))).collect(),
        }
    }

    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let mut sum = 0.0;
        for comb in &mut self.combs {
            sum += comb.process(input, feedback, damp);
        }
        for allpass in &mut self.allpasses {
            sum = allpass.process(sum);
        }
        sum
    }

    fn clear(&mut self) {
        self.combs.iter_mut().for_each(Comb::clear);
        self.allpasses.iter_mut().for_each(Allpass::clear);
    }
}

pub struct Reverb {
    sample_rate: f32,
    channels: [Channel; 2],
    room: f32,
    damp: f32,
    width: f32,
    mix: f32,
    /// The input's wait before the room: a ring of `MAX_PREDELAY_MS`, read `predelay` frames back.
    predelay_line: Vec<f32>,
    predelay_write: usize,
    predelay: usize,
    predelay_ms: f32,
    low_cut_hz: f32,
    low_cut: Coefficients,
    low_cut_state: Biquad,
}

impl Reverb {
    fn build(sample_rate: f32) -> [Channel; 2] {
        let scale = sample_rate / REFERENCE_RATE;
        [Channel::new(scale, 0), Channel::new(scale, STEREO_SPREAD)]
    }

    /// The pre-delay ring for the rate (control side: it allocates).
    fn predelay_line(sample_rate: f32) -> Vec<f32> {
        vec![0.0; (MAX_PREDELAY_MS * 0.001 * sample_rate) as usize + 1]
    }

    /// Frames of pre-delay and the low cut's filter for the current settings and rate.
    fn update(&mut self) {
        self.predelay = ((self.predelay_ms * 0.001 * self.sample_rate) as usize).min(self.predelay_line.len() - 1);
        self.low_cut = if self.low_cut_hz <= LOW_CUT_OFF_HZ {
            Coefficients::PASS
        } else {
            Coefficients::high_pass(self.sample_rate, self.low_cut_hz, BUTTERWORTH_Q)
        };
    }

    /// The input after its pre-delay and low cut.
    fn delayed(&mut self, input: f32) -> f32 {
        let len = self.predelay_line.len();
        self.predelay_line[self.predelay_write] = input;
        let out = self.predelay_line[(self.predelay_write + len - self.predelay) % len];
        self.predelay_write = (self.predelay_write + 1) % len;
        self.low_cut_state.process(&self.low_cut, out)
    }
}

impl EffectKind for Reverb {
    const KIND: &'static str = "reverb";
    const LABEL: L10n = L10n { en: "Reverb", ru: "Реверб" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let sample_rate = config.sample_rate as f32;
        let mut reverb = Self {
            sample_rate,
            channels: Self::build(sample_rate),
            room: 0.0,
            damp: 0.0,
            width: 0.0,
            mix: 0.0,
            predelay_line: Self::predelay_line(sample_rate),
            predelay_write: 0,
            predelay: 0,
            predelay_ms: 0.0,
            low_cut_hz: LOW_CUT_OFF_HZ,
            low_cut: Coefficients::PASS,
            low_cut_state: Biquad::default(),
        };
        reverb.update();
        reverb
    }
}

impl Effect for Reverb {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.channels = Self::build(self.sample_rate);
        self.predelay_line = Self::predelay_line(self.sample_rate);
        self.predelay_write = 0;
        self.low_cut_state = Biquad::default();
        self.update();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "room" => self.room = value,
            "damp" => self.damp = value,
            "width" => self.width = value,
            "mix" => self.mix = value,
            "predelay" => self.predelay_ms = value,
            "low_cut" => self.low_cut_hz = value,
            _ => {}
        }
        self.update();
    }

    fn reset(&mut self) {
        self.channels.iter_mut().for_each(Channel::clear);
        self.predelay_line.fill(0.0);
        self.low_cut_state = Biquad::default();
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        let feedback = ROOM_OFFSET + self.room * ROOM_SCALE;
        let damp = self.damp * DAMP_SCALE;
        let wet_main = self.mix * (self.width / 2.0 + 0.5);
        let wet_cross = self.mix * ((1.0 - self.width) / 2.0);
        for frame in interleaved.chunks_exact_mut(2) {
            let input = self.delayed((frame[0] + frame[1]) * INPUT_GAIN);
            let left = self.channels[0].process(input, feedback, damp) * OUTPUT_GAIN;
            let right = self.channels[1].process(input, feedback, damp) * OUTPUT_GAIN;
            frame[0] = frame[0] * (1.0 - self.mix) + left * wet_main + right * wet_cross;
            frame[1] = frame[1] * (1.0 - self.mix) + right * wet_main + left * wet_cross;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{impulse, peak, rms};

    #[test]
    fn impulse_produces_a_decaying_tail() {
        let mut r = build::<Reverb>([("mix", 1.0), ("room", 0.8)]);
        let mut buffer = impulse(96_000);
        r.process(&mut buffer, None);
        let early = rms(&buffer[2_000 * 2..12_000 * 2]);
        let late = rms(&buffer[80_000 * 2..]);
        assert!(early > 1e-3, "the tail must be audible");
        assert!(late < early, "the tail decays");
        assert!(buffer.iter().all(|v| v.is_finite()));
        assert!(peak(&buffer) < 2.0);
    }

    #[test]
    fn bigger_room_rings_longer() {
        let tail = |room: f32| {
            let mut r = build::<Reverb>([("mix", 1.0), ("room", room)]);
            let mut buffer = impulse(96_000);
            r.process(&mut buffer, None);
            rms(&buffer[60_000 * 2..])
        };
        assert!(tail(0.95) > tail(0.2) * 2.0);
    }

    #[test]
    fn zero_mix_is_dry() {
        let mut r = build::<Reverb>([("mix", 0.0)]);
        let original = impulse(4_000);
        let mut buffer = original.clone();
        r.process(&mut buffer, None);
        assert_eq!(buffer, original);
    }

    #[test]
    fn pre_delay_holds_the_room_back() {
        let early = |predelay: f32| {
            let mut r = build::<Reverb>([("mix", 1.0), ("predelay", predelay)]);
            let mut buffer = impulse(24_000);
            r.process(&mut buffer, None);
            // 30–90 ms: the combs answer after about 25 ms without a pre-delay.
            rms(&buffer[1_440 * 2..4_320 * 2])
        };
        assert!(early(0.0) > 1e-3);
        assert!(early(100.0) < 1e-6, "a 100 ms pre-delay keeps the first 125 ms silent");
    }

    #[test]
    fn low_cut_keeps_the_lows_out_of_the_tail() {
        let tail = |low_cut: f32| {
            let mut r = build::<Reverb>([("mix", 1.0), ("low_cut", low_cut)]);
            let mut buffer = crate::testing::sine(60.0, 0.5, 48_000);
            r.process(&mut buffer, None);
            rms(&buffer[24_000 * 2..])
        };
        assert!(tail(500.0) < tail(20.0) * 0.1, "{} vs {}", tail(500.0), tail(20.0));
    }

    #[test]
    fn zero_width_gives_identical_channels() {
        let mut r = build::<Reverb>([("mix", 1.0), ("width", 0.0)]);
        let mut buffer = impulse(20_000);
        r.process(&mut buffer, None);
        assert!(buffer.chunks_exact(2).all(|f| (f[0] - f[1]).abs() < 1e-6));
    }
}
