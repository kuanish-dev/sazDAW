//! Linear-phase EQ, for mastering: the bands and params of the EQ (`eq.rs`: low cut, low shelf, mid peak, high shelf,
//! high cut) with their magnitude only, as a linear-phase FIR — every frequency is delayed alike, so the EQ changes the
//! tone without smearing transients or turning the phase between bands. It works in three jobs:
//! * `design.rs` turns the settings into the kernel (on the designer thread, or in `start`/`prepare` on the control
//!   side);
//! * `worker.rs` runs the designer thread and hands kernels to the audio side without locks;
//! * `convolver.rs` convolves the sound with the kernel by FFT in blocks, crossfading to a new kernel.
//!
//! The cost is latency: a block of `kernel_len` frames plus half the kernel (`Effect::latency_samples`, 6,144 frames —
//! 128 ms — at 48 kHz; the kernel doubles at 88.2/96 kHz and again above, so the latency stays near 128 ms), which
//! plugin delay compensation lines the rest of the mix up with. A setting change is heard once the designer has
//! designed it (a poll of up to 20 ms plus a few ms) and the next block begins; an offline render running faster than
//! real time may hear an automated move a block or two late. The graph in the interface is the EQ's
//! (`app/src/domain/eqCurve.ts`), with the latency noted.

mod convolver;
mod design;
mod worker;

use std::sync::Arc;

use daw_contracts::{ContractError, Effect, L10n, ParamInfo, StreamConfig};

use crate::eq::{PARAMS, VALUES};
use crate::{EffectKind, clamp_param};
use convolver::Convolver;
use design::{Designer, kernel_len};
use worker::{Exchange, Worker};

pub struct LinearEq {
    sample_rate: u32,
    values: [f32; VALUES],
    convolver: Convolver,
    exchange: Arc<Exchange>,
    /// The designer thread: none until `start` (the factory starts it; a bare `build` in a test has none).
    worker: Option<Worker>,
    /// The last kernel taken from the exchange, and the buffer it is copied into.
    seen: u64,
    incoming: Vec<f32>,
}

impl LinearEq {
    /// On the control side: a kernel for the current settings at once, with no crossfade.
    fn design_now(&mut self) {
        let len = kernel_len(self.sample_rate);
        if self.convolver.len() != len {
            self.convolver = Convolver::new(len);
            self.incoming = vec![0.0; len + 1];
        }
        let mut designer = Designer::new(len);
        self.convolver.install(designer.design(self.sample_rate as f32, &self.values));
    }
}

impl EffectKind for LinearEq {
    const KIND: &'static str = "linear_eq";
    const LABEL: L10n = L10n { en: "Linear-phase EQ", ru: "Линейно-фазовый эквалайзер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let len = kernel_len(config.sample_rate);
        let mut eq = Self {
            sample_rate: config.sample_rate,
            values: PARAMS.map(|p| p.default),
            convolver: Convolver::new(len),
            exchange: Exchange::new(),
            worker: None,
            seen: 0,
            incoming: vec![0.0; len + 1],
        };
        eq.design_now();
        eq
    }

    fn start(&mut self) -> Result<(), ContractError> {
        let covered = self.exchange.requests();
        self.design_now();
        self.worker = Some(Worker::start(Arc::clone(&self.exchange), covered)?);
        Ok(())
    }
}

impl Effect for LinearEq {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate;
        self.design_now();
        self.convolver.clear();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((index, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        self.values[index] = value;
        self.exchange.ask(&self.values, self.sample_rate);
    }

    fn reset(&mut self) {
        self.convolver.clear();
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        if !self.convolver.is_switching()
            && let Some(version) = self.exchange.take(self.seen, self.sample_rate, &mut self.incoming)
        {
            self.seen = version;
            self.convolver.offer(&self.incoming);
        }
        for frame in interleaved.chunks_exact_mut(2) {
            (frame[0], frame[1]) = self.convolver.push(frame[0], frame[1]);
        }
    }

    fn latency_samples(&self) -> u32 {
        self.convolver.latency() as u32
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, noise, sine, tone};

    fn started(set: &[(&str, f32)]) -> LinearEq {
        let mut eq = build::<LinearEq>(set.iter().copied());
        eq.start().unwrap();
        eq
    }

    /// The level of `hz` through the effect, against the input's, in dB (the latency skipped).
    fn gain_db(eq: &mut LinearEq, hz: f32) -> f32 {
        let latency = eq.latency_samples() as usize;
        let mut signal = sine(hz, 0.25, latency + SR as usize);
        eq.process(&mut signal, None);
        20.0 * (tone(&signal[(latency + SR as usize / 2) * 2..], hz) / 0.25).log10()
    }

    #[test]
    fn reports_its_latency_and_an_impulse_comes_out_there() {
        let mut eq = started(&[]);
        assert_eq!(eq.latency_samples(), 6_144, "a 4,096-frame block and half the kernel at 48 kHz");
        assert_latency_at_impulse(&mut eq);
        eq.prepare(StreamConfig { sample_rate: 96_000, channels: 2 });
        assert_eq!(eq.latency_samples(), 12_288, "the kernel doubles at 96 kHz");
        assert_latency_at_impulse(&mut eq);
    }

    #[test]
    fn flat_settings_are_a_pure_delay() {
        let mut eq = started(&[]);
        let latency = eq.latency_samples() as usize;
        let input = noise(20_000, 0.5);
        let mut output = input.clone();
        output.extend(vec![0.0; latency * 2]);
        eq.process(&mut output, None);
        for (out, inp) in output[latency * 2..].iter().zip(&input) {
            assert!((out - inp).abs() < 1e-4, "{out} vs {inp}");
        }
    }

    #[test]
    fn the_magnitude_is_the_eqs() {
        let set = [("low_gain", 9.0), ("mid_gain", -12.0), ("mid_freq", 2_000.0), ("high_cut", 8_000.0)];
        let mut linear = started(&set);
        let mut minimum = build::<crate::eq::Eq>(set);
        for hz in [60.0, 400.0, 2_000.0, 6_000.0, 12_000.0] {
            let mut tone_in = sine(hz, 0.25, SR as usize);
            minimum.reset();
            minimum.process(&mut tone_in, None);
            let expected = 20.0 * (tone(&tone_in[SR as usize..], hz) / 0.25).log10();
            linear.reset();
            let got = gain_db(&mut linear, hz);
            assert!((got - expected).abs() < 0.5, "{hz} Hz: {got} dB, the EQ {expected} dB");
        }
    }

    #[test]
    fn the_impulse_response_is_symmetric_about_the_latency() {
        let mut eq = started(&[("mid_gain", 12.0), ("low_cut", 80.0)]);
        let latency = eq.latency_samples() as usize;
        let mut buffer = crate::testing::impulse(latency * 2);
        eq.process(&mut buffer, None);
        let left: Vec<f32> = buffer.iter().step_by(2).copied().collect();
        for i in 1..1_500 {
            assert!((left[latency + i] - left[latency - i]).abs() < 1e-4, "±{i}: {} {}", left[latency + i], left[latency - i]);
        }
    }

    #[test]
    fn a_new_setting_is_designed_off_the_audio_thread_and_crossfaded_in() {
        let mut eq = started(&[]);
        let latency = eq.latency_samples() as usize;
        let mut signal = sine(1_000.0, 0.25, latency + SR as usize * 4);
        let blocks: Vec<&mut [f32]> = signal.chunks_mut(1_024).collect();
        let started = Instant::now();
        let mut changed_at = None;
        for (i, block) in blocks.into_iter().enumerate() {
            if i == 40 {
                eq.set_param("mid_gain", -12.0);
                changed_at = Some(i);
            }
            eq.process(block, None);
            if changed_at.is_some() {
                // Give the designer thread time, as the audio callback's real-time pace would.
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(started.elapsed() < Duration::from_secs(20), "the designer answers");
        }
        let tail = &signal[signal.len() - SR as usize..];
        assert!((tone(tail, 1_000.0) / 0.25 - 0.25).abs() < 0.02, "−12 dB at 1 kHz: {}", tone(tail, 1_000.0) / 0.25);
        let baseline = max_step(&sine(1_000.0, 0.25, SR as usize));
        assert!(max_step(&signal) < baseline * 1.05, "no click on the switch: {} with {baseline}", max_step(&signal));
    }
}
