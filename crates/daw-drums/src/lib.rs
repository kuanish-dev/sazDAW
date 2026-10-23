//! Synthesized drums: each is an instrument with one voice.

mod voice;

use daw_contracts::{ContractError, Instrument, InstrumentFactory, L10n, ParamInfo, ParamKind, Params, PluginState};

use voice::Drum;

#[derive(Debug, Clone, Copy)]
enum DrumKind {
    Kick,
    Snare,
    Hat,
    Clap,
    OpenHat,
    TomHigh,
    TomMid,
    TomLow,
    Crash,
    Ride,
    Rim,
}

/// The parameters of every drum (the same for all kinds). Each `about` says what the parameter does to the sound, for the
/// AI that cannot hear it: a change to the sound in `voice.rs` needs the same change in the text.
const PARAMS: [ParamInfo; 3] = [
    ParamInfo {
        name: "gain",
        label: L10n { en: "Volume", ru: "Громкость" },
        min: 0.0,
        max: 1.5,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Loudness of the hit, linear: 1 is the normal level, 0.5 half (-6 dB), 1.5 the most (+3.5 dB).",
            ru: "Громкость удара, линейно: 1 — обычный уровень, 0,5 — вдвое тише (-6 дБ), 1,5 — максимум (+3,5 дБ).",
        },
    },
    ParamInfo {
        name: "tune",
        label: L10n { en: "Pitch, semitones", ru: "Высота, полутона" },
        min: -12.0,
        max: 12.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Pitch of the hit's tone in semitones (12 = an octave up). It moves only the tonal part: the kick's and the toms' \
                 sine with its sweep, the snare's 190 Hz body, the rimshot's tone and the ride's ring. Pure noise (hi-hats, clap, \
                 crash, the snare's rattle) does not change. On the kick, -3…-5 is a deeper boom, +2…+5 a tighter thud.",
            ru: "Высота тона удара в полутонах (12 = октава вверх). Сдвигает только тональную часть: синус бочки и томов вместе с \
                 их «падением», корпус снейра 190 Гц, тон римшота и звон райда. Чистый шум (хэты, клэп, крэш, шумовая часть \
                 снейра) не меняется. На бочке -3…-5 — более глубокий «бум», +2…+5 — более плотный удар.",
        },
    },
    ParamInfo {
        name: "decay",
        label: L10n { en: "Decay, x", ru: "Затухание, x" },
        min: 0.3,
        max: 3.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n {
            en: "Stretches the whole hit in time: 2 is twice as long, 0.5 half. Everything stretches together, the kick's and the \
                 toms' pitch drop included (a slower sweep), while the pitch it ends on stays. A kick at 2-3 rings for 1-1.5 s, \
                 like a long 808.",
            ru: "Растягивает весь удар во времени: 2 — вдвое длиннее, 0,5 — вдвое короче. Растягивается всё вместе, включая \
                 «падение» высоты бочки и томов (оно становится медленнее), а конечная высота не меняется. Бочка при 2-3 звучит \
                 1-1,5 с, как длинная 808.",
        },
    },
];

/// How every drum plays notes: the end of each drum's `about` (`concat!` needs literals, so a macro, not a `const`).
macro_rules! plays_notes {
    (en) => {
        "Velocity (1-127) scales the loudness linearly: 64 is half as loud as 127. The note's pitch and length do not matter: a \
         hit always plays to its end (tune sets the pitch). One voice: a new note restarts the hit and cuts the previous one's \
         tail."
    };
    (ru) => {
        "Сила нажатия (velocity, 1-127) меняет громкость линейно: 64 вдвое тише, чем 127. Высота и длина ноты не важны: удар \
         всегда звучит до конца (высоту задаёт tune). Один голос: новая нота перезапускает удар и обрывает хвост предыдущего."
    };
}

/// One drum of the catalog: its sound, kind id, name and what it sounds like (`InstrumentFactory::about`).
struct DrumFactory {
    kind: DrumKind,
    id: &'static str,
    label: L10n,
    about: L10n,
}

impl InstrumentFactory for DrumFactory {
    fn kind(&self) -> &'static str {
        self.id
    }

    fn label(&self) -> L10n {
        self.label
    }

    fn describe(&self) -> Vec<ParamInfo> {
        PARAMS.to_vec()
    }

    fn about(&self) -> L10n {
        self.about
    }

    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        let mut drum = Drum::new(self.kind);
        for (name, value) in params {
            drum.set_param(name, *value);
        }
        Ok(Box::new(drum))
    }
}

/// All drums, in catalog order. The `about` texts describe the synthesis in `voice.rs`: keep them in step with it.
const DRUMS: [DrumFactory; 11] = [
    DrumFactory {
        kind: DrumKind::Kick,
        id: "kick",
        label: L10n { en: "Kick", ru: "Кик" },
        about: L10n {
            en: concat!(
                "A pure sine boom without a click or noise: the pitch drops from about 155 Hz to 45 Hz in the first 0.1 s, the \
                 body fades (-24 dB at 0.5 s) and is cut at 0.5 s. Between a short 808 and a 909 without its click; for a click \
                 layer a rimshot or a closed hat, for drive put a distortion or saturator after it.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Чистый синусный «бум» без щелчка и шума: высота падает примерно со 155 до 45 Гц за первые 0,1 с, тело затухает \
                 (-24 дБ к 0,5 с) и обрывается на 0,5 с. Между короткой 808 и 909 без её щелчка; для щелчка добавьте римшот или \
                 закрытый хэт, для перегруза — дисторшн или сатуратор после неё.",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::Snare,
        id: "snare",
        label: L10n { en: "Snare", ru: "Снейр" },
        about: L10n {
            en: concat!(
                "A dry electronic snare: a white-noise rattle and a 190 Hz tone, both gone in about 0.2 s, cut at 0.3 s (808-like, \
                 no room or body ring).",
                plays_notes!(en)
            ),
            ru: concat!(
                "Сухой электронный снейр: шорох белого шума и тон 190 Гц, оба затихают примерно за 0,2 с, обрыв на 0,3 с (похож на \
                 808, без комнаты и звона корпуса).",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::Hat,
        id: "hat",
        label: L10n { en: "Hi-hat", ru: "Хэт" },
        about: L10n {
            en: concat!("A closed hi-hat: a short tick of bright noise, gone in about 0.05 s (cut at 0.12 s).", plays_notes!(en)),
            ru: concat!("Закрытый хэт: короткий «тик» яркого шума, затихает примерно за 0,05 с (обрыв на 0,12 с).", plays_notes!(ru)),
        },
    },
    DrumFactory {
        kind: DrumKind::Clap,
        id: "clap",
        label: L10n { en: "Clap", ru: "Клэп" },
        about: L10n {
            en: concat!(
                "An 808/909-style clap: three quick bursts of bright noise 11 ms apart and a short tail, cut at 0.35 s.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Клэп в стиле 808/909: три быстрых всплеска яркого шума с шагом 11 мс и короткий хвост, обрыв на 0,35 с.",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::OpenHat,
        id: "openhat",
        label: L10n { en: "Open hi-hat", ru: "Открытый хэт" },
        about: L10n {
            en: concat!(
                "An open hi-hat: bright noise ringing about half a second (cut at 0.55 s). A closed hat does not choke it: they \
                 are separate tracks.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Открытый хэт: яркий шум звучит около полсекунды (обрыв на 0,55 с). Закрытый хэт его не глушит: это отдельные \
                 дорожки.",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::TomHigh,
        id: "tom_hi",
        label: L10n { en: "High tom", ru: "Том высокий" },
        about: L10n {
            en: concat!(
                "An electronic tom: a sine that falls from about 280 Hz to 210 Hz in the first 0.1 s with a tiny click, fading \
                 over about 0.4 s (cut at 0.5 s).",
                plays_notes!(en)
            ),
            ru: concat!(
                "Электронный том: синус, который падает примерно с 280 до 210 Гц за первые 0,1 с, с лёгким щелчком, затихает \
                 примерно за 0,4 с (обрыв на 0,5 с).",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::TomMid,
        id: "tom_mid",
        label: L10n { en: "Mid tom", ru: "Том средний" },
        about: L10n {
            en: concat!(
                "An electronic tom: a sine that falls from about 220 Hz to 150 Hz in the first 0.1 s with a tiny click, fading \
                 over about 0.4 s (cut at 0.5 s).",
                plays_notes!(en)
            ),
            ru: concat!(
                "Электронный том: синус, который падает примерно с 220 до 150 Гц за первые 0,1 с, с лёгким щелчком, затихает \
                 примерно за 0,4 с (обрыв на 0,5 с).",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::TomLow,
        id: "tom_lo",
        label: L10n { en: "Low tom", ru: "Том низкий" },
        about: L10n {
            en: concat!(
                "An electronic tom: a sine that falls from about 175 Hz to 105 Hz in the first 0.1 s with a tiny click, fading \
                 over about 0.4 s (cut at 0.5 s).",
                plays_notes!(en)
            ),
            ru: concat!(
                "Электронный том: синус, который падает примерно со 175 до 105 Гц за первые 0,1 с, с лёгким щелчком, затихает \
                 примерно за 0,4 с (обрыв на 0,5 с).",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::Crash,
        id: "crash",
        label: L10n { en: "Crash", ru: "Крэш" },
        about: L10n {
            en: concat!("A crash cymbal: a sharp burst of bright noise and a long hissing tail, about 1.6 s.", plays_notes!(en)),
            ru: concat!("Тарелка крэш: резкий всплеск яркого шума и длинный шипящий хвост, около 1,6 с.", plays_notes!(ru)),
        },
    },
    DrumFactory {
        kind: DrumKind::Ride,
        id: "ride",
        label: L10n { en: "Ride", ru: "Райд" },
        about: L10n {
            en: concat!(
                "A ride cymbal: a metallic ping (three inharmonic tones at 3.15, 4.4 and 6.3 kHz) over a short hiss, ringing about \
                 1 s.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Тарелка райд: металлический «пинг» (три негармонических тона 3,15, 4,4 и 6,3 кГц) поверх короткого шипения, \
                 звенит около 1 с.",
                plays_notes!(ru)
            ),
        },
    },
    DrumFactory {
        kind: DrumKind::Rim,
        id: "rim",
        label: L10n { en: "Rimshot", ru: "Римшот" },
        about: L10n {
            en: concat!(
                "A rimshot: a very short click (about 0.03 s) of a 1050 Hz tone and noise; also a click layer for a kick.",
                plays_notes!(en)
            ),
            ru: concat!(
                "Римшот: очень короткий щелчок (около 0,03 с) из тона 1050 Гц и шума; годится и как щелчок поверх бочки.",
                plays_notes!(ru)
            ),
        },
    },
];

/// Factories of all drums (see `DRUMS`).
pub fn factories() -> Vec<Box<dyn InstrumentFactory>> {
    DRUMS.into_iter().map(|drum| Box::new(drum) as Box<dyn InstrumentFactory>).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{AudioBlock, StreamConfig};

    /// One hit of `kind` with `params`: `frames` mono frames.
    fn hit_with(kind: DrumKind, params: &[(&str, f32)], frames: usize) -> Vec<f32> {
        let mut drum = Drum::new(kind);
        drum.prepare(StreamConfig { sample_rate: 48_000, channels: 1 });
        for (name, value) in params {
            drum.set_param(name, *value);
        }
        drum.note_on(36, 1.0);
        let mut buffer = vec![0.0; frames];
        drum.render(&mut AudioBlock::new(&mut buffer, 1));
        buffer
    }

    fn hit(kind: DrumKind, frames: usize) -> Vec<f32> {
        hit_with(kind, &[], frames)
    }

    #[test]
    fn every_drum_is_audible_finite_and_ends() {
        for DrumFactory { kind, .. } in DRUMS {
            let out = hit(kind, 120_000);
            let peak = out.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            assert!(peak > 0.05 && peak <= 1.0, "{kind:?} peak = {peak}");
            assert!(out.iter().all(|v| v.is_finite()));
            assert!(out[100_000..].iter().all(|v| *v == 0.0), "{kind:?} should end");
        }
    }

    #[test]
    fn tune_and_decay_change_the_sound() {
        let kick = |tune: f32, decay: f32| hit_with(DrumKind::Kick, &[("tune", tune), ("decay", decay)], 48_000);
        let normal = kick(0.0, 1.0);
        let last_audible = |b: &[f32]| b.iter().rposition(|v| *v != 0.0).unwrap_or(0);
        assert!(last_audible(&kick(0.0, 2.0)) > last_audible(&normal));
        assert_ne!(normal, kick(12.0, 1.0));
    }

    #[test]
    fn toms_are_ordered_by_pitch_and_cymbals_differ_in_length() {
        let zero_crossings = |kind| hit(kind, 4_800).windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!(zero_crossings(DrumKind::TomHigh) > zero_crossings(DrumKind::TomMid));
        assert!(zero_crossings(DrumKind::TomMid) > zero_crossings(DrumKind::TomLow));
        let last_audible = |kind| hit(kind, 120_000).iter().rposition(|v| v.abs() > 1e-4).unwrap_or(0);
        assert!(last_audible(DrumKind::Hat) < last_audible(DrumKind::OpenHat));
        assert!(last_audible(DrumKind::OpenHat) < last_audible(DrumKind::Crash));
    }

    #[test]
    fn all_notes_off_fades_a_ringing_hit_out_quickly() {
        let mut drum = Drum::new(DrumKind::Crash);
        drum.prepare(StreamConfig { sample_rate: 48_000, channels: 1 });
        drum.note_on(49, 1.0);
        let mut before = vec![0.0; 4_800];
        drum.render(&mut AudioBlock::new(&mut before, 1));
        drum.all_notes_off();
        let mut after = vec![0.0; 4_800];
        drum.render(&mut AudioBlock::new(&mut after, 1));
        assert!(after[..240].iter().any(|v| *v != 0.0), "a fade, not a cut (a cut clicks)");
        assert!(after[2_400..].iter().all(|v| *v == 0.0), "silent after 50 ms instead of ringing on for over a second");
        drum.note_on(49, 1.0);
        let mut next = vec![0.0; 4_800];
        drum.render(&mut AudioBlock::new(&mut next, 1));
        assert!(next[2_400..].iter().any(|v| *v != 0.0), "the next hit plays in full");
    }

    #[test]
    fn silent_until_triggered() {
        let mut drum = Drum::new(DrumKind::Kick);
        drum.prepare(StreamConfig { sample_rate: 48_000, channels: 1 });
        let mut buffer = vec![0.0; 256];
        drum.render(&mut AudioBlock::new(&mut buffer, 1));
        assert!(buffer.iter().all(|v| *v == 0.0));
    }
}
