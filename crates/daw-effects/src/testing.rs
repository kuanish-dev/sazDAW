//! Helpers for the effect tests.

use daw_contracts::{Effect, EffectFactory, Params, StreamConfig};

pub const SR: u32 = 48_000;

/// A stereo sine of the given frequency and amplitude.
pub fn sine(hz: f32, amplitude: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * hz * i as f32 / SR as f32).sin() * amplitude;
            [v, v]
        })
        .collect()
}

pub fn rms(buffer: &[f32]) -> f32 {
    (buffer.iter().map(|v| v * v).sum::<f32>() / buffer.len().max(1) as f32).sqrt()
}

pub fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |a, b| a.max(b.abs()))
}

/// A stereo impulse: one in the first frame, silence after.
pub fn impulse(frames: usize) -> Vec<f32> {
    let mut buffer = vec![0.0; frames * 2];
    buffer[0] = 1.0;
    buffer[1] = 1.0;
    buffer
}

/// The largest step between neighbouring frames of one channel: a click or zipper noise shows up as a spike.
pub fn max_step(buffer: &[f32]) -> f32 {
    buffer.windows(3).map(|w| (w[2] - w[0]).abs()).fold(0.0, f32::max)
}

/// Amplitude of the `hz` component in the left channel (one DFT bin). Exact when the buffer holds a whole number of periods.
pub fn tone(buffer: &[f32], hz: f32) -> f32 {
    let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
    let (re, im) = buffer.iter().step_by(2).enumerate().fold((0.0, 0.0), |(re, im), (i, &v)| {
        let phase = w * i as f64;
        (re + f64::from(v) * phase.cos(), im - f64::from(v) * phase.sin())
    });
    let frames = (buffer.len() / 2).max(1) as f64;
    ((re * re + im * im).sqrt() * 2.0 / frames) as f32
}

/// Deterministic white noise in −amplitude..amplitude, stereo (a small LCG: the same buffer on every run).
pub fn noise(frames: usize, amplitude: f32) -> Vec<f32> {
    let mut state = 0x2545_f491_u32;
    (0..frames * 2)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amplitude
        })
        .collect()
}

/// The `Effect` contract every built-in effect honours, checked the same way for each kind:
/// * parameter fit: values outside the range, NaN and infinities of every param leave the output finite;
/// * `prepare` at another rate and `reset`: silence in gives silence out;
/// * `reset` forgets the past: after a loud burst and a reset, silence still gives silence.
pub fn assert_honours_contract(factory: &dyn EffectFactory) {
    let kind = factory.kind();
    let mut effect = factory.create(&Params::new(), None).unwrap();
    effect.prepare(StreamConfig { sample_rate: 44_100, channels: 2 });
    for p in factory.describe() {
        for value in [p.min - 1_000.0, p.max + 1_000.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            effect.set_param(p.name, value);
        }
    }
    effect.set_param("no_such_param", 1.0);
    let mut loud = noise(4_096, 1.0);
    let key = loud.clone();
    effect.process(&mut loud, Some(&key));
    assert!(loud.iter().all(|v| v.is_finite()), "{kind}: a finite output with every param at its edge");

    effect.prepare(StreamConfig { sample_rate: 96_000, channels: 2 });
    effect.reset();
    let mut silence = vec![0.0; 8_192];
    effect.process(&mut silence, None);
    assert!(peak(&silence) < 1e-6, "{kind}: silence after prepare and reset gives silence: {}", peak(&silence));

    let mut burst = noise(4_096, 0.9);
    effect.process(&mut burst, None);
    effect.reset();
    let mut silence = vec![0.0; 16_384];
    effect.process(&mut silence, None);
    assert!(peak(&silence) < 1e-6, "{kind}: reset forgets the burst: {}", peak(&silence));
}

/// How far after the reported latency an impulse may peak: an IIR filter (a crossover's all-passes, a shelf) smears it
/// over a few frames.
const IMPULSE_SPREAD: usize = 8;

/// The latency the effect reports (`Effect::latency_samples`) is where an impulse comes out: nothing before it, the
/// strongest frame at most `IMPULSE_SPREAD` frames after it. The effect must pass a half-scale impulse at its current
/// settings.
pub fn assert_latency_at_impulse(effect: &mut dyn Effect) {
    let latency = effect.latency_samples() as usize;
    let frames = latency + 4_096;
    let mut buffer = impulse(frames);
    for sample in &mut buffer {
        *sample *= 0.5;
    }
    effect.process(&mut buffer, None);
    let left: Vec<f32> = buffer.iter().step_by(2).copied().collect();
    let strongest = left.iter().enumerate().fold(0, |best, (i, v)| if v.abs() > left[best].abs() { i } else { best });
    assert!(left[..latency].iter().all(|v| v.abs() < 1e-4), "nothing before the latency of {latency} frames");
    assert!(strongest >= latency && strongest <= latency + IMPULSE_SPREAD, "the impulse peaks at {strongest}, latency {latency}");
    assert!(left[strongest].abs() > 0.1, "the impulse passes: {}", left[strongest]);
}
