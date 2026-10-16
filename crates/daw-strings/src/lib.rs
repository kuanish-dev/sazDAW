//! Plucked strings: the built-in acoustic guitar, electric guitar, ukulele and bass guitar (`GUITAR_KIND`,
//! `ELECTRIC_GUITAR_KIND`, `UKULELE_KIND`, `BASS_GUITAR_KIND`). One physical model (`engine`: extended Karplus–Strong,
//! 12 voices) and kinds that differ in body (or pickup) resonances, pick position and default parameters; the electric
//! kinds also have an amp's Drive. This file is the `Instrument` glue: parameters in real units, volume, body and drive
//! smoothed so knob moves and automation do not click, notes.
//! A string's period is set when it is plucked, so controllers (pitch bend included) are ignored, and `ring` and `tone`
//! apply from the next pluck.

mod engine;
#[cfg(test)]
mod tests;

use daw_contracts::{
    AudioBlock, BASS_GUITAR_KIND, ContractError, ControllerKind, ELECTRIC_GUITAR_KIND, GUITAR_KIND, Instrument, InstrumentFactory, L10n,
    ParamInfo, ParamKind, Params, PluginState, StreamConfig, UKULELE_KIND,
};

use engine::{Body, Engine, Settings};

/// How long volume, body and drive take to follow a knob.
const SMOOTH_SECONDS: f32 = 0.02;

/// An instrument kind: its body and the default `ring` (s), `tone`, `body` and `gain` (%).
#[derive(Clone, Copy)]
struct Kind {
    id: &'static str,
    label: L10n,
    body: Body,
    defaults: [f32; 4],
    /// The default `drive` (%) of a kind played through an amp; `None`: no Drive knob (an acoustic instrument).
    drive: Option<f32>,
}

const KINDS: [Kind; 4] = [
    Kind {
        id: GUITAR_KIND,
        label: L10n { en: "Acoustic guitar", ru: "Акустическая гитара" },
        body: Body { modes: [(100.0, 3.0), (205.0, 5.0)], pick: 0.13 },
        defaults: [4.0, 55.0, 50.0, 60.0],
        drive: None,
    },
    // No hollow body: a magnetic pickup's resonance (a bright peak near 3 kHz) and a little low end; picked nearer the
    // bridge and with a longer sustain than the acoustic. Clean by default: Drive turns it into crunch and fuzz.
    Kind {
        id: ELECTRIC_GUITAR_KIND,
        label: L10n { en: "Electric guitar", ru: "Электрогитара" },
        body: Body { modes: [(120.0, 1.2), (3_200.0, 2.0)], pick: 0.1 },
        defaults: [7.0, 60.0, 40.0, 55.0],
        drive: Some(0.0),
    },
    Kind {
        id: UKULELE_KIND,
        label: L10n { en: "Ukulele", ru: "Укулеле" },
        body: Body { modes: [(270.0, 3.0), (540.0, 4.0)], pick: 0.2 },
        defaults: [1.8, 65.0, 45.0, 60.0],
        drive: None,
    },
    // An electric bass has no hollow body: its "body" is the pickup and cabinet, a low and a mid bump.
    Kind {
        id: BASS_GUITAR_KIND,
        label: L10n { en: "Bass guitar", ru: "Бас-гитара" },
        body: Body { modes: [(70.0, 1.0), (750.0, 1.5)], pick: 0.1 },
        defaults: [5.0, 30.0, 15.0, 70.0],
        drive: Some(0.0),
    },
];

fn number(name: &'static str, label: L10n, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { name, label, min, max, default, kind: ParamKind::Number, choices: &[] }
}

fn params(kind: &Kind) -> Vec<ParamInfo> {
    let [ring, tone, body, gain] = kind.defaults;
    let mut params = vec![
        number("ring", L10n { en: "Ring, s", ru: "Звучание, с" }, 0.2, 12.0, ring),
        number("tone", L10n { en: "Brightness, %", ru: "Яркость, %" }, 0.0, 100.0, tone),
        number("body", L10n { en: "Body, %", ru: "Корпус, %" }, 0.0, 100.0, body),
        number("gain", L10n { en: "Volume, %", ru: "Громкость, %" }, 0.0, 100.0, gain),
    ];
    if let Some(drive) = kind.drive {
        params.push(number("drive", L10n { en: "Drive, %", ru: "Перегруз, %" }, 0.0, 100.0, drive));
    }
    params
}

struct Strings {
    engine: Engine,
    params: Vec<ParamInfo>,
    /// The current values; `body`, `drive` and `gain` move towards their targets every frame.
    settings: Settings,
    body_target: f32,
    drive_target: f32,
    gain_target: f32,
    /// One-pole smoothing coefficient per frame.
    smoothing: f32,
}

impl Strings {
    fn new(kind: &Kind, params: &Params) -> Self {
        let mut strings = Self {
            engine: Engine::new(kind.body),
            params: self::params(kind),
            settings: Settings::default(),
            body_target: 0.0,
            drive_target: 0.0,
            gain_target: 0.0,
            smoothing: smoothing(StreamConfig::DEFAULT.sample_rate as f32),
        };
        for info in strings.params.clone() {
            strings.set_param(info.name, info.default);
        }
        for (name, value) in params {
            strings.set_param(name, *value);
        }
        // A new instrument starts at its values, not gliding from zero.
        strings.jump();
        strings
    }

    fn jump(&mut self) {
        self.settings.body = self.body_target;
        self.settings.drive = self.drive_target;
        self.settings.gain = self.gain_target;
    }
}

fn smoothing(sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (SMOOTH_SECONDS * sample_rate)).exp()
}

impl Instrument for Strings {
    fn prepare(&mut self, config: StreamConfig) {
        let sample_rate = config.sample_rate as f32;
        self.engine.prepare(sample_rate);
        self.smoothing = smoothing(sample_rate);
        self.jump();
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.engine.note_on(note, velocity, &self.settings);
    }

    fn note_off(&mut self, note: u8) {
        self.engine.note_off(note);
    }

    fn all_notes_off(&mut self) {
        self.engine.all_notes_off();
    }

    fn controller(&mut self, _kind: ControllerKind, _value: f32) {}

    fn set_param(&mut self, name: &str, value: f32) {
        let Some(info) = self.params.iter().find(|p| p.name == name) else {
            return;
        };
        let value = info.fit(value);
        match name {
            "ring" => self.settings.ring = value,
            "tone" => self.settings.tone = value / 100.0,
            "body" => self.body_target = value / 100.0,
            "drive" => self.drive_target = value / 100.0,
            "gain" => self.gain_target = value / 100.0,
            _ => {}
        }
    }

    fn render(&mut self, block: &mut AudioBlock) {
        let channels = block.channels();
        for frame in block.as_mut_slice().chunks_exact_mut(channels) {
            self.settings.body += (self.body_target - self.settings.body) * self.smoothing;
            self.settings.drive += (self.drive_target - self.settings.drive) * self.smoothing;
            self.settings.gain += (self.gain_target - self.settings.gain) * self.smoothing;
            let sample = self.engine.next_sample(&self.settings);
            for out in frame {
                *out += sample;
            }
        }
    }
}

struct StringsFactory {
    kind: Kind,
}

impl InstrumentFactory for StringsFactory {
    fn kind(&self) -> &'static str {
        self.kind.id
    }

    fn label(&self) -> L10n {
        self.kind.label
    }

    fn describe(&self) -> Vec<ParamInfo> {
        params(&self.kind)
    }

    // A built-in instrument has no state of its own: everything is in the parameters.
    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        Ok(Box::new(Strings::new(&self.kind, params)))
    }
}

/// Factories of all string kinds (see `KINDS`).
pub fn factories() -> Vec<Box<dyn InstrumentFactory>> {
    KINDS.into_iter().map(|kind| Box::new(StringsFactory { kind }) as Box<dyn InstrumentFactory>).collect()
}
