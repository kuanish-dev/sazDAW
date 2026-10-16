//! The plucked-string sound core: extended Karplus–Strong. A string is a delay line one period long; a pluck fills it
//! with a noise burst (darkened by a one-pole lowpass, with the harmonics that have a node at the pick position
//! removed), and every trip around the loop passes a two-tap lowpass (higher harmonics die first, as on a real
//! string), a first-order allpass (the fractional part of the period, so every note is in tune) and a loss factor
//! (how long the string rings). Releasing a note damps the string like a finger or a palm. The voices are summed and
//! a body adds two resonances (the guitar's air and top modes, or an electric's pickup). An amp's overdrive (`drive`)
//! soft-clips the result.
//! Pure math without the contracts: the `Instrument` glue in `lib.rs` feeds it notes and settings.

use std::f32::consts::TAU;

pub(crate) const VOICES: usize = 12;
/// Delay line length, a power of two: the longest period it holds is 23 Hz even at 192 kHz.
const LINE: usize = 8192;
const MASK: usize = LINE - 1;
/// How long a released string rings: the finger or palm stops it almost at once, but not with a click.
pub(crate) const DAMPED_T60: f32 = 0.12;
/// A voice below this level is finished.
const SILENT: f32 = 1e-4;
/// Every voice's share of the mix: a six-string strum stays below full scale.
const VOICE_LEVEL: f32 = 0.25;
/// The level follower's fall per frame (at 48 kHz it forgets a peak in about 50 ms).
const FOLLOWER_FALL: f32 = 0.9995;
/// The overdrive's input gain at full `drive`: a strummed chord clips hard, like a distortion pedal.
const DRIVE_GAIN: f32 = 20.0;

/// What depends on the instrument kind and does not change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Body {
    /// Body resonances: frequency in Hz and Q.
    pub modes: [(f32, f32); 2],
    /// Where the string is plucked, as a share of its length from the bridge.
    pub pick: f32,
}

/// Current parameter values (seconds, `0..=1` shares, linear gain).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    /// Seconds an open string near G3 rings until −60 dB; higher strings ring shorter, lower ones longer.
    pub ring: f32,
    /// Darker (0) to brighter (1): the pluck and the loop filter. Applies from the next pluck.
    pub tone: f32,
    /// How much of the body resonances is mixed in.
    pub body: f32,
    /// The amp's overdrive: 0 is clean, 1 is fuzz.
    pub drive: f32,
    pub gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { ring: 4.0, tone: 0.5, body: 0.5, drive: 0.0, gain: 0.5 }
    }
}

/// The amp: a soft clipper whose input gain grows with `drive` (0 leaves the sound untouched). Dividing by that gain
/// keeps quiet playing at its level and squashes loud playing, so the loudness stays near the clean sound's (a little
/// lower at full drive) while the peaks flatten into the distorted sound.
fn overdrive(sample: f32, drive: f32) -> f32 {
    if drive <= 0.0 {
        return sample;
    }
    let gain = 1.0 + DRIVE_GAIN * drive;
    (sample * gain).tanh() / gain
}

/// The note's frequency in Hz (A4 = 440).
pub(crate) fn frequency(note: u8) -> f32 {
    440.0 * ((f32::from(note) - 69.0) / 12.0).exp2()
}

/// Per-trip loss for a string of `frequency` to fall by 60 dB in `t60` seconds.
fn loss_for(t60: f32, frequency: f32) -> f32 {
    0.001f32.powf(1.0 / (t60 * frequency).max(1.0))
}

struct Voice {
    line: Vec<f32>,
    write: usize,
    /// The whole samples of the period; `damp` and the allpass make up the fraction.
    delay: usize,
    /// The loop lowpass: 0 passes everything, 0.5 averages two neighbours (the darkest).
    damp: f32,
    allpass: f32,
    allpass_in: f32,
    allpass_out: f32,
    loss: f32,
    frequency: f32,
    note: u8,
    held: bool,
    active: bool,
    level: f32,
}

impl Voice {
    fn new() -> Self {
        Self {
            line: vec![0.0; LINE],
            write: 0,
            delay: 1,
            damp: 0.5,
            allpass: 0.0,
            allpass_in: 0.0,
            allpass_out: 0.0,
            loss: 0.0,
            frequency: 0.0,
            note: 0,
            held: false,
            active: false,
            level: 0.0,
        }
    }

    /// Sets the loop to the note's period: the whole samples, the lowpass's delay (`damp` samples at low
    /// frequencies) and the allpass's fraction, kept in `0.1..1.1` where the allpass stays well-behaved.
    fn tune(&mut self, frequency: f32, sample_rate: f32, damp: f32) {
        let period = (sample_rate / frequency).clamp(3.0, (LINE - 2) as f32);
        let rest = period - damp;
        let delay = ((rest - 0.1).floor() as usize).max(1);
        let fraction = rest - delay as f32;
        self.delay = delay;
        self.damp = damp;
        self.allpass = (1.0 - fraction) / (1.0 + fraction);
        self.frequency = frequency;
    }

    /// Fills the period with a noise burst: lowpassed (`brightness` is the one-pole coefficient), without the pick
    /// position's harmonics, zero-mean, peaking at `amplitude`.
    fn pluck(&mut self, rng: &mut u32, amplitude: f32, brightness: f32, pick: f32) {
        let length = self.delay + 1;
        let start = self.write + LINE - length;
        let mut lowpassed = 0.0;
        for k in 0..length {
            lowpassed += brightness * (noise(rng) - lowpassed);
            self.line[(start + k) & MASK] = lowpassed;
        }
        // Backwards, so every sample subtracts the original value `notch` samples earlier.
        let notch = ((pick * length as f32).round() as usize).clamp(1, length - 1);
        for k in (notch..length).rev() {
            self.line[(start + k) & MASK] -= self.line[(start + k - notch) & MASK];
        }
        let mean = (0..length).map(|k| self.line[(start + k) & MASK]).sum::<f32>() / length as f32;
        let peak = (0..length).map(|k| (self.line[(start + k) & MASK] - mean).abs()).fold(0.0f32, f32::max);
        let scale = if peak > 0.0 { amplitude / peak } else { 0.0 };
        for k in 0..length {
            let sample = &mut self.line[(start + k) & MASK];
            *sample = (*sample - mean) * scale;
        }
        self.allpass_in = 0.0;
        self.allpass_out = 0.0;
        self.level = amplitude;
        self.active = true;
        self.held = true;
    }

    fn next(&mut self) -> f32 {
        let newer = self.line[(self.write + LINE - self.delay) & MASK];
        let older = self.line[(self.write + LINE - self.delay - 1) & MASK];
        let filtered = (1.0 - self.damp) * newer + self.damp * older;
        let tuned = self.allpass * filtered + self.allpass_in - self.allpass * self.allpass_out;
        self.allpass_in = filtered;
        self.allpass_out = tuned;
        let sample = tuned * self.loss;
        self.line[self.write] = sample;
        self.write = (self.write + 1) & MASK;
        self.level = (self.level * FOLLOWER_FALL).max(sample.abs());
        if self.level < SILENT {
            self.active = false;
        }
        sample
    }
}

/// A two-pole bandpass with 0 dB at its peak (RBJ cookbook, transposed direct form II).
#[derive(Default)]
struct Resonator {
    b0: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Resonator {
    fn tune(&mut self, frequency: f32, q: f32, sample_rate: f32) {
        let w0 = TAU * frequency.min(sample_rate * 0.45) / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.a1 = -2.0 * w0.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = -self.a1 * output + self.z2;
        self.z2 = -self.b0 * input - self.a2 * output;
        // A silent tail would decay into denormals, which are slow on some CPUs.
        if output.abs() < 1e-20 && self.z1.abs() < 1e-20 && self.z2.abs() < 1e-20 {
            self.z1 = 0.0;
            self.z2 = 0.0;
        }
        output
    }
}

pub(crate) struct Engine {
    voices: Vec<Voice>,
    body: Body,
    resonators: [Resonator; 2],
    sample_rate: f32,
    rng: u32,
}

impl Engine {
    /// Allocates every voice's delay line here: notes and rendering never allocate.
    pub(crate) fn new(body: Body) -> Self {
        let mut engine = Self {
            voices: (0..VOICES).map(|_| Voice::new()).collect(),
            body,
            resonators: Default::default(),
            sample_rate: 48_000.0,
            rng: 0x9E37_79B9,
        };
        engine.prepare(engine.sample_rate);
        engine
    }

    pub(crate) fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for (resonator, (frequency, q)) in self.resonators.iter_mut().zip(self.body.modes) {
            resonator.tune(frequency, q, sample_rate);
        }
        for voice in &mut self.voices {
            voice.active = false;
        }
    }

    /// Plucks the note: the same note re-plucks its own string, otherwise a free voice or the quietest one.
    pub(crate) fn note_on(&mut self, note: u8, velocity: f32, settings: &Settings) {
        let velocity = velocity.clamp(0.0, 1.0);
        let index = self
            .voices
            .iter()
            .position(|voice| voice.active && voice.note == note)
            .or_else(|| self.voices.iter().position(|voice| !voice.active))
            .unwrap_or_else(|| {
                let quietest = self.voices.iter().enumerate().min_by(|a, b| a.1.level.total_cmp(&b.1.level));
                quietest.map_or(0, |(index, _)| index)
            });
        let tone = settings.tone.clamp(0.0, 1.0);
        let frequency = frequency(note);
        let t60 = (settings.ring * (196.0 / frequency).sqrt()).clamp(0.15, settings.ring * 3.0);
        // A harder pluck is brighter as well as louder.
        let brightness = (0.15 + 0.8 * tone) * (0.6 + 0.4 * velocity);
        let voice = &mut self.voices[index];
        voice.tune(frequency, self.sample_rate, 0.5 - 0.42 * tone);
        voice.loss = loss_for(t60, frequency);
        voice.note = note;
        voice.pluck(&mut self.rng, 0.1 + 0.9 * velocity * velocity, brightness, self.body.pick);
    }

    /// Damps the note's string.
    pub(crate) fn note_off(&mut self, note: u8) {
        for voice in self.voices.iter_mut().filter(|voice| voice.active && voice.held && voice.note == note) {
            voice.held = false;
            voice.loss = loss_for(DAMPED_T60, voice.frequency);
        }
    }

    pub(crate) fn all_notes_off(&mut self) {
        for voice in self.voices.iter_mut().filter(|voice| voice.active) {
            voice.held = false;
            voice.loss = loss_for(DAMPED_T60, voice.frequency);
        }
    }

    pub(crate) fn next_sample(&mut self, settings: &Settings) -> f32 {
        let mut dry = 0.0;
        for voice in self.voices.iter_mut().filter(|voice| voice.active) {
            dry += voice.next();
        }
        let resonance: f32 = self.resonators.iter_mut().map(|resonator| resonator.process(dry)).sum();
        overdrive((dry + settings.body * resonance) * VOICE_LEVEL, settings.drive) * settings.gain
    }

    #[cfg(test)]
    pub(crate) fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }
}

/// White noise in `-1..1` (xorshift32: cheap, no allocation, the same sequence every run).
fn noise(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32 / u32::MAX as f32) * 2.0 - 1.0
}
