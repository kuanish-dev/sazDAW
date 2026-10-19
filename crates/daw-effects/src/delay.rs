//! Stereo delay with feedback, ping-pong mode and time synced to the project tempo.
//! A new time comes in through a short crossfade (`FADE_MS`) from the old echo to the new one: a jump of the read
//! position is heard as a click, and a fast glide of the time as a pitch squeal.
//! `low_cut` and `high_cut` sit in the loop: the echo is filtered as it is read, so the first repeat passes the filters
//! once, the second twice, and every repeat comes back thinner and darker (a tape or analog delay); at their ends they
//! are off and the echo is untouched.
//! The graph in the interface draws the echoes from the same rules (`SYNC_QUARTERS`, feedback, ping-pong, the mix) in
//! `app/src/effects/delayTaps.ts`: a change here must be mirrored there. The loop filters change the echoes' tone, not
//! their level in the passband, so the graph leaves them out.

use daw_contracts::{BlockTiming, Bpm, Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::biquad::{BUTTERWORTH_Q, Biquad, Coefficients};
use crate::{EffectKind, clamp_param, mix_over_dry, number};

const MAX_SECONDS: f32 = 2.0;
/// The loop filters at these ends are off.
const LOW_CUT_OFF_HZ: f32 = 20.0;
const HIGH_CUT_OFF_HZ: f32 = 20_000.0;
/// Length of the crossfade from the old time to the new one.
const FADE_MS: f32 = 20.0;

/// Tempo sync choices and their lengths in quarter notes (the first is "Off": the time comes from "Time, ms").
const SYNC: [L10n; 12] = [
    L10n { en: "Off", ru: "Выкл" },
    L10n::same("1/2"),
    L10n::same("1/4"),
    L10n { en: "1/4 dotted", ru: "1/4 с точкой" },
    L10n { en: "1/4 triplet", ru: "1/4 триоль" },
    L10n::same("1/8"),
    L10n { en: "1/8 dotted", ru: "1/8 с точкой" },
    L10n { en: "1/8 triplet", ru: "1/8 триоль" },
    L10n::same("1/16"),
    L10n { en: "1/16 dotted", ru: "1/16 с точкой" },
    L10n { en: "1/16 triplet", ru: "1/16 триоль" },
    L10n::same("1/32"),
];
const SYNC_QUARTERS: [f64; 12] = [0.0, 2.0, 1.0, 1.5, 2.0 / 3.0, 0.5, 0.75, 1.0 / 3.0, 0.25, 0.375, 1.0 / 6.0, 0.125];

const PARAMS: [ParamInfo; 7] = [
    ParamInfo {
        name: "time",
        label: L10n { en: "Time, ms", ru: "Время, мс" },
        min: 1.0,
        max: 2_000.0,
        default: 350.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo { name: "feedback", label: L10n::same("Feedback"), min: 0.0, max: 0.95, default: 0.4, kind: ParamKind::Number, choices: &[] },
    ParamInfo {
        name: "mix",
        label: L10n { en: "Mix", ru: "Микс" },
        min: 0.0,
        max: 1.0,
        default: 0.3,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "ping_pong",
        label: L10n::same("Ping-pong"),
        min: 0.0,
        max: 1.0,
        default: 0.0,
        kind: ParamKind::Switch,
        choices: &[],
    },
    ParamInfo {
        name: "sync",
        label: L10n { en: "Tempo sync", ru: "По темпу" },
        min: 0.0,
        max: 11.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &SYNC,
    },
    number("low_cut", L10n { en: "Echo low cut, Hz", ru: "Срез низов эха, Гц" }, LOW_CUT_OFF_HZ, 2_000.0, LOW_CUT_OFF_HZ),
    number("high_cut", L10n { en: "Echo high cut, Hz", ru: "Срез верхов эха, Гц" }, 1_000.0, HIGH_CUT_OFF_HZ, HIGH_CUT_OFF_HZ),
];

pub struct Delay {
    sample_rate: f32,
    lines: [Vec<f32>; 2],
    write: usize,
    /// The delay that sounds and the one the parameters and tempo ask for (in frames).
    delay_samples: usize,
    target_samples: usize,
    /// The delay the crossfade comes from and how many crossfade frames are left.
    fade_from: usize,
    fade_left: usize,
    fade_frames: usize,
    time_ms: f32,
    /// Index of the `SYNC` choice; 0 means no sync.
    sync: usize,
    tempo: f64,
    feedback: f32,
    mix: f32,
    ping_pong: bool,
    low_cut_hz: f32,
    high_cut_hz: f32,
    /// The loop filters: low cut, then high cut.
    loop_filters: [Coefficients; 2],
    /// Their memory, per channel.
    loop_state: [[Biquad; 2]; 2],
}

impl Delay {
    /// The loop filters for the current cuts: a filter at its end passes everything.
    fn tune_filters(&mut self) {
        let sample_rate = self.sample_rate;
        let low = self.low_cut_hz;
        let high = self.high_cut_hz.min(0.45 * sample_rate);
        self.loop_filters = [
            if low <= LOW_CUT_OFF_HZ { Coefficients::PASS } else { Coefficients::high_pass(sample_rate, low, BUTTERWORTH_Q) },
            if self.high_cut_hz >= HIGH_CUT_OFF_HZ { Coefficients::PASS } else { Coefficients::low_pass(sample_rate, high, BUTTERWORTH_Q) },
        ];
    }

    fn resize(&mut self) {
        let len = (MAX_SECONDS * self.sample_rate) as usize + 2;
        self.lines = [vec![0.0; len], vec![0.0; len]];
        self.write = 0;
        self.fade_frames = (FADE_MS * 0.001 * self.sample_rate) as usize;
        self.loop_state = [[Biquad::default(); 2]; 2];
        self.tune_filters();
        self.update();
        self.settle();
    }

    /// Straight to the target time with no crossfade: after a reset the lines are empty, there is nothing to fade from.
    fn settle(&mut self) {
        self.delay_samples = self.target_samples;
        self.fade_left = 0;
    }

    /// Target delay in whole frames: from the tempo when sync is on, otherwise from milliseconds.
    fn update(&mut self) {
        let seconds = if self.sync == 0 { f64::from(self.time_ms) * 0.001 } else { 60.0 / self.tempo * SYNC_QUARTERS[self.sync] };
        let max = self.lines[0].len().saturating_sub(1).max(1) as f64;
        self.target_samples = (seconds * f64::from(self.sample_rate)).round().clamp(1.0, max) as usize;
    }

    /// Echo of both channels for the current frame; during a crossfade a mix of the old and the new.
    fn echo(&mut self) -> [f32; 2] {
        if self.fade_left == 0 && self.delay_samples != self.target_samples {
            self.fade_from = self.delay_samples;
            self.delay_samples = self.target_samples;
            self.fade_left = self.fade_frames;
        }
        let len = self.lines[0].len();
        let read = |delay: usize, line: &Vec<f32>| line[(self.write + len - delay) % len];
        let new = [read(self.delay_samples, &self.lines[0]), read(self.delay_samples, &self.lines[1])];
        if self.fade_left == 0 {
            return new;
        }
        let old = [read(self.fade_from, &self.lines[0]), read(self.fade_from, &self.lines[1])];
        let t = self.fade_left as f32 / self.fade_frames.max(1) as f32;
        self.fade_left -= 1;
        [new[0] + (old[0] - new[0]) * t, new[1] + (old[1] - new[1]) * t]
    }
}

impl EffectKind for Delay {
    const KIND: &'static str = "delay";
    const LABEL: L10n = L10n { en: "Delay", ru: "Дилей" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut delay = Self {
            sample_rate: config.sample_rate as f32,
            lines: [Vec::new(), Vec::new()],
            write: 0,
            delay_samples: 1,
            target_samples: 1,
            fade_from: 1,
            fade_left: 0,
            fade_frames: 0,
            time_ms: 0.0,
            sync: 0,
            tempo: Bpm::DEFAULT.get(),
            feedback: 0.0,
            mix: 0.0,
            ping_pong: false,
            low_cut_hz: LOW_CUT_OFF_HZ,
            high_cut_hz: HIGH_CUT_OFF_HZ,
            loop_filters: [Coefficients::PASS; 2],
            loop_state: [[Biquad::default(); 2]; 2],
        };
        delay.resize();
        delay
    }
}

impl Effect for Delay {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.resize();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "time" => self.time_ms = value,
            "feedback" => self.feedback = value,
            "mix" => self.mix = value,
            "ping_pong" => self.ping_pong = value >= 0.5,
            "sync" => self.sync = value as usize,
            "low_cut" => {
                self.low_cut_hz = value;
                self.tune_filters();
            }
            "high_cut" => {
                self.high_cut_hz = value;
                self.tune_filters();
            }
            _ => {}
        }
        self.update();
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.loop_state = [[Biquad::default(); 2]; 2];
        self.settle();
    }

    fn set_timing(&mut self, timing: &BlockTiming) {
        if self.tempo != timing.tempo.get() {
            self.tempo = timing.tempo.get();
            self.update();
        }
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        let len = self.lines[0].len();
        for frame in interleaved.chunks_exact_mut(2) {
            let raw = self.echo();
            let filters = &self.loop_filters;
            let echo = [0, 1].map(|c| {
                let [low, high] = &mut self.loop_state[c];
                high.process(&filters[1], low.process(&filters[0], raw[c]))
            });
            let dry = [frame[0], frame[1]];

            let feed = if self.ping_pong {
                // Crossed echo: the left feeds the right line and back; the input goes into the left one.
                [dry[0] + echo[1] * self.feedback, echo[0] * self.feedback]
            } else {
                [dry[0] + echo[0] * self.feedback, dry[1] + echo[1] * self.feedback]
            };
            self.lines[0][self.write] = feed[0];
            self.lines[1][self.write] = feed[1];
            self.write = (self.write + 1) % len;

            frame[0] = mix_over_dry(dry[0], echo[0], self.mix);
            frame[1] = mix_over_dry(dry[1], echo[1], self.mix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, impulse, max_step, sine};
    use daw_contracts::TimeSignature;

    fn timing(bpm: f64) -> BlockTiming {
        BlockTiming {
            frames: 512,
            playing: true,
            start_tick: 0.0,
            ticks_per_frame: 0.0,
            tempo: Bpm::new(bpm).unwrap(),
            time_signature: TimeSignature::FOUR_FOUR,
            signature_start: 0.0,
            loop_region: None,
        }
    }

    /// Frame of the impulse's first echo (the loudest frame of the left channel after the input).
    fn first_echo(d: &mut Delay) -> usize {
        let mut buffer = impulse(SR as usize * 2);
        d.process(&mut buffer, None);
        (1..buffer.len() / 2).max_by(|&a, &b| buffer[a * 2].total_cmp(&buffer[b * 2])).unwrap()
    }

    #[test]
    fn synced_time_follows_the_tempo() {
        // (choice, quarter notes): 1/4, dotted 1/8, 1/16 triplet.
        for (sync, quarters) in [(2.0, 1.0), (6.0, 0.75), (10.0, 1.0 / 6.0)] {
            for bpm in [90.0, 120.0, 174.0] {
                let mut d = build::<Delay>([("sync", sync), ("feedback", 0.0), ("mix", 1.0)]);
                d.set_timing(&timing(bpm));
                // Set the new time before the first sound at once: compare the time itself, not a crossfade to it.
                d.reset();
                let expected = 60.0 / bpm * quarters * f64::from(SR);
                let echo = first_echo(&mut d) as f64;
                assert!((echo - expected).abs() <= 1.0, "{sync} at {bpm}: {echo} instead of {expected}");
            }
        }
    }

    #[test]
    fn sync_off_uses_milliseconds_and_ignores_the_tempo() {
        let mut d = build::<Delay>([("time", 100.0), ("feedback", 0.0), ("mix", 1.0)]);
        d.set_timing(&timing(60.0));
        assert_eq!(first_echo(&mut d), 4_800);
    }

    /// Changing time and tempo mid-sound gives no click: the old echo crossfades into the new one.
    #[test]
    fn time_changes_do_not_click() {
        let mut d = build::<Delay>([("time", 300.0), ("feedback", 0.3), ("mix", 1.0)]);
        let mut signal = sine(220.0, 0.3, SR as usize * 2);
        let sine_step = max_step(&signal);
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            match block {
                100 => d.set_param("time", 120.0),
                200 => d.set_param("sync", 5.0),
                250 => d.set_timing(&timing(140.0)),
                _ => {}
            }
            d.process(chunk, None);
        }
        // The dry sine and the echo add up, so the step is larger than for one sine, but far from a click (~0.3).
        assert!(max_step(&signal) < sine_step * 4.0, "{} with {sine_step}", max_step(&signal));
    }

    #[test]
    fn echo_arrives_after_the_set_time_and_repeats() {
        let mut d = build::<Delay>([("time", 100.0), ("feedback", 0.5), ("mix", 1.0)]);
        let mut buffer = impulse(20_000);
        d.process(&mut buffer, None);
        let at = |frame: usize| buffer[frame * 2];
        assert!((at(4_800) - 1.0).abs() < 1e-5, "first echo");
        assert!((at(9_600) - 0.5).abs() < 1e-5, "the second echo is half as loud");
        assert!(at(2_000).abs() < 1e-5, "silence between echoes");
    }

    #[test]
    fn zero_mix_is_dry() {
        let mut d = build::<Delay>([("mix", 0.0)]);
        let original = impulse(2_000);
        let mut buffer = original.clone();
        d.process(&mut buffer, None);
        assert_eq!(buffer, original);
    }

    #[test]
    fn ping_pong_moves_the_echo_between_channels() {
        let mut d = build::<Delay>([("time", 50.0), ("feedback", 0.8), ("mix", 1.0), ("ping_pong", 1.0)]);
        let mut buffer = impulse(10_000);
        d.process(&mut buffer, None);
        // The first echo is on the left (from the input), the second on the right.
        assert!(buffer[2_400 * 2] > 0.5 && buffer[2_400 * 2 + 1].abs() < 1e-5);
        assert!(buffer[4_800 * 2 + 1] > 0.1 && buffer[4_800 * 2].abs() < 1e-5);
    }

    /// RMS of the first echo of a short burst of `hz` (100 ms later, no feedback), with `set` on top.
    fn echo_level(hz: f32, set: &[(&str, f32)]) -> f32 {
        let mut d = build::<Delay>([("time", 100.0), ("feedback", 0.0), ("mix", 1.0)].into_iter().chain(set.iter().copied()));
        let mut buffer = sine(hz, 0.5, SR as usize / 2);
        buffer[2 * 2_400..].fill(0.0);
        d.process(&mut buffer, None);
        crate::testing::rms(&buffer[2 * 5_000..2 * 6_800])
    }

    #[test]
    fn loop_filters_thin_and_darken_the_echo() {
        let (low, high) = (echo_level(80.0, &[]), echo_level(8_000.0, &[]));
        assert!(echo_level(80.0, &[("low_cut", 1_000.0)]) < low * 0.1, "the low cut takes the lows out of the echo");
        assert!(echo_level(8_000.0, &[("high_cut", 1_500.0)]) < high * 0.1, "the high cut takes the highs out");
        assert!(echo_level(1_000.0, &[("low_cut", 100.0), ("high_cut", 10_000.0)]) > echo_level(1_000.0, &[]) * 0.9, "the middle stays");
    }

    #[test]
    fn every_repeat_passes_the_loop_filters_again() {
        let mut d = build::<Delay>([("time", 50.0), ("feedback", 0.9), ("mix", 1.0), ("high_cut", 3_000.0)]);
        let mut buffer = sine(6_000.0, 0.5, SR as usize);
        buffer[2 * 1_200..].fill(0.0);
        d.process(&mut buffer, None);
        let echo = |n: usize| crate::testing::rms(&buffer[2 * (n * 2_400 + 100)..2 * (n * 2_400 + 1_100)]);
        assert!(echo(2) < echo(1) * 0.9 * 0.5 && echo(3) < echo(2) * 0.9 * 0.5, "{} {} {}", echo(1), echo(2), echo(3));
    }

    #[test]
    fn feedback_never_blows_up() {
        let mut d = build::<Delay>([("time", 5.0), ("feedback", 0.95), ("mix", 1.0)]);
        let mut buffer = impulse(200_000);
        d.process(&mut buffer, None);
        assert!(buffer.iter().all(|v| v.is_finite() && v.abs() <= 1.0 + 1e-3));
    }
}
