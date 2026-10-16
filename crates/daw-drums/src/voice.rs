//! A drum voice: synthesizes one hit from a tone, noise and envelopes.

use std::f32::consts::TAU;

use daw_contracts::{AudioBlock, ControllerKind, Instrument, StreamConfig};

use crate::{DrumKind, PARAMS};

/// How long a hit takes to fade out after `all_notes_off`: short enough to stop at once, long enough not to click.
const STOP_FADE_SECONDS: f32 = 0.05;

pub(crate) struct Drum {
    kind: DrumKind,
    gain: f32,
    /// Frequency multiplier of the tonal parts.
    pitch_ratio: f32,
    /// Length multiplier.
    decay_scale: f32,
    sample_rate: f32,
    /// Seconds since the hit started; `None` while silent.
    age: Option<f32>,
    /// Frames left of the fade after `all_notes_off`; `None` while the hit plays to its end. Counted in whole frames,
    /// not seconds: a sum of float steps never lands exactly on zero, and the fade would leave a tiny extra sample.
    fade: Option<u32>,
    velocity: f32,
    phase: f32,
    /// Extra phases for ringing cymbals (ride).
    partials: [f32; 3],
    noise_state: u32,
    last_noise: f32,
}

/// Exponential decay with a time constant of `tau` seconds.
fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

/// A decay that starts at time `start`; silence before it.
fn decay_from(t: f32, start: f32, tau: f32) -> f32 {
    if t >= start { decay(t - start, tau) } else { 0.0 }
}

impl Drum {
    pub(crate) fn new(kind: DrumKind) -> Self {
        Self {
            kind,
            gain: 1.0,
            pitch_ratio: 1.0,
            decay_scale: 1.0,
            sample_rate: 48_000.0,
            age: None,
            fade: None,
            velocity: 1.0,
            phase: 0.0,
            partials: [0.0; 3],
            noise_state: 0x1234_5678,
            last_noise: 0.0,
        }
    }

    /// `STOP_FADE_SECONDS` at the current sample rate, at least one frame.
    fn fade_frames(&self) -> u32 {
        (STOP_FADE_SECONDS * self.sample_rate).round().max(1.0) as u32
    }

    fn length(&self) -> f32 {
        self.decay_scale
            * match self.kind {
                DrumKind::Kick | DrumKind::TomHigh | DrumKind::TomMid | DrumKind::TomLow => 0.5,
                DrumKind::Snare => 0.3,
                DrumKind::Hat => 0.12,
                DrumKind::Clap => 0.35,
                DrumKind::OpenHat => 0.55,
                DrumKind::Crash => 1.6,
                DrumKind::Ride => 1.0,
                DrumKind::Rim => 0.08,
            }
    }

    /// White noise (xorshift32): deterministic, so a hit sounds the same in every render.
    fn noise(&mut self) -> f32 {
        let mut x = self.noise_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise_state = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// Noise without lows (the difference of adjacent samples), the base of hats, clap and cymbals.
    fn hp_noise(&mut self) -> f32 {
        let noise = self.noise();
        let high = noise - self.last_noise;
        self.last_noise = noise;
        high
    }

    /// A sine of the fundamental at `freq` Hz (tuning already applied).
    fn osc(&mut self, freq: f32) -> f32 {
        self.phase = (self.phase + TAU * freq / self.sample_rate) % TAU;
        self.phase.sin()
    }

    /// Three inharmonic overtones: the "ring" of the ride.
    fn ring(&mut self) -> f32 {
        let mut ring = 0.0;
        for (phase, freq) in self.partials.iter_mut().zip([3_150.0, 4_380.0, 6_270.0]) {
            *phase = (*phase + TAU * freq * self.pitch_ratio / self.sample_rate) % TAU;
            ring += phase.sin();
        }
        ring
    }

    /// A tom: a tone with a fast pitch drop from `base` Hz and a short click at the start.
    fn tom(&mut self, t: f32, base: f32) -> f32 {
        let tone = self.osc((base + 70.0 * decay(t, 0.03)) * self.pitch_ratio);
        tone * decay(t, 0.17) * 0.85 + self.noise() * decay(t, 0.004) * 0.2
    }

    fn sample(&mut self, t: f32) -> f32 {
        let t = t / self.decay_scale;
        let ratio = self.pitch_ratio;
        match self.kind {
            DrumKind::Kick => self.osc((45.0 + 110.0 * decay(t, 0.035)) * ratio) * decay(t, 0.18) * 0.9,
            DrumKind::Snare => self.noise() * decay(t, 0.06) * 0.5 + self.osc(190.0 * ratio) * decay(t, 0.05) * 0.4,
            DrumKind::Hat => self.hp_noise() * decay(t, 0.02) * 0.35,
            DrumKind::Clap => {
                // Three quick claps in a row and a shared tail after the last.
                let bursts: f32 = [0.0, 0.011, 0.022].iter().map(|&start| decay_from(t, start, 0.01)).sum();
                self.hp_noise() * (bursts * 0.12 + decay_from(t, 0.022, 0.08) * 0.18)
            }
            DrumKind::OpenHat => self.hp_noise() * decay(t, 0.16) * 0.3,
            DrumKind::TomHigh => self.tom(t, 210.0),
            DrumKind::TomMid => self.tom(t, 150.0),
            DrumKind::TomLow => self.tom(t, 105.0),
            // A sharp hit and a long hissing tail.
            DrumKind::Crash => self.hp_noise() * (decay(t, 0.04) * 0.25 + decay(t, 0.55) * 0.22),
            DrumKind::Ride => {
                let hiss = self.hp_noise();
                self.ring() * decay(t, 0.4) * 0.11 + hiss * decay(t, 0.12) * 0.12
            }
            DrumKind::Rim => (self.noise() * 0.35 + self.osc(1_050.0 * ratio) * 0.65) * decay(t, 0.012) * 0.7,
        }
    }
}

impl Instrument for Drum {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.age = None;
    }

    fn note_on(&mut self, _note: u8, velocity: f32) {
        self.age = Some(0.0);
        self.fade = None;
        self.velocity = velocity.clamp(0.0, 1.0);
        self.phase = 0.0;
        self.partials = [0.0; 3];
        self.last_noise = 0.0;
    }

    fn note_off(&mut self, _note: u8) {}

    // A hit ignores `note_off`, but stop, seek and loop wraps must not leave a cymbal ringing for seconds.
    fn all_notes_off(&mut self) {
        if self.age.is_some() && self.fade.is_none() {
            self.fade = Some(self.fade_frames());
        }
    }

    // A hit plays to its end on its own: neither pitch bend nor the pedal applies to it.
    fn controller(&mut self, _kind: ControllerKind, _value: f32) {}

    fn set_param(&mut self, name: &str, value: f32) {
        let Some(info) = PARAMS.iter().find(|p| p.name == name) else {
            return;
        };
        let value = value.clamp(info.min, info.max);
        match name {
            "gain" => self.gain = value,
            "tune" => self.pitch_ratio = 2f32.powf(value / 12.0),
            "decay" => self.decay_scale = value,
            _ => {}
        }
    }

    fn render(&mut self, block: &mut AudioBlock) {
        let dt = 1.0 / self.sample_rate;
        let length = self.length();
        for frame in 0..block.frames() {
            let Some(t) = self.age else {
                return;
            };
            if t >= length || self.fade == Some(0) {
                self.age = None;
                self.fade = None;
                return;
            }
            let fade = self.fade.map_or(1.0, |left| left as f32 / self.fade_frames() as f32);
            self.fade = self.fade.map(|left| left - 1);
            let value = self.sample(t) * self.velocity * self.gain * fade;
            block.add_to_frame(frame, value);
            self.age = Some(t + dt);
        }
    }
}
