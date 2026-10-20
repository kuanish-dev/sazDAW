//! Sampler: plays a loaded sound (whole or a chosen region, forwards or backwards) on notes, with pitch following the
//! note. The file is not copied: the region is set by the `start` and `end` parameters.
//! Of the MIDI controllers it understands pitch bend (±2 semitones, the default of most synths) and the sustain pedal
//! (CC64).

use std::sync::Arc;

use daw_contracts::codes::samples;
use daw_contracts::{
    AudioBlock, CC_SUSTAIN, ContractError, ControllerKind, Instrument, InstrumentFactory, L10n, ParamInfo, ParamKind, Params, PluginState,
    SAMPLER_KIND, SAMPLER_ROOT_PARAM, SAMPLER_SAMPLE_PARAM, SampleData, SampleStore, StreamConfig,
};

const VOICES: usize = 16;
/// How far pitch bend at the end of the wheel shifts the pitch, in semitones.
const BEND_RANGE_SEMITONES: f32 = 2.0;

/// What the sampler as a whole does with notes (`InstrumentFactory::about`); the parameters explain themselves in
/// `PARAMS`.
const ABOUT: L10n = L10n {
    en: "Plays a sound from the project's sample library (the sample param) on notes, 16 voices. The root note plays the file at \
         its own pitch; each semitone above or below plays it faster or slower, so pitch and length change together, like a \
         record. Velocity (1-127) scales the loudness linearly: 64 is half as loud as 127. With one_shot on (the default) the \
         whole region plays whatever the note length; off, the note's end fades it over release. Pitch bend is ±2 semitones, the \
         sustain pedal (CC64) holds released notes. Without a sample it is silent.",
    ru: "Играет звук из библиотеки сэмплов проекта (параметр sample) по нотам, 16 голосов. Корневая нота играет файл в его \
         собственной высоте; каждый полутон выше или ниже играет его быстрее или медленнее, так что высота и длина меняются \
         вместе, как у пластинки. Сила нажатия (velocity, 1-127) меняет громкость линейно: 64 вдвое тише, чем 127. При включённом \
         one_shot (по умолчанию) область звучит целиком при любой длине ноты; при выключенном конец ноты гасит звук за release. \
         Pitch bend — ±2 полутона, педаль сустейна (CC64) держит отпущенные ноты. Без сэмпла молчит.",
};

/// Each `about` says what the parameter does to the sound, for the AI that cannot hear it: a change to the playback in
/// this file needs the same change in the text.
const PARAMS: [ParamInfo; 10] = [
    ParamInfo {
        name: SAMPLER_SAMPLE_PARAM,
        label: L10n { en: "Sample", ru: "Сэмпл" },
        min: 0.0,
        max: 1_000_000.0,
        default: 0.0,
        kind: ParamKind::Sample,
        choices: &[],
        about: L10n {
            en: "Which sound plays: the id of a sample in the project library (samples.list, samples.import); 0 is none, and the \
                 sampler is silent. Changing it recreates the instrument.",
            ru: "Какой звук играет: id сэмпла из библиотеки проекта (samples.list, samples.import); 0 — никакого, сэмплер молчит. \
                 Смена пересоздаёт инструмент.",
        },
    },
    ParamInfo {
        name: "start",
        label: L10n { en: "Start", ru: "Начало" },
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Where playback starts, as a share of the file length: 0 is the beginning, 0.5 the middle. The region lies between \
                 start and end in either order.",
            ru: "Где начинается воспроизведение, доля длины файла: 0 — начало, 0,5 — середина. Область лежит между start и end в \
                 любом порядке.",
        },
    },
    ParamInfo {
        name: "end",
        label: L10n { en: "End", ru: "Конец" },
        min: 0.0,
        max: 1.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Where playback stops, as a share of the file length: 1 is the end of the file.",
            ru: "Где заканчивается воспроизведение, доля длины файла: 1 — конец файла.",
        },
    },
    ParamInfo {
        name: "reverse",
        label: L10n { en: "Reverse", ru: "Реверс" },
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Switch,
        choices: &[],
        about: L10n {
            en: "On: plays the region backwards, from end to start.",
            ru: "Вкл: играет область задом наперёд, от конца к началу.",
        },
    },
    ParamInfo {
        name: SAMPLER_ROOT_PARAM,
        label: L10n { en: "Root note", ru: "Корневая нота" },
        min: 0.0,
        max: 127.0,
        default: 60.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "The MIDI note that plays the sample at its recorded pitch and speed (60 = C4). A note 12 semitones higher plays \
                 it twice as fast and an octave up, 12 lower at half speed and an octave down. Set it to the sample's own pitch so \
                 notes play in tune.",
            ru: "MIDI-нота, на которой сэмпл звучит с записанной высотой и скоростью (60 = C4). Нота на 12 полутонов выше играет \
                 его вдвое быстрее и на октаву выше, на 12 ниже — вдвое медленнее и на октаву ниже. Поставьте сюда собственную \
                 высоту сэмпла, чтобы ноты строили.",
        },
    },
    ParamInfo {
        name: "gain",
        label: L10n { en: "Volume", ru: "Громкость" },
        min: 0.0,
        max: 1.5,
        default: 0.8,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Loudness, linear: 1 is the file's own level, 0.5 half (-6 dB), 1.5 the most (+3.5 dB).",
            ru: "Громкость, линейно: 1 — уровень самого файла, 0,5 — вдвое тише (-6 дБ), 1,5 — максимум (+3,5 дБ).",
        },
    },
    ParamInfo {
        name: "attack",
        label: L10n { en: "Attack, s", ru: "Attack, с" },
        min: 0.0005,
        max: 4.0,
        default: 0.002,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Seconds each note takes to fade in from silence (a straight line). Slightly above 0 avoids a click when the \
                 region starts in the middle of a sound.",
            ru: "За сколько секунд каждая нота нарастает из тишины (по прямой). Чуть больше 0 убирает щелчок, когда область \
                 начинается посреди звука.",
        },
    },
    ParamInfo {
        name: "release",
        label: L10n { en: "Release, s", ru: "Release, с" },
        min: 0.0005,
        max: 8.0,
        default: 0.08,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Seconds a note fades out after it ends (a straight line) while one_shot is off; also the fade when playback \
                 stops.",
            ru: "За сколько секунд нота затухает после конца (по прямой), когда one_shot выключен; так же звук затухает при \
                 остановке.",
        },
    },
    ParamInfo {
        name: "loop",
        label: L10n { en: "Loop", ru: "Зацикливание" },
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Switch,
        choices: &[],
        about: L10n {
            en: "On: the region repeats while the note is held, end joined to start without a gap (a sustained pad, a drone); the \
                 note's end fades it over release. With one_shot on as well, a looped note never ends until playback stops, so \
                 turn one_shot off with the loop.",
            ru: "Вкл: область повторяется, пока нота удерживается, конец стыкуется с началом без паузы (тянущийся пэд, дрон); \
                 конец ноты гасит звук за release. Если включён и one_shot, зацикленная нота не кончается до остановки \
                 воспроизведения, поэтому вместе с зацикливанием выключайте one_shot.",
        },
    },
    ParamInfo {
        name: "one_shot",
        label: L10n { en: "Ignore note off", ru: "Игнорировать note off" },
        min: 0.0,
        max: 1.0,
        default: 1.0,
        kind: ParamKind::Switch,
        choices: &[],
        about: L10n {
            en: "On (the default): the note's end is ignored and the region always plays to its end (drum hits, one-shots). Off: \
                 the note's end fades the sound over release, so note length matters.",
            ru: "Вкл (по умолчанию): конец ноты не учитывается, область всегда звучит до конца (удары, ваншоты). Выкл: конец ноты \
                 гасит звук за release, так что длина ноты важна.",
        },
    },
];

#[derive(Clone, Copy, Default)]
struct Settings {
    /// The playback region as fractions of the file length.
    start: f32,
    end: f32,
    reverse: bool,
    root: f32,
    gain: f32,
    attack: f32,
    release: f32,
    looping: bool,
    one_shot: bool,
}

impl Settings {
    /// Defaults from `PARAMS`, overridden by the project parameters.
    fn from_params(params: &Params) -> Self {
        let mut settings = Self::default();
        for info in &PARAMS {
            settings.set(info.name, info.default);
        }
        for (name, value) in params {
            settings.set(name, *value);
        }
        settings
    }

    /// The region bounds in file frames: `lo < hi <= frame_count - 1`, never shorter than one frame.
    fn region(&self, frame_count: usize) -> (f64, f64) {
        let last = (frame_count.max(2) - 1) as f64;
        let lo = (f64::from(self.start.min(self.end)) * last).round();
        let hi = (f64::from(self.start.max(self.end)) * last).round();
        if hi - lo >= 1.0 {
            (lo, hi)
        } else if lo + 1.0 <= last {
            (lo, lo + 1.0)
        } else {
            (last - 1.0, last)
        }
    }

    fn set(&mut self, name: &str, value: f32) {
        let Some(info) = PARAMS.iter().find(|p| p.name == name) else {
            return;
        };
        let value = value.clamp(info.min, info.max);
        match name {
            "start" => self.start = value,
            "end" => self.end = value,
            "reverse" => self.reverse = value >= 0.5,
            SAMPLER_ROOT_PARAM => self.root = value,
            "gain" => self.gain = value,
            "attack" => self.attack = value,
            "release" => self.release = value,
            "loop" => self.looping = value >= 0.5,
            "one_shot" => self.one_shot = value >= 0.5,
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    position: f64,
    increment: f64,
    level: f32,
    velocity: f32,
    active: bool,
    releasing: bool,
    /// The key was released while the pedal was down: the voice fades when the pedal is released.
    sustained: bool,
    age: u64,
}

impl Voice {
    const IDLE: Voice = Voice {
        note: 0,
        position: 0.0,
        increment: 0.0,
        level: 0.0,
        velocity: 0.0,
        active: false,
        releasing: false,
        sustained: false,
        age: 0,
    };
}

struct Sampler {
    data: Option<Arc<SampleData>>,
    settings: Settings,
    sample_rate: f32,
    voices: [Voice; VOICES],
    /// How many notes were played: used to find the oldest voice when none is free.
    note_counter: u64,
    /// How much pitch bend speeds up playback of all voices (1 means the wheel is centred).
    bend_ratio: f64,
    /// The sustain pedal is down.
    sustain: bool,
}

impl Instrument for Sampler {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.voices = [Voice::IDLE; VOICES];
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        let Some(data) = &self.data else {
            return;
        };
        self.note_counter += 1;
        let index = self
            .voices
            .iter()
            .position(|v| !v.active)
            .or_else(|| self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i))
            .unwrap_or(0);
        let semitones = f32::from(note) - self.settings.root;
        let ratio = f64::from(data.sample_rate) / f64::from(self.sample_rate) * 2f64.powf(f64::from(semitones) / 12.0);
        let (lo, hi) = self.settings.region(data.frame_count());
        let reverse = self.settings.reverse;
        self.voices[index] = Voice {
            note,
            position: if reverse { hi } else { lo },
            increment: if reverse { -ratio } else { ratio },
            level: 0.0,
            velocity: velocity.clamp(0.0, 1.0),
            active: true,
            releasing: false,
            sustained: false,
            age: self.note_counter,
        };
    }

    fn note_off(&mut self, note: u8) {
        if self.settings.one_shot {
            return;
        }
        let sustain = self.sustain;
        for voice in &mut self.voices {
            if voice.active && voice.note == note {
                if sustain {
                    voice.sustained = true;
                } else {
                    voice.releasing = true;
                }
            }
        }
    }

    fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            if voice.active {
                voice.releasing = true;
            }
        }
    }

    fn controller(&mut self, kind: ControllerKind, value: f32) {
        match kind {
            ControllerKind::PitchBend => {
                let semitones = value.clamp(-1.0, 1.0) * BEND_RANGE_SEMITONES;
                self.bend_ratio = 2f64.powf(f64::from(semitones) / 12.0);
            }
            ControllerKind::Cc { number: CC_SUSTAIN } => {
                self.sustain = value >= 0.5;
                if !self.sustain {
                    for voice in self.voices.iter_mut().filter(|v| v.active && v.sustained) {
                        voice.sustained = false;
                        voice.releasing = true;
                    }
                }
            }
            // The sampler has no way to express the rest: volume and modulation come from parameters and effects.
            ControllerKind::Cc { .. } | ControllerKind::Aftertouch => {}
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        self.settings.set(name, value);
    }

    fn render(&mut self, block: &mut AudioBlock) {
        let Self { data, settings, sample_rate, voices, bend_ratio, .. } = self;
        let bend = *bend_ratio;
        let Some(data) = data.as_deref() else {
            return;
        };
        let frame_count = data.frame_count();
        if frame_count < 2 {
            return;
        }
        let attack_step = 1.0 / (settings.attack * *sample_rate).max(1.0);
        let release_step = 1.0 / (settings.release * *sample_rate).max(1.0);
        let (lo, hi) = settings.region(frame_count);
        let span = hi - lo;
        let channels = block.channels();
        let slice = block.as_mut_slice();

        for frame in 0..slice.len() / channels {
            let mut left = 0.0;
            let mut right = 0.0;
            for voice in voices.iter_mut() {
                if !voice.active {
                    continue;
                }
                let backwards = voice.increment < 0.0;
                let finished = if backwards { voice.position <= lo } else { voice.position >= hi };
                if finished {
                    if settings.looping && !voice.releasing {
                        // Wrapping inside the region without skipping a frame gives a click-free join.
                        voice.position += if backwards { span } else { -span };
                    } else {
                        *voice = Voice::IDLE;
                        continue;
                    }
                }
                if voice.releasing {
                    voice.level -= release_step;
                    if voice.level <= 0.0 {
                        *voice = Voice::IDLE;
                        continue;
                    }
                } else if voice.level < 1.0 {
                    voice.level = (voice.level + attack_step).min(1.0);
                }
                let amp = voice.level * voice.velocity * settings.gain;
                let (l, r) = data.stereo_lerp(voice.position);
                left += l * amp;
                right += r * amp;
                voice.position += voice.increment * bend;
            }
            let base = frame * channels;
            if channels == 1 {
                slice[base] += f32::midpoint(left, right);
            } else {
                slice[base] += left;
                slice[base + 1] += right;
            }
        }
    }
}

struct SamplerFactory {
    store: Arc<dyn SampleStore>,
}

impl InstrumentFactory for SamplerFactory {
    fn kind(&self) -> &'static str {
        SAMPLER_KIND
    }

    fn label(&self) -> L10n {
        L10n { en: "Sampler", ru: "Сэмплер" }
    }

    fn describe(&self) -> Vec<ParamInfo> {
        PARAMS.to_vec()
    }

    fn about(&self) -> L10n {
        ABOUT
    }

    // A built-in instrument has no state of its own: everything is in the parameters.
    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        let id = params.get(SAMPLER_SAMPLE_PARAM).copied().unwrap_or(0.0) as u32;
        // Without a sample (id 0) the instrument is created silent: a file can be set later. A sample that is not in the
        // store is a failure the project reports, not a silent track nobody can explain.
        let data = match id {
            0 => None,
            id => Some(self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?),
        };
        Ok(Box::new(Sampler {
            data,
            settings: Settings::from_params(params),
            sample_rate: StreamConfig::DEFAULT.sample_rate as f32,
            voices: [Voice::IDLE; VOICES],
            note_counter: 0,
            bend_ratio: 1.0,
            sustain: false,
        }))
    }

    fn rebuild_on(&self) -> &'static [&'static str] {
        &[SAMPLER_SAMPLE_PARAM]
    }
}

pub fn factory(store: Arc<dyn SampleStore>) -> Box<dyn InstrumentFactory> {
    Box::new(SamplerFactory { store })
}

#[cfg(test)]
mod tests;
