//! Synth: the built-in polyphonic subtractive synth. Two oscillators (sine, triangle, saw, square, noise) with up to
//! seven detuned unison copies across the stereo field, an AHDSR volume envelope, a resonant lowpass with its own
//! envelope, a low cut, a pitch envelope, mono mode with glide and an LFO (pitch, filter, volume; free or synced to
//! the song). Of the MIDI controllers it understands pitch bend (±2 semitones) and the mod wheel (CC1 opens the filter).
//! The sound is in `engine` and its parts; this file is the `Instrument` glue: parameters in real units, smoothing of the
//! cuts and volume (so knob moves and automation do not click), notes, controllers and the song tempo.

mod engine;
mod envelope;
mod filter;
mod lfo;
mod osc;

use daw_contracts::{
    AudioBlock, BlockTiming, CC_MODULATION, ContractError, ControllerKind, Instrument, InstrumentFactory, L10n, PPQ, ParamInfo, ParamKind,
    Params, PluginState, SYNTH_KIND, StreamConfig,
};

use engine::{Engine, Settings, Unison};
use lfo::LfoShape;
use osc::Waveform;

/// How long cutoff and volume glide to a new value.
const SMOOTH_SECONDS: f32 = 0.02;

const WAVEFORMS: [L10n; 5] = [
    L10n { en: "Sine", ru: "Синус" },
    L10n { en: "Triangle", ru: "Треугольник" },
    L10n { en: "Saw", ru: "Пила" },
    L10n { en: "Square", ru: "Меандр" },
    L10n { en: "Noise", ru: "Шум" },
];

const LFO_SHAPES: [L10n; 5] = [
    L10n { en: "Sine", ru: "Синус" },
    L10n { en: "Triangle", ru: "Треугольник" },
    L10n { en: "Saw", ru: "Пила" },
    L10n { en: "Square", ru: "Меандр" },
    L10n { en: "Random", ru: "Случайная" },
];

/// "Off", then the note lengths of `lfo::SYNC_BEATS` in the same order.
const LFO_SYNC: [L10n; 9] = [
    L10n { en: "Off", ru: "Выкл" },
    L10n::same("1/1"),
    L10n::same("1/2"),
    L10n::same("1/4"),
    L10n::same("1/8"),
    L10n::same("1/16"),
    L10n { en: "1/4 triplet", ru: "1/4 триоль" },
    L10n { en: "1/8 triplet", ru: "1/8 триоль" },
    L10n { en: "1/16 triplet", ru: "1/16 триоль" },
];

const fn number(name: &'static str, label: L10n, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { name, label, min, max, default, kind: ParamKind::Number, choices: &[] }
}

const fn choice(name: &'static str, label: L10n, choices: &'static [L10n], max: f32, default: f32) -> ParamInfo {
    ParamInfo { name, label, min: 0.0, max, default, kind: ParamKind::Choice, choices }
}

/// Grouped as the instrument window shows them: oscillators, unison, volume envelope, filter, pitch, mono, LFO, volume.
/// Envelope names follow the convention the interface draws as an envelope graph (attack/hold/decay/sustain/release;
/// sustain is a level `0..1`).
const PARAMS: [ParamInfo; 30] = [
    choice("waveform", L10n { en: "Waveform", ru: "Форма волны" }, &WAVEFORMS, 4.0, 2.0),
    number("osc2_level", L10n { en: "Osc 2 level, %", ru: "Осц. 2: уровень, %" }, 0.0, 100.0, 0.0),
    choice("osc2_waveform", L10n { en: "Osc 2 waveform", ru: "Осц. 2: форма волны" }, &WAVEFORMS, 4.0, 2.0),
    number("osc2_semitones", L10n { en: "Osc 2 pitch, semitones", ru: "Осц. 2: высота, полутоны" }, -24.0, 24.0, 0.0),
    number("osc2_detune", L10n { en: "Osc 2 detune, cents", ru: "Осц. 2: расстройка, центы" }, -50.0, 50.0, 0.0),
    number("unison", L10n { en: "Unison voices", ru: "Голоса унисона" }, 1.0, 7.0, 1.0),
    number("unison_detune", L10n { en: "Unison detune, cents", ru: "Расстройка унисона, центы" }, 0.0, 100.0, 20.0),
    number("unison_width", L10n { en: "Unison width, %", ru: "Ширина унисона, %" }, 0.0, 100.0, 50.0),
    number("attack", L10n { en: "Attack, s", ru: "Attack, с" }, 0.0005, 4.0, 0.005),
    number("hold", L10n { en: "Hold, s", ru: "Hold, с" }, 0.0, 4.0, 0.0),
    number("decay", L10n { en: "Decay, s", ru: "Decay, с" }, 0.0005, 4.0, 0.15),
    number("sustain", L10n::same("Sustain"), 0.0, 1.0, 0.6),
    number("release", L10n { en: "Release, s", ru: "Release, с" }, 0.0005, 8.0, 0.12),
    number("low_cut", L10n { en: "Low cut, Hz", ru: "Срез низов, Гц" }, 20.0, 2_000.0, 20.0),
    number("high_cut", L10n { en: "High cut, Hz", ru: "Срез верхов, Гц" }, 40.0, 18_000.0, 1_800.0),
    number("resonance", L10n { en: "Resonance, %", ru: "Резонанс, %" }, 0.0, 100.0, 0.0),
    number("filter_env", L10n { en: "Filter envelope, octaves", ru: "Огибающая фильтра, октавы" }, -6.0, 6.0, 0.0),
    number("filter_attack", L10n { en: "Filter attack, s", ru: "Атака фильтра, с" }, 0.0005, 4.0, 0.005),
    number("filter_decay", L10n { en: "Filter decay, s", ru: "Спад фильтра, с" }, 0.0005, 4.0, 0.3),
    number("pitch_env", L10n { en: "Pitch envelope, semitones", ru: "Огибающая высоты, полутоны" }, -48.0, 48.0, 0.0),
    number("pitch_decay", L10n { en: "Pitch decay, s", ru: "Спад высоты, с" }, 0.001, 2.0, 0.08),
    ParamInfo { kind: ParamKind::Switch, ..number("mono", L10n { en: "Mono", ru: "Моно" }, 0.0, 1.0, 0.0) },
    number("glide", L10n { en: "Glide, s", ru: "Глайд, с" }, 0.0, 2.0, 0.0),
    choice("lfo_shape", L10n { en: "LFO shape", ru: "LFO: форма" }, &LFO_SHAPES, 4.0, 0.0),
    number("lfo_rate", L10n { en: "LFO rate, Hz", ru: "LFO: скорость, Гц" }, 0.05, 20.0, 5.0),
    choice("lfo_sync", L10n { en: "LFO tempo sync", ru: "LFO: синхронизация с темпом" }, &LFO_SYNC, 8.0, 0.0),
    number("lfo_pitch", L10n { en: "LFO to pitch, cents", ru: "LFO → высота, центы" }, 0.0, 100.0, 0.0),
    number("lfo_filter", L10n { en: "LFO to filter, octaves", ru: "LFO → фильтр, октавы" }, 0.0, 4.0, 0.0),
    number("lfo_volume", L10n { en: "LFO to volume, %", ru: "LFO → громкость, %" }, 0.0, 100.0, 0.0),
    number("gain", L10n { en: "Volume, %", ru: "Громкость, %" }, 0.0, 100.0, 25.0),
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
    /// The unison knobs (voices, detune in cents, width `0..=1`): the copies are rebuilt from all three when one moves.
    unison: [f32; 3],
    /// Osc 2 offset: semitones and cents.
    osc2_pitch: [f32; 2],
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
            unison: [1.0, 0.0, 0.0],
            osc2_pitch: [0.0; 2],
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

fn unison([voices, detune, width]: [f32; 3]) -> Unison {
    Unison::new(voices, detune, width)
}

fn osc2_ratio([semitones, cents]: [f32; 2]) -> f32 {
    ((semitones + cents / 100.0) / 12.0).exp2()
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
        self.engine.note_on(note, velocity, &self.settings);
    }

    fn note_off(&mut self, note: u8) {
        self.engine.note_off(note, &self.settings);
    }

    fn all_notes_off(&mut self) {
        self.engine.all_notes_off(self.settings.envelope.release);
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
            "osc2_level" => s.osc2_level = value / 100.0,
            "osc2_waveform" => s.osc2 = Waveform::from_index(value),
            "osc2_semitones" | "osc2_detune" => {
                self.osc2_pitch[usize::from(name == "osc2_detune")] = value;
                s.osc2_ratio = osc2_ratio(self.osc2_pitch);
            }
            "unison" | "unison_detune" | "unison_width" => {
                match name {
                    "unison" => self.unison[0] = value,
                    "unison_detune" => self.unison[1] = value,
                    _ => self.unison[2] = value / 100.0,
                }
                s.unison = unison(self.unison);
            }
            "attack" => s.envelope.attack = value,
            "hold" => s.envelope.hold = value,
            "decay" => s.envelope.decay = value,
            "sustain" => s.envelope.sustain = value,
            "release" => s.envelope.release = value,
            "low_cut" => self.low_cut_octaves.glide_to(value.log2(), self.smooth_frames),
            "high_cut" => self.high_cut_octaves.glide_to(value.log2(), self.smooth_frames),
            "resonance" => s.resonance = value / 100.0,
            "filter_env" => s.filter_env_octaves = value,
            "filter_attack" => s.filter_attack = value,
            "filter_decay" => s.filter_decay = value,
            "pitch_env" => s.pitch_env_semitones = value,
            "pitch_decay" => s.pitch_decay = value,
            "mono" => s.mono = value >= 0.5,
            "glide" => s.glide = value,
            "lfo_shape" => s.lfo.shape = LfoShape::from_index(value),
            "lfo_rate" => s.lfo.rate_hz = value,
            "lfo_sync" => s.lfo.sync_beats = lfo::sync_beats(value),
            "lfo_pitch" => s.lfo.pitch_cents = value,
            "lfo_filter" => s.lfo.filter_octaves = value,
            "lfo_volume" => s.lfo.volume = value / 100.0,
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
            let [left, right] = self.engine.next_frame(&self.settings);
            match frame {
                [mono] => *mono += 0.5 * (left + right),
                [l, r, rest @ ..] => {
                    *l += left;
                    *r += right;
                    for out in rest {
                        *out += 0.5 * (left + right);
                    }
                }
                [] => {}
            }
        }
    }

    fn set_timing(&mut self, timing: &BlockTiming) {
        let ppq = f64::from(PPQ);
        self.engine.set_timing(timing.start_tick / ppq, timing.ticks_per_frame / ppq, timing.playing, self.settings.lfo.sync_beats);
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
