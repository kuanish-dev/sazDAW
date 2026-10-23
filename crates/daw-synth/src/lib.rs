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

const fn number(name: &'static str, label: L10n, min: f32, max: f32, default: f32, about: L10n) -> ParamInfo {
    ParamInfo { name, label, min, max, default, kind: ParamKind::Number, choices: &[], about }
}

const fn choice(name: &'static str, label: L10n, choices: &'static [L10n], max: f32, default: f32, about: L10n) -> ParamInfo {
    ParamInfo { name, label, min: 0.0, max, default, kind: ParamKind::Choice, choices, about }
}

/// What the synth as a whole does with notes (`InstrumentFactory::about`); the parameters explain themselves in `PARAMS`.
const ABOUT: L10n = L10n {
    en: "Subtractive synth with 8 voices (a ninth note takes the oldest voice; 1 voice in mono mode): two oscillators through a \
         resonant 12 dB/octave low-pass filter per voice (high_cut), shaped by an AHDSR volume envelope. A note sounds while it is \
         held, then fades over release. Velocity (1-127) changes only the loudness, linearly: 64 is half as loud as 127. It does \
         not open the filter, so there is no TB-303-style accent; imitate one by automating filter_env or high_cut at the accented \
         notes. Pitch bend is ±2 semitones; the mod wheel (CC1) opens the filter by up to 4 octaves; other controllers and \
         aftertouch do nothing.",
    ru: "Субтрактивный синтезатор на 8 голосов (девятая нота забирает самый старый голос; в режиме моно голос один): два \
         осциллятора через резонансный фильтр нижних частот 12 дБ/окт на каждом голосе (high_cut) и огибающую громкости AHDSR. \
         Нота звучит, пока удерживается, затем затухает за release. Сила нажатия (velocity, 1-127) меняет только громкость, \
         линейно: 64 вдвое тише, чем 127. Фильтр она не открывает, поэтому акцента в духе TB-303 нет; его можно изобразить \
         автоматизацией filter_env или high_cut на акцентных нотах. Pitch bend — ±2 полутона; колесо модуляции (CC1) открывает \
         фильтр до 4 октав; остальные контроллеры и aftertouch ничего не делают.",
};

/// Grouped as the instrument window shows them: oscillators, unison, volume envelope, filter, pitch, mono, LFO, volume.
/// Envelope names follow the convention the interface draws as an envelope graph (attack/hold/decay/sustain/release;
/// sustain is a level `0..1`). Each `about` says what the parameter does to the sound, for the AI that cannot hear it:
/// a change to the sound in `engine` needs the same change in the text.
const PARAMS: [ParamInfo; 30] = [
    choice(
        "waveform",
        L10n { en: "Waveform", ru: "Форма волны" },
        &WAVEFORMS,
        4.0,
        2.0,
        L10n {
            en: "Shape of oscillator 1. Sine: a pure tone without overtones (sub bass). Triangle: soft, few overtones. Saw: bright \
                 and buzzy, all harmonics (leads, pads, acid). Square: hollow, odd harmonics (acid, chiptune). Noise: hiss without \
                 pitch (risers, percussion).",
            ru: "Форма осциллятора 1. Синус — чистый тон без обертонов (саб-бас). Треугольник — мягкий, мало обертонов. Пила — \
                 яркий, жужжащий, все гармоники (лиды, пэды, эйсид). Меандр — полый, нечётные гармоники (эйсид, чиптюн). Шум — \
                 шипение без высоты (райзеры, перкуссия).",
        },
    ),
    number(
        "osc2_level",
        L10n { en: "Osc 2 level, %", ru: "Осц. 2: уровень, %" },
        0.0,
        100.0,
        0.0,
        L10n {
            en: "Level of oscillator 2 against oscillator 1: 0 % is off, 100 % an equal mix. The sum is scaled down as it grows, \
                 so the loudness stays about the same.",
            ru: "Уровень осциллятора 2 относительно осциллятора 1: 0 % — выключен, 100 % — поровну. Сумма приглушается по мере \
                 роста, так что громкость почти не меняется.",
        },
    ),
    choice(
        "osc2_waveform",
        L10n { en: "Osc 2 waveform", ru: "Осц. 2: форма волны" },
        &WAVEFORMS,
        4.0,
        2.0,
        L10n {
            en: "Shape of oscillator 2, the same options as waveform. Heard only when osc2_level is above 0.",
            ru: "Форма осциллятора 2, те же варианты, что у waveform. Слышна, только когда osc2_level больше 0.",
        },
    ),
    number(
        "osc2_semitones",
        L10n { en: "Osc 2 pitch, semitones", ru: "Осц. 2: высота, полутоны" },
        -24.0,
        24.0,
        0.0,
        L10n {
            en: "Pitch of oscillator 2 against oscillator 1, in semitones: 12 is an octave up, -12 an octave down, 7 a fifth up. \
                 Heard only when osc2_level is above 0.",
            ru: "Высота осциллятора 2 относительно осциллятора 1 в полутонах: 12 — октава вверх, -12 — октава вниз, 7 — квинта \
                 вверх. Слышна, только когда osc2_level больше 0.",
        },
    ),
    number(
        "osc2_detune",
        L10n { en: "Osc 2 detune, cents", ru: "Осц. 2: расстройка, центы" },
        -50.0,
        50.0,
        0.0,
        L10n {
            en: "Fine pitch offset of oscillator 2 in cents (100 cents = a semitone): 5-15 cents beat against oscillator 1 into a \
                 thicker, chorus-like sound.",
            ru: "Тонкая расстройка осциллятора 2 в центах (100 центов = полутон): 5-15 центов дают биения с осциллятором 1 — звук \
                 гуще, как с хорусом.",
        },
    ),
    number(
        "unison",
        L10n { en: "Unison voices", ru: "Голоса унисона" },
        1.0,
        7.0,
        1.0,
        L10n {
            en: "Copies of each oscillator per note, 1-7 (whole numbers). 1 is a plain mono sound; more copies are detuned \
                 (unison_detune) and spread in stereo (unison_width) into a wide, thick sound (a supersaw with the saw). The \
                 loudness stays about the same.",
            ru: "Сколько копий каждого осциллятора звучит на ноту, 1-7 (целое). 1 — простой моно-звук; больше копий расстраиваются \
                 (unison_detune) и расходятся по стерео (unison_width) — широкий плотный звук (с пилой — суперпила). Громкость \
                 почти не меняется.",
        },
    ),
    number(
        "unison_detune",
        L10n { en: "Unison detune, cents", ru: "Расстройка унисона, центы" },
        0.0,
        100.0,
        20.0,
        L10n {
            en: "How far apart the outermost unison copies are tuned, in cents (100 = a semitone): 10-30 is a lush chorus, above \
                 50 sounds out of tune. No effect at unison 1.",
            ru: "Насколько разведены по высоте крайние копии унисона, в центах (100 = полутон): 10-30 — сочный хорус, больше 50 — \
                 звучит расстроенно. При unison 1 не действует.",
        },
    ),
    number(
        "unison_width",
        L10n { en: "Unison width, %", ru: "Ширина унисона, %" },
        0.0,
        100.0,
        50.0,
        L10n {
            en: "Stereo spread of the unison copies: 0 % all in the centre, 100 % the outermost ones hard left and right. No \
                 effect at unison 1.",
            ru: "Ширина стерео для копий унисона: 0 % — все в центре, 100 % — крайние полностью слева и справа. При unison 1 не \
                 действует.",
        },
    ),
    number(
        "attack",
        L10n { en: "Attack, s", ru: "Attack, с" },
        0.0005,
        4.0,
        0.005,
        L10n {
            en: "Volume envelope: seconds from silence to full level when a note starts (a straight line). Below 0.01 s is a sharp \
                 start; 0.3 s and more a slow swell.",
            ru: "Огибающая громкости: секунды от тишины до полного уровня в начале ноты (по прямой). Меньше 0,01 с — резкое \
                 начало; от 0,3 с — медленное нарастание.",
        },
    ),
    number(
        "hold",
        L10n { en: "Hold, s", ru: "Hold, с" },
        0.0,
        4.0,
        0.0,
        L10n {
            en: "Volume envelope: seconds the level stays at full after the attack, before the decay starts.",
            ru: "Огибающая громкости: сколько секунд уровень держится на максимуме после атаки, перед спадом.",
        },
    ),
    number(
        "decay",
        L10n { en: "Decay, s", ru: "Decay, с" },
        0.0005,
        4.0,
        0.15,
        L10n {
            en: "Volume envelope: seconds to fall from full level to sustain (a straight line). With sustain 0 this is the length \
                 of a pluck.",
            ru: "Огибающая громкости: секунды спада от полного уровня до sustain (по прямой). При sustain 0 это длина щипка.",
        },
    ),
    number(
        "sustain",
        L10n::same("Sustain"),
        0.0,
        1.0,
        0.6,
        L10n {
            en: "Volume envelope: the level (0-1) kept while the note is held, after the decay. 1 holds full volume; 0 makes every \
                 note a pluck that dies after attack + hold + decay, however long the note is.",
            ru: "Огибающая громкости: уровень (0-1), который держится, пока нота удерживается, после спада. 1 — полная громкость; \
                 0 — каждая нота становится щипком и затихает за attack + hold + decay, какой бы длинной ни была.",
        },
    ),
    number(
        "release",
        L10n { en: "Release, s", ru: "Release, с" },
        0.0005,
        8.0,
        0.12,
        L10n {
            en: "Volume envelope: seconds from the current level to silence after the note ends (a straight line). Long values let \
                 notes ring into each other.",
            ru: "Огибающая громкости: секунды от текущего уровня до тишины после конца ноты (по прямой). Большие значения дают \
                 нотам звучать друг поверх друга.",
        },
    ),
    number(
        "low_cut",
        L10n { en: "Low cut, Hz", ru: "Срез низов, Гц" },
        20.0,
        2_000.0,
        20.0,
        L10n {
            en: "A gentle (6 dB/octave) high-pass on the whole synth: thins out the lows below this frequency. 20 Hz is off; \
                 100-300 Hz keeps a pad or a lead out of the bass's way.",
            ru: "Пологий (6 дБ/окт) фильтр верхних частот на весь синт: убирает низы ниже этой частоты. 20 Гц — выключен; 100-300 \
                 Гц — пэд или лид не мешают басу.",
        },
    ),
    number(
        "high_cut",
        L10n { en: "High cut, Hz", ru: "Срез верхов, Гц" },
        40.0,
        18_000.0,
        1_800.0,
        L10n {
            en: "Cutoff of the voice low-pass filter (12 dB/octave), the main brightness control: 100-400 Hz is dark and muffled \
                 (a deep acid bass), 1-3 kHz warm, above 8 kHz the oscillator passes almost untouched. The filter envelope, the \
                 LFO and the mod wheel move the cutoff from here, in octaves.",
            ru: "Частота среза фильтра нижних частот голоса (12 дБ/окт) — главная ручка яркости: 100-400 Гц — тёмный, глухой звук \
                 (глубокий эйсид-бас), 1-3 кГц — тёплый, выше 8 кГц осциллятор проходит почти без изменений. Огибающая фильтра, \
                 LFO и колесо модуляции сдвигают срез отсюда, в октавах.",
        },
    ),
    number(
        "resonance",
        L10n { en: "Resonance, %", ru: "Резонанс, %" },
        0.0,
        100.0,
        0.0,
        L10n {
            en: "Peak of the low-pass at its cutoff: 0 % is a smooth corner, 100 % a sharp ringing peak (Q about 10) that never \
                 self-oscillates. 60-90 % with a moving cutoff is the squelchy acid sound; high values boost the harmonics at the \
                 cutoff a lot (up to ×10).",
            ru: "Пик фильтра на частоте среза: 0 % — плавный излом, 100 % — острый звенящий пик (добротность около 10), который \
                 никогда не самовозбуждается. 60-90 % при движущемся срезе — «хлюпающий» эйсид-звук; высокие значения сильно (до \
                 ×10) усиливают гармоники у среза.",
        },
    ),
    number(
        "filter_env",
        L10n { en: "Filter envelope, octaves", ru: "Огибающая фильтра, октавы" },
        -6.0,
        6.0,
        0.0,
        L10n {
            en: "How far the filter envelope moves the cutoff at its peak, in octaves (each octave doubles the frequency): +3 \
                 opens a 500 Hz cutoff to 4 kHz at the note start, and it closes back over filter_decay. Negative values close the \
                 filter instead; 0 is off. The depth is the same at every velocity.",
            ru: "На сколько октав огибающая фильтра сдвигает срез на пике (каждая октава удваивает частоту): +3 открывает срез 500 \
                 Гц до 4 кГц в начале ноты, затем он закрывается за filter_decay. Отрицательные значения, наоборот, закрывают \
                 фильтр; 0 — выключено. Глубина одинакова при любой силе нажатия.",
        },
    ),
    number(
        "filter_attack",
        L10n { en: "Filter attack, s", ru: "Атака фильтра, с" },
        0.0005,
        4.0,
        0.005,
        L10n {
            en: "Filter envelope: seconds to rise from 0 to its peak when a note starts (a straight line in octaves). Under 0.01 s \
                 opens at once; longer is a swell of brightness.",
            ru: "Огибающая фильтра: секунды подъёма от 0 до пика в начале ноты (по прямой в октавах). Меньше 0,01 с — открывается \
                 сразу; дольше — плавное нарастание яркости.",
        },
    ),
    number(
        "filter_decay",
        L10n { en: "Filter decay, s", ru: "Спад фильтра, с" },
        0.0005,
        4.0,
        0.3,
        L10n {
            en: "Filter envelope: seconds to fall from the peak back to 0 (a straight line in octaves), whether the key is still \
                 held or not. 0.05-0.3 s gives a plucky 'wow' on every note. It restarts on every note except legato notes in mono \
                 mode.",
            ru: "Огибающая фильтра: секунды спада от пика обратно к 0 (по прямой в октавах), удерживается клавиша или нет. \
                 0,05-0,3 с — щёлкающее «вау» на каждой ноте. Запускается заново на каждой ноте, кроме легато-нот в режиме моно.",
        },
    ),
    number(
        "pitch_env",
        L10n { en: "Pitch envelope, semitones", ru: "Огибающая высоты, полутоны" },
        -48.0,
        48.0,
        0.0,
        L10n {
            en: "The pitch starts this many semitones away from the note and settles on it over pitch_decay: +12…+36 with a short \
                 decay is a kick or tom punch, negative values rise into the note. 0 is off.",
            ru: "Высота начинается на столько полутонов от ноты и приходит к ней за pitch_decay: +12…+36 с коротким спадом — удар \
                 бочки или тома, отрицательные значения подъезжают к ноте снизу. 0 — выключено.",
        },
    ),
    number(
        "pitch_decay",
        L10n { en: "Pitch decay, s", ru: "Спад высоты, с" },
        0.001,
        2.0,
        0.08,
        L10n {
            en: "Seconds the pitch envelope takes to settle on the note (a curve: 95 % there after this time).",
            ru: "За сколько секунд огибающая высоты приходит к ноте (по кривой: через это время пройдено 95 %).",
        },
    ),
    ParamInfo {
        kind: ParamKind::Switch,
        ..number(
            "mono",
            L10n { en: "Mono", ru: "Моно" },
            0.0,
            1.0,
            0.0,
            L10n {
                en: "On: one voice. A note that starts while the previous one is still held (overlapping notes) slides to its \
                     pitch over glide without restarting the envelopes (legato, like a TB-303 slide); letting it go slides back to \
                     a key still held. A note after a gap restarts the envelopes. Off: up to 8 voices, chords.",
                ru: "Вкл: один голос. Нота, начатая, пока предыдущая ещё удерживается (ноты перекрываются), съезжает к своей \
                     высоте за glide без перезапуска огибающих (легато, как слайд TB-303); при её отпускании высота возвращается к \
                     ещё нажатой клавише. Нота после паузы перезапускает огибающие. Выкл: до 8 голосов, аккорды.",
            },
        )
    },
    number(
        "glide",
        L10n { en: "Glide, s", ru: "Глайд, с" },
        0.0,
        2.0,
        0.0,
        L10n {
            en: "Mono mode only: seconds a legato slide takes (a curve: 95 % there after this time). Only overlapping notes slide \
                 (the next starts before the previous ends); separate notes, and every note when mono is off, jump straight to \
                 their pitch. 0 jumps.",
            ru: "Только в режиме моно: время слайда легато в секундах (по кривой: через это время пройдено 95 %). Скользят только \
                 перекрывающиеся ноты (следующая начинается до конца предыдущей); отдельные ноты, а при выключенном моно — все \
                 ноты, сразу встают на свою высоту. 0 — без скольжения.",
        },
    ),
    choice(
        "lfo_shape",
        L10n { en: "LFO shape", ru: "LFO: форма" },
        &LFO_SHAPES,
        4.0,
        0.0,
        L10n {
            en: "LFO wave: sine and triangle sweep smoothly, saw rises and drops back, square jumps between the two extremes (a \
                 trill, a gate), random holds a new random value each cycle (sample and hold).",
            ru: "Форма LFO: синус и треугольник плавно качают, пила нарастает и сбрасывается, меандр прыгает между двумя крайними \
                 значениями (трель, гейт), случайная держит новое случайное значение каждый цикл (sample and hold).",
        },
    ),
    number(
        "lfo_rate",
        L10n { en: "LFO rate, Hz", ru: "LFO: скорость, Гц" },
        0.05,
        20.0,
        5.0,
        L10n {
            en: "LFO speed in cycles per second while lfo_sync is Off: 0.1-1 Hz is a slow sweep, 4-7 Hz a vibrato or tremolo.",
            ru: "Скорость LFO в циклах в секунду, пока lfo_sync выключен: 0,1-1 Гц — медленное движение, 4-7 Гц — вибрато или \
                 тремоло.",
        },
    ),
    choice(
        "lfo_sync",
        L10n { en: "LFO tempo sync", ru: "LFO: синхронизация с темпом" },
        &LFO_SYNC,
        8.0,
        0.0,
        L10n {
            en: "Locks the LFO cycle to the song grid (1/1 = 4 beats, 1/16 = a sixteenth) and keeps it on the beat while playing; \
                 Off runs at lfo_rate. One LFO serves all voices and does not restart on notes.",
            ru: "Привязывает цикл LFO к сетке песни (1/1 = 4 доли, 1/16 = шестнадцатая) и держит его в долю при воспроизведении; \
                 Выкл — скорость из lfo_rate. Один LFO на все голоса, с нотами он не перезапускается.",
        },
    ),
    number(
        "lfo_pitch",
        L10n { en: "LFO to pitch, cents", ru: "LFO → высота, центы" },
        0.0,
        100.0,
        0.0,
        L10n {
            en: "LFO depth on pitch, in cents up and down (100 = a semitone): 10-30 is a vibrato.",
            ru: "Глубина LFO по высоте, в центах вверх и вниз (100 = полутон): 10-30 — вибрато.",
        },
    ),
    number(
        "lfo_filter",
        L10n { en: "LFO to filter, octaves", ru: "LFO → фильтр, октавы" },
        0.0,
        4.0,
        0.0,
        L10n {
            en: "LFO depth on the filter cutoff, in octaves up and down: 1-3 octaves at a synced rate is a wobble bass or a \
                 rhythmic filter sweep. The cutoff (high_cut) must be low enough to hear it move.",
            ru: "Глубина LFO по срезу фильтра, в октавах вверх и вниз: 1-3 октавы при синхронизации с темпом — воббл-бас или \
                 ритмичное движение фильтра. Чтобы движение было слышно, срез (high_cut) должен быть достаточно низким.",
        },
    ),
    number(
        "lfo_volume",
        L10n { en: "LFO to volume, %", ru: "LFO → громкость, %" },
        0.0,
        100.0,
        0.0,
        L10n {
            en: "How deep the LFO dips the volume: 0 % not at all, 100 % down to silence at the bottom of each cycle (a tremolo; a \
                 synced square is a trance gate).",
            ru: "Насколько глубоко LFO приглушает громкость: 0 % — никак, 100 % — до тишины в нижней точке каждого цикла (тремоло; \
                 синхронизированный меандр — транс-гейт).",
        },
    ),
    number(
        "gain",
        L10n { en: "Volume, %", ru: "Громкость, %" },
        0.0,
        100.0,
        25.0,
        L10n {
            en: "Output level, 0-100 % (linear). The default 25 % leaves headroom for chords and unison; balance tracks with the \
                 mixer volume instead.",
            ru: "Выходной уровень, 0-100 % (линейно). 25 % по умолчанию оставляют запас для аккордов и унисона; баланс дорожек \
                 лучше выставлять громкостью в микшере.",
        },
    ),
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

    fn about(&self) -> L10n {
        ABOUT
    }

    fn create(&self, params: &Params, _state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        Ok(Box::new(Synth::new(params)))
    }
}

pub fn factory() -> Box<dyn InstrumentFactory> {
    Box::new(SynthFactory)
}

#[cfg(test)]
mod tests;
