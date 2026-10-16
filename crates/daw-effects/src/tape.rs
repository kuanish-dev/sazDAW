//! Tape and console emulation: characterful saturation with an output trim. The curve is a soft knee — linear up to the
//! knee, then rounding off toward full scale — shifted a little off centre, so the two half-waves saturate differently
//! and even harmonics appear (the asymmetry of a real machine); it runs at twice the sample rate (`oversample.rs`) and
//! a DC blocker takes the offset away. `drive` pushes the sound into the knee and the curve is divided by it again, so
//! quiet sounds keep their level and only the peaks round off; `output` trims the result.
//! * **Tape:** an early knee and more asymmetry, then the head bump (a broad lift around 70 Hz) and a gentle roll-off of
//!   the top (a low-pass at 16 kHz with a soft corner).
//! * **Console:** a late knee and a trace of asymmetry (subtle drive), and crosstalk: each channel hears the other at
//!   −32 dB and gives up as much of itself, so a sound in the middle keeps its level and only the width narrows a hair.
//!
//! The mode crossfades: both voices run and the output glides from one to the other. No lookahead, no latency.
//! The graph in the interface draws the curve (`app/src/domain/shaperCurve.ts: tapeTransfer`); the head bump, the
//! roll-off and the crosstalk shape the sound over time and show in the live trace: a change here must be mirrored
//! there.

use daw_contracts::{Effect, L10n, ParamInfo, StreamConfig};

use crate::biquad::{Biquad, Coefficients, DcBlocker};
use crate::oversample::{Decimation, Oversampled};
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, choice, clamp_param, db_to_gain, number};

const MODES: [L10n; 2] = [L10n { en: "Tape", ru: "Лента" }, L10n { en: "Console", ru: "Пульт" }];

const PARAMS: [ParamInfo; 3] = [
    choice("mode", L10n { en: "Mode", ru: "Режим" }, &MODES, 0.0),
    number("drive", L10n { en: "Drive, dB", ru: "Драйв, дБ" }, 0.0, 24.0, 6.0),
    number("output", L10n { en: "Output, dB", ru: "Выход, дБ" }, -24.0, 6.0, 0.0),
];

/// What makes a mode sound like itself.
struct Character {
    /// Where the curve leaves the straight line, as a sample value.
    knee: f32,
    /// How far the curve is shifted off centre.
    bias: f32,
    /// The head bump: a peaking lift, dB, at `HEAD_BUMP_HZ`.
    head_bump_db: f32,
    /// The top's roll-off: a low-pass frequency, or none.
    roll_off_hz: Option<f32>,
    /// How much of the other channel each channel hears.
    crosstalk: f32,
}

const TAPE: Character = Character { knee: 0.4, bias: 0.08, head_bump_db: 2.5, roll_off_hz: Some(16_000.0), crosstalk: 0.0 };
/// −32 dB of the other channel.
const CONSOLE: Character = Character { knee: 0.75, bias: 0.02, head_bump_db: 0.0, roll_off_hz: None, crosstalk: 0.025 };

const HEAD_BUMP_HZ: f32 = 70.0;
const HEAD_BUMP_Q: f32 = 1.0;
/// The roll-off's corner: softer than Butterworth, so the top bends down gently.
const ROLL_OFF_Q: f32 = 0.6;
/// DC blocker cutoff, Hz: the asymmetric curve offsets the signal.
const DC_CUTOFF_HZ: f32 = 10.0;

/// The curve: linear up to `knee`, then rounding off toward ±1 with no corner (the slope is 1 at the knee).
fn soft_knee(x: f32, knee: f32) -> f32 {
    let size = x.abs();
    if size <= knee { x } else { x.signum() * (knee + (1.0 - knee) * ((size - knee) / (1.0 - knee)).tanh()) }
}

/// The curve of a character for a driven sample, off centre and back to 0 at 0.
fn shape(character: &Character, x: f32) -> f32 {
    soft_knee(x + character.bias, character.knee) - soft_knee(character.bias, character.knee)
}

/// One mode's sound for both channels.
struct Voice {
    character: &'static Character,
    head_bump: Coefficients,
    roll_off: Coefficients,
    oversampled: [Oversampled; 2],
    dc: [DcBlocker; 2],
    filters: [[Biquad; 2]; 2],
}

impl Voice {
    fn new(character: &'static Character, sample_rate: f32) -> Self {
        let mut voice = Self {
            character,
            head_bump: Coefficients::PASS,
            roll_off: Coefficients::PASS,
            oversampled: [Oversampled::default(); 2],
            dc: [DcBlocker::default(); 2],
            filters: [[Biquad::default(); 2]; 2],
        };
        voice.prepare(sample_rate);
        voice
    }

    fn prepare(&mut self, sample_rate: f32) {
        self.head_bump = Coefficients::peaking(sample_rate, HEAD_BUMP_HZ, self.character.head_bump_db, HEAD_BUMP_Q);
        self.roll_off = self.character.roll_off_hz.map_or(Coefficients::PASS, |hz| Coefficients::low_pass(sample_rate, hz, ROLL_OFF_Q));
        self.reset();
    }

    fn reset(&mut self) {
        self.oversampled = [Oversampled::default(); 2];
        self.dc = [DcBlocker::default(); 2];
        self.filters = [[Biquad::default(); 2]; 2];
    }

    /// One stereo frame at `drive` (a gain): curve, DC blocker, head bump and roll-off, crosstalk.
    fn process(&mut self, decimation: &Decimation, dc_pole: f32, drive: f32, frame: [f32; 2]) -> [f32; 2] {
        let character = self.character;
        let [left, right] = [0, 1].map(|c| {
            let shaped = self.oversampled[c].process(decimation, frame[c] * drive, |x| shape(character, x)) / drive;
            let flat = self.dc[c].process(dc_pole, shaped);
            let [bump, top] = &mut self.filters[c];
            top.process(&self.roll_off, bump.process(&self.head_bump, flat))
        });
        [left + character.crosstalk * (right - left), right + character.crosstalk * (left - right)]
    }
}

pub struct Tape {
    sample_rate: f32,
    smoothing: f32,
    decimation: Decimation,
    dc_pole: f32,
    drive: Smoothed,
    output: Smoothed,
    /// 1 tape, 0 console.
    tape: Smoothed,
    voices: [Voice; 2],
}

impl Tape {
    fn update(&mut self) {
        self.smoothing = smooth_coefficient(self.sample_rate);
        self.decimation = Decimation::new(self.sample_rate);
        self.dc_pole = DcBlocker::pole(self.sample_rate, DC_CUTOFF_HZ);
        for voice in &mut self.voices {
            voice.prepare(self.sample_rate);
        }
    }
}

impl EffectKind for Tape {
    const KIND: &'static str = "tape";
    const LABEL: L10n = L10n { en: "Tape / console", ru: "Лента / пульт" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let sample_rate = config.sample_rate as f32;
        let mut tape = Self {
            sample_rate,
            smoothing: 0.0,
            decimation: Decimation::new(sample_rate),
            dc_pole: 0.0,
            drive: Smoothed::new(1.0),
            output: Smoothed::new(1.0),
            tape: Smoothed::new(1.0),
            voices: [Voice::new(&TAPE, sample_rate), Voice::new(&CONSOLE, sample_rate)],
        };
        tape.update();
        tape
    }
}

impl Effect for Tape {
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
            "mode" => self.tape.set(if value >= 1.0 { 0.0 } else { 1.0 }),
            "drive" => self.drive.set(db_to_gain(value)),
            "output" => self.output.set(db_to_gain(value)),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.drive.snap();
        self.output.snap();
        self.tape.snap();
        for voice in &mut self.voices {
            voice.reset();
        }
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            let drive = self.drive.next(self.smoothing);
            let output = self.output.next(self.smoothing);
            let tape = self.tape.next(self.smoothing);
            let input = [frame[0], frame[1]];
            let [on_tape, on_console] = [0, 1].map(|v| self.voices[v].process(&self.decimation, self.dc_pole, drive, input));
            for c in 0..2 {
                frame[c] = (tape * on_tape[c] + (1.0 - tape) * on_console[c]) * output;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, peak, sine, tone};

    /// A sine of `hz` through the effect; the settled second half.
    fn render(set: &[(&str, f32)], hz: f32, amplitude: f32) -> Vec<f32> {
        let mut tape = build::<Tape>(set.iter().copied());
        let mut signal = sine(hz, amplitude, SR as usize);
        tape.process(&mut signal, None);
        signal.split_off(SR as usize)
    }

    fn db(gain: f32) -> f32 {
        20.0 * gain.log10()
    }

    #[test]
    fn a_quiet_sound_keeps_its_level_in_the_middle_of_the_band() {
        for mode in [0.0, 1.0] {
            let out = render(&[("mode", mode), ("drive", 18.0)], 1_000.0, 0.005);
            assert!(db(tone(&out, 1_000.0) / 0.005).abs() < 0.2, "mode {mode}: {} dB", db(tone(&out, 1_000.0) / 0.005));
        }
    }

    #[test]
    fn tape_lifts_the_head_bump_and_rolls_off_the_top() {
        let bump = render(&[("mode", 0.0)], 70.0, 0.005);
        assert!((db(tone(&bump, 70.0) / 0.005) - 2.5).abs() < 0.3, "{} dB", db(tone(&bump, 70.0) / 0.005));
        let top = render(&[("mode", 0.0)], 18_000.0, 0.005);
        assert!(db(tone(&top, 18_000.0) / 0.005) < -2.0, "{} dB", db(tone(&top, 18_000.0) / 0.005));
        let console = render(&[("mode", 1.0)], 70.0, 0.005);
        assert!(db(tone(&console, 70.0) / 0.005).abs() < 0.2, "the console has no head bump");
    }

    #[test]
    fn drive_rounds_the_peaks_off_with_odd_and_even_harmonics() {
        let out = render(&[("mode", 0.0), ("drive", 18.0)], 500.0, 0.5);
        let fundamental = tone(&out, 500.0);
        assert!(fundamental < 0.5 * 0.7, "the peaks are rounded off: {fundamental}");
        assert!(tone(&out, 1_500.0) > fundamental * 0.03, "3rd harmonic");
        assert!(tone(&out, 1_000.0) > fundamental * 0.005, "2nd harmonic from the asymmetry");
        assert!(tone(&out, 0.0).abs() < 0.01, "DC is removed");
    }

    #[test]
    fn the_console_lets_each_channel_hear_the_other() {
        let mut tape = build::<Tape>([("mode", 1.0), ("drive", 0.0)]);
        let mut signal = sine(1_000.0, 0.1, SR as usize);
        for frame in signal.chunks_exact_mut(2) {
            frame[1] = 0.0;
        }
        tape.process(&mut signal, None);
        let right: Vec<f32> = signal[SR as usize..].chunks_exact(2).flat_map(|f| [f[1], f[1]]).collect();
        assert!((db(tone(&right, 1_000.0) / 0.1) + 32.0).abs() < 0.5, "{} dB", db(tone(&right, 1_000.0) / 0.1));
    }

    #[test]
    fn output_stays_bounded_at_full_drive() {
        for mode in [0.0, 1.0] {
            let out = render(&[("mode", mode), ("drive", 24.0), ("output", 6.0)], 100.0, 1.0);
            assert!(out.iter().all(|v| v.is_finite()) && peak(&out) < 2.5, "mode {mode}: {}", peak(&out));
        }
    }

    #[test]
    fn switching_mode_drive_and_output_does_not_click() {
        let held = |set: &[(&str, f32)]| max_step(&render(set, 200.0, 0.5));
        let baseline =
            [held(&[("mode", 0.0), ("drive", 18.0)]), held(&[("mode", 1.0), ("drive", 0.0)]), held(&[("mode", 0.0), ("drive", 0.0)])]
                .into_iter()
                .fold(0.0, f32::max);
        let mut tape = build::<Tape>([]);
        let mut signal = sine(200.0, 0.5, SR as usize);
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            tape.set_param("mode", (block % 2) as f32);
            tape.set_param("drive", if block % 3 == 0 { 0.0 } else { 18.0 });
            tape.set_param("output", if block % 5 == 0 { 0.0 } else { -6.0 });
            tape.process(chunk, None);
        }
        assert!(max_step(&signal[SR as usize..]) < baseline * 1.5, "{} with {baseline}", max_step(&signal[SR as usize..]));
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut tape = build::<Tape>([("drive", 0.0)]);
        assert_eq!(tape.latency_samples(), 0);
        assert_latency_at_impulse(&mut tape);
    }
}
