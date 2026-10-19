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

/// An instrument kind: its body, the default `ring` (s), `tone`, `body` and `gain` (%) and what it sounds like.
#[derive(Clone, Copy)]
struct Kind {
    id: &'static str,
    label: L10n,
    body: Body,
    defaults: [f32; 4],
    /// The default `drive` (%) of a kind played through an amp; `None`: no Drive knob (an acoustic instrument).
    drive: Option<f32>,
    /// `InstrumentFactory::about`: the kind's sound, then how every kind plays notes (`plays_notes!`).
    about: L10n,
}

/// How every string kind plays notes: the end of each kind's `about` (`concat!` needs literals, so a macro, not a
/// `const`). Describes `engine.rs`: keep it in step with it.
macro_rules! plays_notes {
    (en) => {
        "A physical model of a plucked string, 12 voices. A note is a pluck that rings while it is held (see ring); ending the \
         note damps the string within about 0.1 s, so note length matters. Velocity (1-127) changes the loudness on a curve (64 is \
         about a third as loud as 127) and the brightness (softer is darker). Repeating a note re-plucks its own string. Pitch \
         bend and other controllers are ignored."
    };
    (ru) => {
        "Физическая модель щипковой струны, 12 голосов. Нота — это щипок, который звучит, пока нота удерживается (см. ring); конец \
         ноты глушит струну примерно за 0,1 с, так что длина ноты важна. Сила нажатия (velocity, 1-127) меняет громкость по кривой \
         (64 примерно втрое тише, чем 127) и яркость (тише — темнее). Повтор ноты заново дёргает ту же струну. Pitch bend и другие \
         контроллеры не действуют."
    };
}

const KINDS: [Kind; 4] = [
    Kind {
        id: GUITAR_KIND,
        label: L10n { en: "Acoustic guitar", ru: "Акустическая гитара" },
        body: Body { modes: [(100.0, 3.0), (205.0, 5.0)], pick: 0.13 },
        defaults: [4.0, 55.0, 50.0, 60.0],
        drive: None,
        about: L10n {
            en: concat!(
                "An acoustic steel-string guitar: a warm wooden body (resonances near 100 and 205 Hz), rings about 4 s.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Акустическая гитара со стальными струнами: тёплый деревянный корпус (резонансы около 100 и 205 Гц), звучит около \
                 4 с.",
                plays_notes!(ru)
            ),
        },
    },
    // No hollow body: a magnetic pickup's resonance (a bright peak near 3 kHz) and a little low end; picked nearer the
    // bridge and with a longer sustain than the acoustic. Clean by default: Drive turns it into crunch and fuzz.
    Kind {
        id: ELECTRIC_GUITAR_KIND,
        label: L10n { en: "Electric guitar", ru: "Электрогитара" },
        body: Body { modes: [(120.0, 1.2), (3_200.0, 2.0)], pick: 0.1 },
        defaults: [7.0, 60.0, 40.0, 55.0],
        drive: Some(0.0),
        about: L10n {
            en: concat!(
                "A clean electric guitar: no hollow body but a bright pickup peak near 3.2 kHz, picked near the bridge, rings \
                 about 7 s; drive turns it into crunch and fuzz.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Чистая электрогитара: без полого корпуса, но с ярким пиком звукоснимателя около 3,2 кГц, щипок ближе к подставке, \
                 звучит около 7 с; drive превращает её в кранч и фузз.",
                plays_notes!(ru)
            ),
        },
    },
    Kind {
        id: UKULELE_KIND,
        label: L10n { en: "Ukulele", ru: "Укулеле" },
        body: Body { modes: [(270.0, 3.0), (540.0, 4.0)], pick: 0.2 },
        defaults: [1.8, 65.0, 45.0, 60.0],
        drive: None,
        about: L10n {
            en: concat!("A ukulele: a small bright body (resonances near 270 and 540 Hz) and a short ring, about 1.8 s.", plays_notes!(en)),
            ru: concat!(
                "Укулеле: маленький яркий корпус (резонансы около 270 и 540 Гц) и короткое звучание, около 1,8 с.",
                plays_notes!(ru)
            ),
        },
    },
    // An electric bass has no hollow body: its "body" is the pickup and cabinet, a low and a mid bump.
    Kind {
        id: BASS_GUITAR_KIND,
        label: L10n { en: "Bass guitar", ru: "Бас-гитара" },
        body: Body { modes: [(70.0, 1.0), (750.0, 1.5)], pick: 0.1 },
        defaults: [5.0, 30.0, 15.0, 70.0],
        drive: Some(0.0),
        about: L10n {
            en: concat!(
                "An electric bass guitar: a dark tone with pickup and cabinet bumps near 70 Hz and 750 Hz, rings about 5 s; drive \
                 adds grit. A real 4-string bass goes down to E1 (MIDI 28).",
                plays_notes!(en)
            ),
            ru: concat!(
                "Бас-гитара: тёмный тон с подъёмами звукоснимателя и кабинета около 70 и 750 Гц, звучит около 5 с; drive добавляет \
                 грязи. Настоящий 4-струнный бас звучит до E1 (MIDI 28).",
                plays_notes!(ru)
            ),
        },
    },
];

fn number(name: &'static str, label: L10n, min: f32, max: f32, default: f32, about: L10n) -> ParamInfo {
    ParamInfo { name, label, min, max, default, kind: ParamKind::Number, choices: &[], about }
}

/// The parameters of `kind`. Each `about` says what the parameter does to the sound, for the AI that cannot hear it: a
/// change to the sound in `engine.rs` needs the same change in the text.
fn params(kind: &Kind) -> Vec<ParamInfo> {
    let [ring, tone, body, gain] = kind.defaults;
    let mut params = vec![
        number(
            "ring",
            L10n { en: "Ring, s", ru: "Звучание, с" },
            0.2,
            12.0,
            ring,
            L10n {
                en: "Seconds an open string near G3 (196 Hz) rings until -60 dB while the note is held; higher notes ring shorter, \
                     lower ones longer (by the square root of the pitch ratio, at most 3× this). Ending the note damps the string \
                     at once. Applies from the next pluck.",
                ru: "Сколько секунд открытая струна около G3 (196 Гц) звучит до -60 дБ, пока нота удерживается; высокие ноты \
                     звучат короче, низкие — дольше (по корню из отношения высот, не больше 3× от этого). Конец ноты сразу глушит \
                     струну. Действует со следующего щипка.",
            },
        ),
        number(
            "tone",
            L10n { en: "Brightness, %", ru: "Яркость, %" },
            0.0,
            100.0,
            tone,
            L10n {
                en: "Brightness of the pluck and of the ringing string: 0 % dark and dull, 100 % bright and twangy; harder notes \
                     (velocity) are brighter too. Applies from the next pluck.",
                ru: "Яркость щипка и звучащей струны: 0 % — тёмный, глухой звук, 100 % — яркий, звонкий; сильнее взятые ноты \
                     (velocity) тоже ярче. Действует со следующего щипка.",
            },
        ),
        number(
            "body",
            L10n { en: "Body, %", ru: "Корпус, %" },
            0.0,
            100.0,
            body,
            L10n {
                en: "How much of the body resonances is mixed in (the wood of an acoustic, the pickup of an electric): 0 % a bare \
                     string, 100 % a full, boomy body.",
                ru: "Сколько резонансов корпуса подмешано (дерево акустики, звукосниматель электрогитары): 0 % — голая струна, 100 \
                     % — полный, гулкий корпус.",
            },
        ),
        number(
            "gain",
            L10n { en: "Volume, %", ru: "Громкость, %" },
            0.0,
            100.0,
            gain,
            L10n { en: "Output level, 0-100 % (linear).", ru: "Выходной уровень, 0-100 % (линейно)." },
        ),
    ];
    if let Some(drive) = kind.drive {
        params.push(number(
            "drive",
            L10n { en: "Drive, %", ru: "Перегруз, %" },
            0.0,
            100.0,
            drive,
            L10n {
                en: "Amp overdrive: 0 % clean, 30-60 % crunch, 100 % fuzz. Loud notes and chords are squashed into distortion \
                     while quiet playing keeps its level, so the loudness stays near the clean sound's.",
                ru: "Перегруз усилителя: 0 % — чисто, 30-60 % — кранч, 100 % — фузз. Громкие ноты и аккорды сжимаются в перегруз, \
                     а тихая игра сохраняет уровень, так что громкость остаётся близкой к чистому звуку.",
            },
        ));
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

    fn about(&self) -> L10n {
        self.kind.about
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
