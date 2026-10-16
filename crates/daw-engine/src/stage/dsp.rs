//! The small DSP blocks of the stage voices and the room: a one-pole lowpass, a biquad, the head-shadow shelf and a
//! delay line with fractional reads (the mixer's channel utility uses it too, `mixer/utility.rs`). They are what
//! refactor batch 6 moves into the shared `daw-dsp` crate (the biquad repeats the RBJ formulas of
//! `daw-effects/src/biquad.rs` until then: the engine may not depend on the effects).

use std::f32::consts::TAU;

use super::geometry::{HEAD_RADIUS, SPEED_OF_SOUND};

/// The coefficient of a one-pole lowpass at `hz` (kept below the Nyquist frequency).
pub fn one_pole(hz: f32, sample_rate: f32) -> f32 {
    1.0 - (-TAU * hz.min(sample_rate * 0.45) / sample_rate).exp()
}

#[derive(Clone, Copy, Default)]
pub struct OnePole {
    state: f32,
}

impl OnePole {
    pub fn process(&mut self, input: f32, coefficient: f32) -> f32 {
        self.state += coefficient * (input - self.state);
        self.state
    }
}

/// A biquad (RBJ Audio EQ Cookbook) in transposed direct form II, which tolerates its coefficients changing every
/// segment while the place glides.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Lets everything through unchanged.
    pub const PASS: Biquad = Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 };

    fn set(&mut self, [b0, b1, b2, a0, a1, a2]: [f32; 6]) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    pub fn set_peaking(&mut self, sample_rate: f32, freq: f32, gain_db: f32, q: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let (sin, cos) = (TAU * freq.min(sample_rate * 0.45) / sample_rate).sin_cos();
        let alpha = sin / (2.0 * q);
        self.set([1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a]);
    }

    pub fn set_high_shelf(&mut self, sample_rate: f32, freq: f32, gain_db: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let (sin, cos) = (TAU * freq.min(sample_rate * 0.45) / sample_rate).sin_cos();
        let beta = a.sqrt() * sin * std::f32::consts::SQRT_2;
        self.set([
            a * ((a + 1.0) + (a - 1.0) * cos + beta),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - beta),
            (a + 1.0) - (a - 1.0) * cos + beta,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - beta,
        ]);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        output
    }
}

/// The head's shadow on one ear (Brown and Duda's spherical head): a first-order shelf, flat in the lows and `highs`
/// times the level above the corner where the head grows large against the wavelength (c / a ≈ 1.25 kHz).
/// `highs` may change every sample: the coefficients are linear in it.
#[derive(Clone, Copy)]
pub struct HeadShadow {
    /// `K / (K + β)`, `β / (K + β)` and the pole, with `K = 2 · sample rate` (the bilinear transform) and `β = 2c / a`.
    k: f32,
    b: f32,
    a1: f32,
    x1: f32,
    y1: f32,
}

impl HeadShadow {
    pub fn new(sample_rate: f32) -> Self {
        let k = 2.0 * sample_rate;
        let beta = 2.0 * SPEED_OF_SOUND / HEAD_RADIUS;
        let norm = k + beta;
        Self { k: k / norm, b: beta / norm, a1: (beta - k) / norm, x1: 0.0, y1: 0.0 }
    }

    pub fn process(&mut self, input: f32, highs: f32) -> f32 {
        let output = (highs * self.k + self.b) * input + (self.b - highs * self.k) * self.x1 - self.a1 * self.y1;
        self.x1 = input;
        self.y1 = output;
        output
    }
}

/// A mono delay line: the newest sample and those before it, read at fractional delays (linear interpolation), so a
/// delay that glides makes a smooth pitch bend rather than clicks.
pub struct DelayLine {
    samples: Vec<f32>,
    mask: usize,
    /// Where the next sample goes.
    write: usize,
}

impl DelayLine {
    /// A line holding at least `frames` past samples (allocates: only outside the audio thread).
    pub fn new(frames: usize) -> Self {
        let length = (frames + 2).next_power_of_two();
        Self { samples: vec![0.0; length], mask: length - 1, write: 0 }
    }

    pub fn clear(&mut self) {
        self.samples.fill(0.0);
    }

    pub fn push(&mut self, sample: f32) {
        self.samples[self.write & self.mask] = sample;
        self.write = self.write.wrapping_add(1);
    }

    /// The sound `delay` frames before the newest sample (0 is the newest), clamped to what the line holds.
    pub fn read(&self, delay: f32) -> f32 {
        let delay = delay.clamp(0.0, (self.mask - 1) as f32);
        let whole = delay as usize;
        let fraction = delay - whole as f32;
        let newest = self.write.wrapping_sub(1);
        let later = self.samples[newest.wrapping_sub(whole) & self.mask];
        let earlier = self.samples[newest.wrapping_sub(whole + 1) & self.mask];
        later + (earlier - later) * fraction
    }
}
