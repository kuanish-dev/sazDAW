//! Synth: the built-in polyphonic subtractive synth (8 voices; waveform, AHDSR envelope, low cut and high cut filters, volume).
//! Of the MIDI controllers it understands pitch bend (±2 semitones) and the mod wheel (CC1 opens the filter).
//! The sound is in `engine`; this file is the `Instrument` glue: parameters in real units, smoothing of the cuts and
//! volume (so knob moves and automation do not click), notes and controllers.

mod engine;

use daw_contracts::{
    AudioBlock, CC_MODULATION, ContractError, ControllerKind, Instrument, InstrumentFactory, L10n, ParamInfo, ParamKind, Params,
    PluginState, SYNTH_KIND, StreamConfig,
};

use engine::{Engine, Settings, Waveform};

/// How long cutoff and volume glide to a new value.
const SMOOTH_SECONDS: f32 = 0.02;

const WAVEFORMS: [L10n; 4] = [
    L10n { en: "Sine", ru: "Синус" },
    L10n { en: "Triangle", ru: "Треугольник" },
    L10n { en: "Saw", ru: "Пила" },
    L10n { en: "Square", ru: "Меандр" },
];

/// Envelope names follow the convention the interface draws as an envelope graph (attack/hold/decay/sustain/release;
/// sustain is a level `0..1`).
const PARAMS: [ParamInfo; 9] = [
    ParamInfo {
        name: "waveform",
        label: L10n { en: "Waveform", ru: "Форма волны" },
        min: 0.0,
        max: 3.0,
        default: 2.0,
        kind: ParamKind::Choice,
        choices: &WAVEFORMS,
    },
    ParamInfo {
        name: "attack",
        label: L10n { en: "Attack, s", ru: "Attack, с" },
        min: 0.0005,
        max: 4.0,
        default: 0.005,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "hold",
        label: L10n { en: "Hold, s", ru: "Hold, с" },
        min: 0.0,
        max: 4.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "decay",
        label: L10n { en: "Decay, s", ru: "Decay, с" },
        min: 0.0005,
        max: 4.0,
        default: 0.15,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "sustain",
        label: L10n { en: "Sustain", ru: "Sustain" },
        min: 0.0,
        max: 1.0,
        default: 0.6,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "release",
        label: L10n { en: "Release, s", ru: "Release, с" },
        min: 0.0005,
        max: 8.0,
        default: 0.12,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "low_cut",
        label: L10n { en: "Low cut, Hz", ru: "Срез низов, Гц" },
        min: 20.0,
        max: 2_000.0,
        default: 20.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "high_cut",
        label: L10n { en: "High cut, Hz", ru: "Срез верхов, Гц" },
        min: 40.0,
        max: 18_000.0,
        default: 1_800.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "gain",
        label: L10n { en: "Volume, %", ru: "Громкость, %" },
        min: 0.0,
        max: 100.0,
        default: 25.0,
        kind: ParamKind::Number,
        choices: &[],
    },
];

/// A value that glides linearly to its target over a fixed number of frames.
#[derive(Clone, Copy)]
struct Ramp {
    value: f32,
    target: f32,
    step: f32,
    left: u32,
}

impl Ramp {
    fn at(value: f32) -> Self {
        Self { value, target: value, step: 0.0, left: 0 }
    }

    fn glide_to(&mut self, target: f32, frames: u32) {
        self.target = target;
        self.left = frames.max(1);
        self.step = (target - self.value) / self.left as f32;
    }

    fn jump(&mut self) {
        *self = Self::at(self.target);
    }

    fn next(&mut self) -> f32 {
        if self.left > 0 {
            self.left -= 1;
            self.value = if self.left == 0 { self.target } else { self.value + self.step };
        }
        self.value
    }
}

struct Synth {
    engine: Engine,
    /// The current values; the cut frequencies and `gain` are rewritten every frame from the ramps.
    settings: Settings,
    /// The cuts glide in octaves (log2 of hertz): equal knob moves sound equal at any height.
    low_cut_octaves: Ramp,
    high_cut_octaves: Ramp,
    gain: Ramp,
    smooth_frames: u32,
}

impl Synth {
    fn new(params: &Params) -> Self {
        let mut synth = Self {
            engine: Engine::default(),
            settings: Settings::default(),
            low_cut_octaves: Ramp::at(0.0),
            high_cut_octaves: Ramp::at(0.0),
            gain: Ramp::at(0.0),
            smooth_frames: smooth_frames(StreamConfig::DEFAULT.sample_rate as f32),
        };
        for info in &PARAMS {
            synth.set_param(info.name, info.default);
        }
        for (name, value) in params {
            synth.set_param(name, *value);
        }
        // A new instrument starts at its values, not gliding from zero.
        synth.low_cut_octaves.jump();
        synth.high_cut_octaves.jump();
        synth.gain.jump();
        synth
    }
}

fn smooth_frames(sample_rate: f32) -> u32 {
    (SMOOTH_SECONDS * sample_rate) as u32
}

impl Instrument for Synth {
    fn prepare(&mut self, config: StreamConfig) {
        let sample_rate = config.sample_rate as f32;
        self.engine.prepare(sample_rate);
        self.smooth_frames = smooth_frames(sample_rate);
        self.low_cut_octaves.jump();
        self.high_cut_octaves.jump();
        self.gain.jump();
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.engine.note_on(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.engine.note_off(note, self.settings.release);
    }

    fn all_notes_off(&mut self) {
        self.engine.all_notes_off(self.settings.release);
    }

    fn controller(&mut self, kind: ControllerKind, value: f32) {
        match kind {
            ControllerKind::PitchBend => self.engine.set_pitch_bend(value),
            ControllerKind::Cc { number: CC_MODULATION } => self.engine.set_modulation(value),
            // The rest has no meaning for this synth: volume comes from the track, aftertouch is not used.
            ControllerKind::Cc { .. } | ControllerKind::Aftertouch => {}
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some(info) = PARAMS.iter().find(|p| p.name == name) else {
            return;
        };
        let value = info.fit(value);
        let s = &mut self.settings;
        match name {
            "waveform" => s.waveform = Waveform::from_index(value),
            "attack" => s.attack = value,
            "hold" => s.hold = value,
            "decay" => s.decay = value,
            "sustain" => s.sustain = value,
            "release" => s.release = value,
            "low_cut" => self.low_cut_octaves.glide_to(value.log2(), self.smooth_frames),
            "high_cut" => self.high_cut_octaves.glide_to(value.log2(), self.smooth_frames),
            "gain" => self.gain.glide_to(value / 100.0, self.smooth_frames),
            _ => {}
        }
    }

    fn render(&mut self, block: &mut AudioBlock) {
        let channels = block.channels();
        for frame in block.as_mut_slice().chunks_exact_mut(channels) {
            self.settings.low_cut_hz = self.low_cut_octaves.next().exp2();
            self.settings.high_cut_hz = self.high_cut_octaves.next().exp2();
            self.settings.gain = self.gain.next();
            let sample = self.engine.next_sample(&self.settings);
            for out in frame {
                *out += sample;
            }
        }
    }
}

struct SynthFactory;

impl InstrumentFactory for SynthFactory {
    fn kind(&self) -> &'static str {
        SYNTH_KIND
    }

    fn label(&self) -> L10n {
        L10n { en: "Synth", ru: "Синт" }
    }

    fn describe(&self) -> Vec<ParamInfo> {
        PARAMS.to_vec()
    }

    // A built-in instrument has no state of its own: everything is in the parameters.
    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        Ok(Box::new(Synth::new(params)))
    }
}

pub fn factory() -> Box<dyn InstrumentFactory> {
    Box::new(SynthFactory)
}

#[cfg(test)]
mod tests;
