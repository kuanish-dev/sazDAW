//! Arpeggiator: held notes play one at a time, in steps of a set length. Steps lie on the transport grid (step `k` at
//! tick `k × step length`, every second one shifted by swing); while the transport is stopped the grid is a free clock
//! at the segment's tempo, started by the first key press (the first step sounds at once).

use daw_contracts::{BlockTiming, L10n, NoteBuffer, NoteEffect, NoteEvent, PPQ, ParamInfo, ParamKind};

use crate::{Clock, fit};

pub const KIND: &str = "arp";
pub const LABEL: L10n = L10n { en: "Arpeggiator", ru: "Арпеджиатор" };

/// Step length in ticks for each choice of the `rate` parameter (in the order of `choices`).
const RATE_TICKS: [u32; 6] = [PPQ, PPQ / 2, PPQ / 3, PPQ / 4, PPQ / 6, PPQ / 8];
/// A sixteenth is the default step.
const DEFAULT_RATE: f32 = 3.0;

pub const PARAMS: [ParamInfo; 5] = [
    ParamInfo {
        name: "rate",
        label: L10n { en: "Step", ru: "Шаг" },
        min: 0.0,
        max: 5.0,
        default: DEFAULT_RATE,
        kind: ParamKind::Choice,
        choices: &[L10n::same("1/4"), L10n::same("1/8"), L10n::same("1/8T"), L10n::same("1/16"), L10n::same("1/16T"), L10n::same("1/32")],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "mode",
        label: L10n { en: "Order", ru: "Порядок" },
        min: 0.0,
        max: 4.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &[
            L10n { en: "Up", ru: "Вверх" },
            L10n { en: "Down", ru: "Вниз" },
            L10n { en: "Up-down", ru: "Вверх-вниз" },
            L10n { en: "Random", ru: "Случайно" },
            L10n { en: "As played", ru: "Как нажаты" },
        ],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "octaves",
        label: L10n { en: "Octaves", ru: "Октавы" },
        min: 1.0,
        max: 4.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "gate",
        label: L10n { en: "Note length, %", ru: "Длина ноты, %" },
        min: 10.0,
        max: 100.0,
        default: 50.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "swing",
        label: L10n { en: "Swing, %", ru: "Свинг, %" },
        min: 0.0,
        max: 75.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Up,
    Down,
    UpDown,
    Random,
    AsPlayed,
}

/// Held keys: press order and the velocity of each (0 means not held).
struct Keys {
    order: [u8; 128],
    count: usize,
    velocity: [f32; 128],
}

impl Keys {
    fn new() -> Self {
        Self { order: [0; 128], count: 0, velocity: [0.0; 128] }
    }

    fn press(&mut self, pitch: u8, velocity: f32) {
        let slot = usize::from(pitch & 0x7f);
        if self.velocity[slot] == 0.0 {
            self.order[self.count] = pitch & 0x7f;
            self.count += 1;
        }
        self.velocity[slot] = velocity;
    }

    fn release(&mut self, pitch: u8) {
        let slot = usize::from(pitch & 0x7f);
        if self.velocity[slot] == 0.0 {
            return;
        }
        self.velocity[slot] = 0.0;
        if let Some(at) = self.order[..self.count].iter().position(|&p| p == pitch & 0x7f) {
            self.order.copy_within(at + 1..self.count, at);
            self.count -= 1;
        }
    }

    /// The `index`-th held key by pitch (from the bottom).
    fn sorted(&self, index: usize) -> u8 {
        (0..=127u8).filter(|&p| self.velocity[usize::from(p)] > 0.0).nth(index).unwrap_or(0)
    }

    fn clear(&mut self) {
        self.count = 0;
        self.velocity = [0.0; 128];
    }
}

pub(crate) struct Arpeggiator {
    step_ticks: f64,
    mode: Mode,
    octaves: usize,
    /// The part of the step the note sounds (0.1..=1).
    gate: f64,
    /// Shift of every second step, in parts of a step (0..=0.75).
    swing: f64,
    keys: Keys,
    /// Steps played since the first press: picks the note of the pattern.
    position: usize,
    /// The sounding note and the tick to release it at.
    sounding: Option<(u8, f64)>,
    /// Free clock position (ticks) at the start of the next segment, while the transport is stopped.
    free_tick: f64,
    random: u32,
}

/// Seed of the random order: the same after every reset, so an export sounds the same as playback.
const SEED: u32 = 0x9E37_79B9;

impl Arpeggiator {
    pub fn new() -> Self {
        Self {
            step_ticks: f64::from(RATE_TICKS[DEFAULT_RATE as usize]),
            mode: Mode::Up,
            octaves: 1,
            gate: 0.5,
            swing: 0.0,
            keys: Keys::new(),
            position: 0,
            sounding: None,
            free_tick: 0.0,
            random: SEED,
        }
    }

    /// Tick of step `k`: every second step is shifted by swing.
    fn step_tick(&self, k: i64) -> f64 {
        let swing = if k.rem_euclid(2) == 1 { self.swing * self.step_ticks } else { 0.0 };
        k as f64 * self.step_ticks + swing
    }

    /// Index of the step not later than `tick` (one step of margin: the loop skips steps before `tick`).
    fn first_step(&self, tick: f64) -> i64 {
        (tick / self.step_ticks).floor() as i64 - 1
    }

    /// An input press or release. On the first press the pattern starts over, and on a stopped transport the free clock
    /// restarts so that step 0 falls on the press frame: returns the new segment start.
    fn take(&mut self, event: NoteEvent, playing: bool, tpf: f64) -> Option<f64> {
        if !event.is_on() {
            self.keys.release(event.pitch);
            return None;
        }
        let first = self.keys.count == 0;
        self.keys.press(event.pitch, event.velocity);
        if !first {
            return None;
        }
        self.position = 0;
        (!playing).then(|| -(event.frame as f64) * tpf)
    }

    fn next_random(&mut self) -> u32 {
        // xorshift32: no memory and no locks.
        let mut x = self.random;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.random = x;
        x
    }

    /// Note and velocity for the current pattern step.
    fn pick(&mut self) -> (u8, f32) {
        let held = self.keys.count;
        let len = held * self.octaves;
        let p = self.position;
        let index = match self.mode {
            Mode::Up | Mode::AsPlayed => p % len,
            Mode::Down => len - 1 - p % len,
            Mode::UpDown if len < 2 => 0,
            Mode::UpDown => {
                let period = 2 * len - 2;
                let r = p % period;
                if r < len { r } else { period - r }
            }
            Mode::Random => self.next_random() as usize % len,
        };
        let (octave, slot) = (index / held, index % held);
        let key = if self.mode == Mode::AsPlayed { self.keys.order[slot] } else { self.keys.sorted(slot) };
        let mut pitch = usize::from(key) + 12 * octave;
        while pitch > 127 {
            pitch -= 12;
        }
        (pitch as u8, self.keys.velocity[usize::from(key)])
    }

    /// Releases the sounding note if it ends not later than `limit` (earlier than `limit` if `strict`).
    fn release_due(&mut self, limit: f64, strict: bool, clock: Clock, output: &mut NoteBuffer) {
        if let Some((pitch, off)) = self.sounding
            && (off < limit || (!strict && off <= limit))
        {
            output.push(NoteEvent::off(clock.frame(off), pitch));
            self.sounding = None;
        }
    }

    /// A step at tick `tick` (frame `frame`): the previous note is released if still sounding, the next pattern note is pressed.
    fn fire(&mut self, tick: f64, frame: usize, clock: Clock, output: &mut NoteBuffer) {
        self.release_due(tick, false, clock, output);
        if self.keys.count == 0 {
            return;
        }
        if let Some((pitch, _)) = self.sounding.take() {
            output.push(NoteEvent::off(frame, pitch));
        }
        let (pitch, velocity) = self.pick();
        output.push(NoteEvent::on(frame, pitch, velocity));
        self.sounding = Some((pitch, tick + self.gate * self.step_ticks));
        self.position += 1;
    }
}

impl NoteEffect for Arpeggiator {
    fn set_param(&mut self, name: &str, value: f32) {
        let Some(value) = fit(&PARAMS, name, value) else {
            return;
        };
        match name {
            "rate" => self.step_ticks = f64::from(RATE_TICKS[value as usize]),
            "mode" => {
                self.mode = [Mode::Up, Mode::Down, Mode::UpDown, Mode::Random, Mode::AsPlayed][value as usize];
            }
            "octaves" => self.octaves = value.round() as usize,
            "gate" => self.gate = f64::from(value) / 100.0,
            "swing" => self.swing = f64::from(value) / 100.0,
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.keys.clear();
        self.position = 0;
        self.sounding = None;
        self.random = SEED;
    }

    fn process(&mut self, input: &[NoteEvent], timing: &BlockTiming, output: &mut NoteBuffer) {
        let base = if timing.playing { timing.start_tick } else { self.free_tick };
        let mut clock = Clock { base, tpf: timing.ticks_per_frame, frames: timing.frames };
        if clock.tpf <= 0.0 {
            for &event in input {
                self.take(event, true, 0.0);
            }
            return;
        }
        let mut next = 0;
        let mut k = self.first_step(clock.base);
        loop {
            let step = self.step_tick(k);
            let step_frame = (step < clock.end()).then(|| clock.frame(step));
            // An input on a step's frame is in time for that step: a note pressed exactly on the grid sounds at once.
            if let Some(&event) = input.get(next).filter(|e| step_frame.is_none_or(|frame| e.frame <= frame)) {
                next += 1;
                if let Some(base) = self.take(event, timing.playing, clock.tpf) {
                    if let Some((_, off)) = &mut self.sounding {
                        *off += base - clock.base;
                    }
                    // The press frame's tick is 0: step 0 sounds first.
                    clock.base = base;
                    k = 0;
                }
                continue;
            }
            let Some(frame) = step_frame else { break };
            if step >= clock.base {
                self.fire(step, frame, clock, output);
            }
            k += 1;
        }
        self.release_due(clock.end(), true, clock, output);
        if !timing.playing {
            self.free_tick = clock.end();
        }
    }
}
