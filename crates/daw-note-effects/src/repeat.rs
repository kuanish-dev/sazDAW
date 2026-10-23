//! Note repeat: every held key plays again and again at a set note length while it is held — hi-hat rolls, ratchets
//! and stutters from one long note. The first hit sounds at the press; the repeats lie on the transport grid (step `k`
//! at tick `k × rate`), so a roll stays in time, in export too. While the transport is stopped the grid is a free
//! clock started by the first key. A velocity change per repeat makes a roll swell or fade.

use daw_contracts::{BlockTiming, L10n, NoteBuffer, NoteEffect, NoteEvent, PPQ, ParamInfo, ParamKind};

use crate::{Clock, fit};

pub const KIND: &str = "repeat";
pub const LABEL: L10n = L10n { en: "Note repeat", ru: "Повтор ноты" };

/// Repeat length in ticks for each choice of the `rate` parameter (in the order of `choices`).
const RATE_TICKS: [u32; 8] = [PPQ, PPQ / 2, PPQ / 3, PPQ / 4, PPQ / 6, PPQ / 8, PPQ / 12, PPQ / 16];
/// A thirty-second is the default: the usual hi-hat roll.
const DEFAULT_RATE: f32 = 5.0;
/// The quietest a fading roll gets: it never fades into silence.
const MIN_VELOCITY: f32 = 0.02;
/// A key hit less than this part of a step before a grid step is not hit again on that step: a note that starts on the
/// grid (or a little before it, humanized) does not flam.
const SAME_HIT: f64 = 0.5;

pub const PARAMS: [ParamInfo; 3] = [
    ParamInfo {
        name: "rate",
        label: L10n { en: "Rate", ru: "Шаг" },
        min: 0.0,
        max: 7.0,
        default: DEFAULT_RATE,
        kind: ParamKind::Choice,
        choices: &[
            L10n::same("1/4"),
            L10n::same("1/8"),
            L10n::same("1/8T"),
            L10n::same("1/16"),
            L10n::same("1/16T"),
            L10n::same("1/32"),
            L10n::same("1/32T"),
            L10n::same("1/64"),
        ],
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
        name: "ramp",
        label: L10n { en: "Velocity change per repeat, %", ru: "Изменение силы за повтор, %" },
        min: -50.0,
        max: 50.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
];

const KEYS: usize = 128;

pub(crate) struct Repeat {
    step_ticks: f64,
    /// The part of the step a hit sounds (0.1..=1).
    gate: f64,
    /// Velocity factor from one repeat to the next.
    ramp: f32,
    /// Velocity of each held key (0 means not held).
    held: [f32; KEYS],
    /// Hits since the key was pressed: the power of `ramp`.
    hits: [u16; KEYS],
    /// Tick of the key's last hit.
    last_hit: [f64; KEYS],
    /// Release tick of the key's sounding note.
    sounding: [Option<f64>; KEYS],
    /// Free clock position (ticks) at the start of the next segment, while the transport is stopped.
    free_tick: f64,
}

impl Repeat {
    pub fn new() -> Self {
        Self {
            step_ticks: f64::from(RATE_TICKS[DEFAULT_RATE as usize]),
            gate: 0.5,
            ramp: 1.0,
            held: [0.0; KEYS],
            hits: [0; KEYS],
            last_hit: [f64::NEG_INFINITY; KEYS],
            sounding: [None; KEYS],
            free_tick: 0.0,
        }
    }

    /// Releases, in time order, every sounding note that ends not later than `limit` (earlier than it if `strict`).
    fn release_due(&mut self, limit: f64, strict: bool, clock: Clock, output: &mut NoteBuffer) {
        loop {
            let due = (0..KEYS)
                .filter_map(|key| self.sounding[key].map(|off| (key, off)))
                .filter(|&(_, off)| off < limit || (!strict && off <= limit))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((key, off)) = due else { break };
            output.push(NoteEvent::off(clock.frame(off), key as u8));
            self.sounding[key] = None;
        }
    }

    /// Hits `key` at `tick` (frame `frame`); a note of it still sounding is cut first.
    fn hit(&mut self, key: usize, tick: f64, frame: usize, output: &mut NoteBuffer) {
        if self.sounding[key].take().is_some() {
            output.push(NoteEvent::off(frame, key as u8));
        }
        let velocity = (self.held[key] * self.ramp.powi(i32::from(self.hits[key]))).clamp(MIN_VELOCITY, 1.0);
        output.push(NoteEvent::on(frame, key as u8, velocity));
        self.hits[key] = self.hits[key].saturating_add(1);
        self.last_hit[key] = tick;
        self.sounding[key] = Some(tick + self.gate * self.step_ticks);
    }

    /// A grid step: notes that end by now are released, then every held key is hit again.
    fn step(&mut self, tick: f64, frame: usize, clock: Clock, output: &mut NoteBuffer) {
        self.release_due(tick, false, clock, output);
        for key in 0..KEYS {
            if self.held[key] > 0.0 && tick - self.last_hit[key] >= SAME_HIT * self.step_ticks {
                self.hit(key, tick, frame, output);
            }
        }
    }

    /// An input press or release. A first key on a stopped transport restarts the free clock: the press is tick 0,
    /// so the repeats follow it at even steps; `clock` and the step index `k` move with it.
    fn take(&mut self, event: NoteEvent, playing: bool, clock: &mut Clock, k: &mut i64, output: &mut NoteBuffer) {
        let key = usize::from(event.pitch & 0x7f);
        if !event.is_on() {
            self.held[key] = 0.0;
            self.release_due(clock.tick(event.frame), false, *clock, output);
            if self.sounding[key].take().is_some() {
                output.push(NoteEvent::off(event.frame, key as u8));
            }
            return;
        }
        if !playing && self.held.iter().all(|v| *v == 0.0) {
            let shift = -(event.frame as f64) * clock.tpf - clock.base;
            for off in self.sounding.iter_mut().flatten() {
                *off += shift;
            }
            clock.base += shift;
            *k = (clock.base / self.step_ticks).floor() as i64 - 1;
        }
        let tick = clock.tick(event.frame);
        self.release_due(tick, false, *clock, output);
        self.held[key] = event.velocity;
        self.hits[key] = 0;
        self.hit(key, tick, event.frame, output);
    }
}

impl NoteEffect for Repeat {
    fn set_param(&mut self, name: &str, value: f32) {
        let Some(value) = fit(&PARAMS, name, value) else {
            return;
        };
        match name {
            "rate" => self.step_ticks = f64::from(RATE_TICKS[value as usize]),
            "gate" => self.gate = f64::from(value) / 100.0,
            "ramp" => self.ramp = 1.0 + value / 100.0,
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.held = [0.0; KEYS];
        self.hits = [0; KEYS];
        self.last_hit = [f64::NEG_INFINITY; KEYS];
        self.sounding = [None; KEYS];
    }

    fn process(&mut self, input: &[NoteEvent], timing: &BlockTiming, output: &mut NoteBuffer) {
        let base = if timing.playing { timing.start_tick } else { self.free_tick };
        let mut clock = Clock { base, tpf: timing.ticks_per_frame, frames: timing.frames };
        if clock.tpf <= 0.0 {
            for &event in input {
                let key = usize::from(event.pitch & 0x7f);
                self.held[key] = event.velocity;
                if !event.is_on() && self.sounding[key].take().is_some() {
                    output.push(NoteEvent::off(event.frame, key as u8));
                }
            }
            return;
        }
        let mut next = 0;
        let mut k = (clock.base / self.step_ticks).floor() as i64 - 1;
        loop {
            let step = k as f64 * self.step_ticks;
            let step_frame = (step < clock.end()).then(|| clock.frame(step));
            if let Some(&event) = input.get(next).filter(|e| step_frame.is_none_or(|frame| e.frame <= frame)) {
                next += 1;
                self.take(event, timing.playing, &mut clock, &mut k, output);
                continue;
            }
            let Some(frame) = step_frame else { break };
            if step >= clock.base {
                self.step(step, frame, clock, output);
            }
            k += 1;
        }
        self.release_due(clock.end(), true, clock, output);
        if !timing.playing {
            self.free_tick = clock.end();
        }
    }
}
