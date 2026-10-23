//! The synth's sound core: 8 voices, each with two oscillators (and their unison copies spread across the stereo field),
//! an AHDSR volume envelope, a resonant lowpass with its own envelope and a pitch envelope; a mono mode that slides
//! between held notes; one LFO for all voices; a low cut on the mix. Pitch wheel (pitch bend, ±2 semitones) and mod
//! wheel (CC1: opens the filter up to four octaves above the cutoff).
//! Pure math without the contracts: the `Instrument` glue in `lib.rs` feeds it notes and settings.

use crate::envelope::{Ahdsr, AttackDecay, Envelope};
use crate::filter::{Coefficients, Svf};
use crate::lfo::{Lfo, LfoShape};
use crate::osc::{Noise, Waveform, wave};

pub(crate) const VOICES: usize = 8;
/// The most unison copies of each oscillator.
pub(crate) const MAX_UNISON: usize = 7;
/// How far pitch bend at the end of the wheel shifts the pitch, in semitones.
pub(crate) const BEND_RANGE_SEMITONES: f32 = 2.0;
/// The low cut at or below this frequency is off (its minimum; a little above it for the glide's rounding).
const LOW_CUT_OFF_HZ: f32 = 20.01;
/// How many octaves the mod wheel at its maximum raises the filter cutoff.
pub(crate) const MODULATION_OCTAVES: f32 = 4.0;
/// Time constants per glide or pitch-decay time: after the set time a slide or the pitch envelope is 95 % done.
const SETTLE: f32 = 3.0;
/// The voice filter never goes below this, whatever the envelope and the LFO do.
const MIN_CUTOFF_HZ: f32 = 20.0;
/// Keys mono mode remembers to slide back to.
const HELD_NOTES: usize = 16;
/// Phase offsets of the unison copies (golden-ratio steps): copies starting in step would sound like one hard attack.
const GOLDEN: f32 = 0.618_034;
/// The song tempo for a synced LFO until the engine reports one (previews, tests).
const DEFAULT_BPM: f64 = 120.0;

/// Unison: how many detuned copies each oscillator plays and where they sit in the stereo field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Unison {
    count: usize,
    /// Frequency ratio of each copy.
    ratios: [f32; MAX_UNISON],
    /// Left and right gain of each copy.
    pans: [[f32; 2]; MAX_UNISON],
    /// Keeps the loudness about the same whatever the count.
    norm: f32,
}

impl Unison {
    /// `voices` copies (`1..=MAX_UNISON`, rounded) spread over `detune_cents` (the outer two are that far apart) and
    /// over `width` (0 all in the centre, 1 the outer two at the sides). One copy is the plain oscillator in the centre.
    pub(crate) fn new(voices: f32, detune_cents: f32, width: f32) -> Self {
        let count = (voices.round().max(1.0) as usize).min(MAX_UNISON);
        let mut unison = Self { count, ratios: [1.0; MAX_UNISON], pans: [[1.0; 2]; MAX_UNISON], norm: 1.0 / (count as f32).sqrt() };
        if count > 1 {
            for k in 0..count {
                let position = 2.0 * k as f32 / (count - 1) as f32 - 1.0;
                unison.ratios[k] = (position * detune_cents / 2.0 / 1200.0).exp2();
                let pan = position * width.clamp(0.0, 1.0);
                unison.pans[k] = [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)];
            }
        }
        unison
    }
}

impl Default for Unison {
    fn default() -> Self {
        Self::new(1.0, 0.0, 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LfoSettings {
    pub shape: LfoShape,
    pub rate_hz: f32,
    /// The cycle in beats when the LFO follows the song grid; `None` runs at `rate_hz`.
    pub sync_beats: Option<f64>,
    pub pitch_cents: f32,
    pub filter_octaves: f32,
    /// How deep it dips the volume, `0..=1`.
    pub volume: f32,
}

impl Default for LfoSettings {
    fn default() -> Self {
        Self { shape: LfoShape::Sine, rate_hz: 5.0, sync_beats: None, pitch_cents: 0.0, filter_octaves: 0.0, volume: 0.0 }
    }
}

/// Current parameter values (seconds, Hz, linear gain; levels and amounts `0..=1` unless the name says otherwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    pub waveform: Waveform,
    pub osc2: Waveform,
    /// Level of the second oscillator next to the first, `0..=1` (0 is off).
    pub osc2_level: f32,
    /// Frequency ratio of the second oscillator to the first.
    pub osc2_ratio: f32,
    pub unison: Unison,
    pub envelope: Ahdsr,
    /// Voice filter cutoff (the filter envelope, the LFO and the mod wheel move it from here).
    pub high_cut_hz: f32,
    /// Highpass corner on the mix: removes lows below it; 20 Hz leaves the sound untouched.
    pub low_cut_hz: f32,
    pub resonance: f32,
    /// How many octaves the filter envelope at its top moves the cutoff (negative closes it).
    pub filter_env_octaves: f32,
    pub filter_attack: f32,
    pub filter_decay: f32,
    /// Semitones the pitch starts away from the note (a kick's punch, a falling riser); it settles in `pitch_decay`.
    pub pitch_env_semitones: f32,
    pub pitch_decay: f32,
    /// One voice: a key pressed while another is held slides to it without restarting the envelopes (808 slides).
    pub mono: bool,
    /// Seconds a mono slide takes; 0 jumps.
    pub glide: f32,
    pub lfo: LfoSettings,
    pub gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            osc2: Waveform::Saw,
            osc2_level: 0.0,
            osc2_ratio: 1.0,
            unison: Unison::default(),
            envelope: Ahdsr::default(),
            high_cut_hz: 1_800.0,
            low_cut_hz: 20.0,
            resonance: 0.0,
            filter_env_octaves: 0.0,
            filter_attack: 0.005,
            filter_decay: 0.3,
            pitch_env_semitones: 0.0,
            pitch_decay: 0.08,
            mono: false,
            glide: 0.0,
            lfo: LfoSettings::default(),
            gain: 0.25,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Voice {
    note: u8,
    velocity: f32,
    age: u64,
    amp: Envelope,
    filter_env: AttackDecay,
    /// Share of the pitch envelope still left: 1 at the note start, settling to 0.
    pitch_env: f32,
    /// Current pitch (a MIDI note number, fractional while sliding) and the note it slides to.
    pitch: f32,
    target: f32,
    /// Phase (`0..1` of a cycle) of every unison copy of both oscillators.
    phases: [[f32; MAX_UNISON]; 2],
    /// The voice filter, per channel.
    filters: [Svf; 2],
}

impl Voice {
    const IDLE: Voice = Voice {
        note: 0,
        velocity: 0.0,
        age: 0,
        amp: Envelope::IDLE,
        filter_env: AttackDecay::REST,
        pitch_env: 0.0,
        pitch: 0.0,
        target: 0.0,
        phases: [[0.0; MAX_UNISON]; 2],
        filters: [Svf::REST; 2],
    };

    fn new(note: u8, velocity: f32, age: u64) -> Self {
        let mut phases = [[0.0; MAX_UNISON]; 2];
        for (k, phase) in phases[0].iter_mut().enumerate() {
            *phase = (k as f32 * GOLDEN).fract();
        }
        phases[1] = phases[0];
        let mut voice = Self { phases, ..Self::IDLE };
        voice.retrigger(note, velocity, age);
        voice
    }

    /// Starts the note's envelopes on this voice, keeping its phases and filter memory (no click on a retrigger).
    fn retrigger(&mut self, note: u8, velocity: f32, age: u64) {
        self.note = note;
        self.velocity = velocity;
        self.age = age;
        self.pitch = f32::from(note);
        self.target = self.pitch;
        self.pitch_env = 1.0;
        self.amp.start();
        self.filter_env.start();
    }

    /// Slides to `note` with the envelopes running on (mono legato).
    fn slide(&mut self, note: u8, glide: f32) {
        self.note = note;
        self.target = f32::from(note);
        if glide <= 0.0 {
            self.pitch = self.target;
        }
    }
}

/// Keys held down in mono mode, oldest first: letting go of the sounding one slides back to the newest one left.
#[derive(Debug, Clone, Copy)]
struct HeldNotes {
    notes: [u8; HELD_NOTES],
    len: usize,
}

impl HeldNotes {
    const EMPTY: HeldNotes = HeldNotes { notes: [0; HELD_NOTES], len: 0 };

    fn push(&mut self, note: u8) {
        self.remove(note);
        if self.len == HELD_NOTES {
            self.notes.copy_within(1.., 0);
            self.len -= 1;
        }
        self.notes[self.len] = note;
        self.len += 1;
    }

    fn remove(&mut self, note: u8) {
        if let Some(i) = self.notes[..self.len].iter().position(|n| *n == note) {
            self.notes.copy_within(i + 1..self.len, i);
            self.len -= 1;
        }
    }

    fn last(&self) -> Option<u8> {
        self.len.checked_sub(1).map(|i| self.notes[i])
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }
}

pub(crate) struct Engine {
    sample_rate: f32,
    voices: [Voice; VOICES],
    /// Lowpass of the mix at the low cut, per channel; subtracting it leaves the highpassed signal.
    low_state: [f32; 2],
    clock: u64,
    /// How much pitch bend speeds up the phase of all voices (1 means the wheel is centered).
    bend_ratio: f32,
    /// Mod wheel, `0..=1`.
    modulation: f32,
    noise: Noise,
    lfo: Lfo,
    /// The song tempo for a synced LFO.
    beats_per_frame: f64,
    held: HeldNotes,
}

impl Default for Engine {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            sample_rate,
            voices: [Voice::IDLE; VOICES],
            low_state: [0.0; 2],
            clock: 0,
            bend_ratio: 1.0,
            modulation: 0.0,
            noise: Noise::new(),
            lfo: Lfo::new(),
            beats_per_frame: DEFAULT_BPM / 60.0 / f64::from(sample_rate),
            held: HeldNotes::EMPTY,
        }
    }
}

#[cfg(test)]
pub(crate) fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(note) - 69.0) / 12.0)
}

impl Engine {
    pub(crate) fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.beats_per_frame = DEFAULT_BPM / 60.0 / f64::from(sample_rate);
        self.reset();
    }

    pub(crate) fn reset(&mut self) {
        self.voices = [Voice::IDLE; VOICES];
        self.low_state = [0.0; 2];
        self.bend_ratio = 1.0;
        self.modulation = 0.0;
        self.noise = Noise::new();
        self.lfo = Lfo::new();
        self.held = HeldNotes::EMPTY;
    }

    /// Pitch wheel: `-1..=1` (0 is the middle), the end is ±[`BEND_RANGE_SEMITONES`].
    pub(crate) fn set_pitch_bend(&mut self, bend: f32) {
        self.bend_ratio = 2f32.powf(bend.clamp(-1.0, 1.0) * BEND_RANGE_SEMITONES / 12.0);
    }

    /// Mod wheel: `0..=1`.
    pub(crate) fn set_modulation(&mut self, amount: f32) {
        self.modulation = amount.clamp(0.0, 1.0);
    }

    /// The song position (in beats) and tempo at the start of a segment: while playing, a synced LFO is put back on
    /// the grid, so it stays on the beat after a seek or a loop.
    pub(crate) fn set_timing(&mut self, start_beat: f64, beats_per_frame: f64, playing: bool, sync_beats: Option<f64>) {
        if beats_per_frame > 0.0 {
            self.beats_per_frame = beats_per_frame;
        }
        if let (true, Some(cycle)) = (playing, sync_beats) {
            self.lfo.align(start_beat, cycle);
        }
    }

    pub(crate) fn note_on(&mut self, note: u8, velocity: f32, s: &Settings) {
        self.clock += 1;
        let velocity = velocity.clamp(0.0, 1.0);
        if s.mono {
            let legato = !self.held.is_empty() && self.voices[0].amp.is_held();
            self.held.push(note);
            let voice = &mut self.voices[0];
            if legato {
                voice.slide(note, s.glide);
            } else if voice.amp.is_idle() {
                *voice = Voice::new(note, velocity, self.clock);
            } else {
                voice.retrigger(note, velocity, self.clock);
            }
            return;
        }
        let index = self
            .voices
            .iter()
            .position(|v| v.amp.is_idle())
            .or_else(|| self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i))
            .unwrap_or(0);
        self.voices[index] = Voice::new(note, velocity, self.clock);
    }

    pub(crate) fn note_off(&mut self, note: u8, s: &Settings) {
        if s.mono {
            self.held.remove(note);
            let back = self.held.last();
            let voice = &mut self.voices[0];
            if let (true, Some(back)) = (voice.note == note && voice.amp.is_held(), back) {
                voice.slide(back, s.glide);
                return;
            }
        }
        let sample_rate = self.sample_rate;
        for voice in self.voices.iter_mut().filter(|v| v.note == note) {
            voice.amp.release(s.envelope.release, sample_rate);
        }
    }

    pub(crate) fn all_notes_off(&mut self, release: f32) {
        self.held = HeldNotes::EMPTY;
        let sample_rate = self.sample_rate;
        for voice in &mut self.voices {
            voice.amp.release(release, sample_rate);
        }
    }

    /// One stereo frame `[left, right]`.
    pub(crate) fn next_frame(&mut self, s: &Settings) -> [f32; 2] {
        let sr = self.sample_rate;
        let cycles = s.lfo.sync_beats.map_or(f64::from(s.lfo.rate_hz / sr), |beats| self.beats_per_frame / beats);
        let lfo = self.lfo.advance(s.lfo.shape, cycles);
        let glide = if s.glide > 0.0 { 1.0 - (-SETTLE / (s.glide * sr)).exp() } else { 1.0 };
        let pitch_settle = (-SETTLE / (s.pitch_decay.max(0.0005) * sr)).exp();
        let pitch_lfo = lfo * s.lfo.pitch_cents / 100.0;
        let cutoff_octaves = self.modulation * MODULATION_OCTAVES + lfo * s.lfo.filter_octaves;
        let max_cutoff = 0.45 * sr;
        let osc2_share = 1.0 / (1.0 + s.osc2_level);
        let bend = self.bend_ratio;
        let mut mix = [0.0f32; 2];
        let Self { voices, noise, .. } = self;
        for voice in voices.iter_mut() {
            if voice.amp.is_idle() {
                continue;
            }
            let level = voice.amp.advance(&s.envelope, sr);
            if voice.amp.is_idle() {
                *voice = Voice::IDLE;
                continue;
            }
            voice.pitch += (voice.target - voice.pitch) * glide;
            voice.pitch_env *= pitch_settle;
            let semitones = voice.pitch - 69.0 + voice.pitch_env * s.pitch_env_semitones + pitch_lfo;
            let step = 440.0 * (semitones / 12.0).exp2() * bend / sr;
            let noise_now = noise.sample();
            let mut frame = [0.0f32; 2];
            for k in 0..s.unison.count {
                let (first, second) = (voice.phases[0][k], voice.phases[1][k]);
                let value = wave(s.waveform, first, noise_now) + s.osc2_level * wave(s.osc2, second, noise_now);
                frame[0] += value * s.unison.pans[k][0];
                frame[1] += value * s.unison.pans[k][1];
                let copy_step = step * s.unison.ratios[k];
                voice.phases[0][k] = (first + copy_step).fract();
                voice.phases[1][k] = (second + copy_step * s.osc2_ratio).fract();
            }
            let scale = level * voice.velocity * s.unison.norm * osc2_share;
            let envelope = voice.filter_env.advance(s.filter_attack, s.filter_decay, sr);
            let cutoff = (s.high_cut_hz * (cutoff_octaves + envelope * s.filter_env_octaves).exp2()).clamp(MIN_CUTOFF_HZ, max_cutoff);
            let coefficients = Coefficients::lowpass(cutoff, s.resonance, sr);
            for ((out, filter), x) in mix.iter_mut().zip(&mut voice.filters).zip(frame) {
                *out += filter.lowpass(&coefficients, x * scale);
            }
        }
        let low_alpha =
            if s.low_cut_hz <= LOW_CUT_OFF_HZ { 0.0 } else { 1.0 - (-std::f32::consts::TAU * s.low_cut_hz.min(max_cutoff) / sr).exp() };
        let gain = s.gain * (1.0 - s.lfo.volume * (1.0 - lfo) * 0.5);
        let mut out = [0.0f32; 2];
        for ((out, low), x) in out.iter_mut().zip(&mut self.low_state).zip(mix) {
            *low += low_alpha * (x - *low);
            *out = (x - *low) * gain;
        }
        out
    }
}

#[cfg(test)]
mod tests;
