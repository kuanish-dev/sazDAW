//! Multiband compressor: a Linkwitz–Riley crossover (`crossover.rs`) splits the sound into four bands, or into three
//! (then the top two work as one band with the settings of band 3), each band goes through its own compressor (the
//! shared soft-knee curve and attack/release, `dynamics.rs`) with its own gain, solo and bypass, and the bands are
//! summed back. With every band bypassed the output is the input up to an all-pass phase: no level change at any
//! frequency. Each band reports what it takes off (`Effect::band_gains_db`, 0 dB or less) and the deepest of them is
//! the effect's gain reduction. No lookahead, so no latency.
//! The graph in the interface draws the split by the same filters and each band's reduction
//! (`app/src/effects/multibandCurve.ts`): a change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, StreamConfig};

use crate::crossover::{BANDS, Crossover, SPLITS, Split};
use crate::dynamics::{Follower, GainComputer, peak_db};
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, choice, clamp_param, db_to_gain, number, switch};

/// The knee of every band's curve, dB: soft enough for a mix bus, so the bands have no knob for it.
const KNEE_DB: f32 = 6.0;

const BAND_COUNTS: [L10n; 2] = [L10n::same("3"), L10n::same("4")];

/// The table: the three split frequencies, the band count, seven params per band (`band<n>_…`, numbered from 1 as the
/// interface shows them, with their own attack and release defaults: slower for the lows) and the output.
macro_rules! multiband_params {
    ($(($n:literal, $attack:literal, $release:literal)),*) => {
        [
            number("split1", L10n { en: "Split 1–2, Hz", ru: "Раздел 1–2, Гц" }, 20.0, 2_000.0, 120.0),
            number("split2", L10n { en: "Split 2–3, Hz", ru: "Раздел 2–3, Гц" }, 100.0, 8_000.0, 1_000.0),
            number("split3", L10n { en: "Split 3–4, Hz", ru: "Раздел 3–4, Гц" }, 500.0, 18_000.0, 6_000.0),
            choice("bands", L10n { en: "Bands", ru: "Полосы" }, &BAND_COUNTS, 1.0),
            $(
                number(concat!("band", $n, "_threshold"), L10n { en: concat!($n, ": Threshold, dB"), ru: concat!($n, ": Порог, дБ") }, -60.0, 0.0, -20.0),
                number(concat!("band", $n, "_ratio"), L10n::same(concat!($n, ": Ratio")), 1.0, 20.0, 3.0),
                number(concat!("band", $n, "_attack"), L10n { en: concat!($n, ": Attack, ms"), ru: concat!($n, ": Attack, мс") }, 0.1, 100.0, $attack),
                number(concat!("band", $n, "_release"), L10n { en: concat!($n, ": Release, ms"), ru: concat!($n, ": Release, мс") }, 10.0, 1_000.0, $release),
                number(concat!("band", $n, "_gain"), L10n { en: concat!($n, ": Gain, dB"), ru: concat!($n, ": Усиление, дБ") }, -12.0, 12.0, 0.0),
                switch(concat!("band", $n, "_solo"), L10n { en: concat!($n, ": Solo"), ru: concat!($n, ": Соло") }),
                switch(concat!("band", $n, "_bypass"), L10n { en: concat!($n, ": Bypass"), ru: concat!($n, ": Обход") }),
            )*
            number("output", L10n { en: "Output, dB", ru: "Выход, дБ" }, -24.0, 12.0, 0.0),
        ]
    };
}

const PARAMS: [ParamInfo; 33] = multiband_params![(1, 20.0, 200.0), (2, 10.0, 150.0), (3, 5.0, 100.0), (4, 3.0, 80.0)];

/// `band3_solo` → band index 2 and "solo".
fn band_param(name: &str) -> Option<(usize, &str)> {
    let (number, what) = name.strip_prefix("band")?.split_once('_')?;
    let index = number.parse::<usize>().ok()?.checked_sub(1)?;
    (index < BANDS).then_some((index, what))
}

#[derive(Clone, Copy)]
struct Band {
    curve: GainComputer,
    attack_ms: f32,
    release_ms: f32,
    /// What the band takes off now, dB (0 or more).
    reduction: Follower,
    /// The band's own gain after the compression, dB.
    gain: Smoothed,
    solo: bool,
    /// 1 while the band compresses, 0 while it is bypassed (it then passes at unity gain).
    engaged: Smoothed,
    /// 1 while the band is heard, 0 while another band is soloed.
    heard: Smoothed,
}

pub struct Multiband {
    sample_rate: f32,
    smoothing: f32,
    /// The split frequencies as set, Hz; each one is kept at least at the one below it.
    split_hz: [f32; SPLITS],
    /// The split frequencies as natural logs, smoothed, and the filters they give now.
    log_splits: [Smoothed; SPLITS],
    splits: [Split; SPLITS],
    four_bands: bool,
    /// 1 with four bands, 0 with three (the top two as one band): a change of the count crossfades.
    four: Smoothed,
    bands: [Band; BANDS],
    output: Smoothed,
    channels: [Crossover; 2],
    /// What each band does now, dB (0 or less), for `band_gains_db`.
    gains: [f32; BANDS],
}

impl Multiband {
    fn band_count(&self) -> usize {
        if self.four_bands { BANDS } else { BANDS - 1 }
    }

    fn update_times(&mut self) {
        self.smoothing = smooth_coefficient(self.sample_rate);
        for band in &mut self.bands {
            band.reduction.set_times(band.attack_ms, band.release_ms, self.sample_rate);
        }
    }

    /// The smoothed splits head for the set frequencies, each at least at the one below it.
    fn order_splits(&mut self) {
        let mut floor = 0f32;
        for (log, &hz) in self.log_splits.iter_mut().zip(&self.split_hz) {
            floor = floor.max(hz);
            log.set(floor.ln());
        }
    }

    /// While a split moves, its filters follow it frame by frame.
    fn advance_splits(&mut self) {
        for (log, split) in self.log_splits.iter_mut().zip(&mut self.splits) {
            if !log.is_settled() {
                *split = Split::new(self.sample_rate, log.next(self.smoothing).exp());
            }
        }
    }

    /// A soloed band among those in use silences the others.
    fn update_solo(&mut self) {
        let count = self.band_count();
        let any = self.bands[..count].iter().any(|b| b.solo);
        for band in &mut self.bands {
            band.heard.set(if !any || band.solo { 1.0 } else { 0.0 });
        }
    }
}

impl EffectKind for Multiband {
    const KIND: &'static str = "multiband";
    const LABEL: L10n = L10n { en: "Multiband compressor", ru: "Многополосный компрессор" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let sample_rate = config.sample_rate as f32;
        let band = Band {
            curve: GainComputer { threshold: 0.0, ratio: 1.0, knee: KNEE_DB },
            attack_ms: 10.0,
            release_ms: 100.0,
            reduction: Follower::default(),
            gain: Smoothed::new(0.0),
            solo: false,
            engaged: Smoothed::new(1.0),
            heard: Smoothed::new(1.0),
        };
        let mut multiband = Self {
            sample_rate,
            smoothing: 0.0,
            split_hz: [120.0, 1_000.0, 6_000.0],
            log_splits: [Smoothed::new(1_000f32.ln()); SPLITS],
            splits: [Split::new(sample_rate, 1_000.0); SPLITS],
            four_bands: true,
            four: Smoothed::new(1.0),
            bands: [band; BANDS],
            output: Smoothed::new(1.0),
            channels: [Crossover::default(); 2],
            gains: [0.0; BANDS],
        };
        multiband.update_times();
        multiband.order_splits();
        multiband.reset();
        multiband
    }
}

impl Effect for Multiband {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update_times();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        if let Some((index, what)) = band_param(name) {
            let band = &mut self.bands[index];
            match what {
                "threshold" => band.curve.threshold = value,
                "ratio" => band.curve.ratio = value,
                "attack" => band.attack_ms = value,
                "release" => band.release_ms = value,
                "gain" => band.gain.set(value),
                "solo" => band.solo = value >= 0.5,
                "bypass" => band.engaged.set(if value >= 0.5 { 0.0 } else { 1.0 }),
                _ => {}
            }
            self.update_times();
            self.update_solo();
            return;
        }
        match name {
            "split1" => self.split_hz[0] = value,
            "split2" => self.split_hz[1] = value,
            "split3" => self.split_hz[2] = value,
            "bands" => {
                self.four_bands = value >= 1.0;
                self.four.set(if self.four_bands { 1.0 } else { 0.0 });
                self.update_solo();
            }
            "output" => self.output.set(db_to_gain(value)),
            _ => {}
        }
        self.order_splits();
    }

    fn reset(&mut self) {
        for (log, split) in self.log_splits.iter_mut().zip(&mut self.splits) {
            log.snap();
            *split = Split::new(self.sample_rate, log.next(self.smoothing).exp());
        }
        self.four.snap();
        self.output.snap();
        for band in &mut self.bands {
            band.reduction.reset();
            band.gain.snap();
            band.engaged.snap();
            band.heard.snap();
        }
        self.channels.iter_mut().for_each(Crossover::reset);
        self.gains = [0.0; BANDS];
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            self.advance_splits();
            let four = self.four.next(self.smoothing);
            let mut left = self.channels[0].split(&self.splits, frame[0]);
            let mut right = self.channels[1].split(&self.splits, frame[1]);
            for bands in [&mut left, &mut right] {
                bands[BANDS - 2] += (1.0 - four) * bands[BANDS - 1];
                bands[BANDS - 1] *= four;
            }
            let mut out = [0.0; 2];
            for (index, band) in self.bands.iter_mut().enumerate() {
                let reduction = band.reduction.follow(band.curve.reduction(peak_db(left[index], right[index])));
                let engaged = band.engaged.next(self.smoothing);
                let gain = band.heard.next(self.smoothing) * db_to_gain(engaged * (band.gain.next(self.smoothing) - reduction));
                self.gains[index] = -engaged * reduction;
                out[0] += left[index] * gain;
                out[1] += right[index] * gain;
            }
            let output = self.output.next(self.smoothing);
            frame[0] = out[0] * output;
            frame[1] = out[1] * output;
        }
    }

    fn gain_reduction_db(&self) -> Option<f32> {
        Some(self.band_gains_db().iter().fold(0.0, |deepest, gain| deepest.max(-gain)))
    }

    fn band_gains_db(&self) -> &[f32] {
        &self.gains[..self.band_count()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, sine, tone};

    /// Two sines at once: a bass note and a high one.
    fn two_tones(low: (f32, f32), high: (f32, f32), frames: usize) -> Vec<f32> {
        sine(low.0, low.1, frames).iter().zip(sine(high.0, high.1, frames)).map(|(a, b)| a + b).collect()
    }

    fn render(set: &[(&str, f32)], input: &[f32]) -> (Multiband, Vec<f32>) {
        let mut multiband = build::<Multiband>(set.iter().copied());
        let mut output = input.to_vec();
        multiband.process(&mut output, None);
        (multiband, output)
    }

    const BYPASS_ALL: [(&str, f32); 4] = [("band1_bypass", 1.0), ("band2_bypass", 1.0), ("band3_bypass", 1.0), ("band4_bypass", 1.0)];

    #[test]
    fn bypassed_bands_give_back_the_level_of_the_input_at_every_frequency() {
        for hz in [50.0, 120.0, 500.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0] {
            let input = sine(hz, 0.8, SR as usize);
            let (_, output) = render(&BYPASS_ALL, &input);
            let level = tone(&output[SR as usize..], hz);
            assert!((level / 0.8 - 1.0).abs() < 0.01, "{hz} Hz: {level}");
        }
    }

    #[test]
    fn a_loud_band_is_compressed_and_the_others_are_left_alone() {
        let input = two_tones((60.0, 0.9), (3_000.0, 0.05), SR as usize);
        let (multiband, output) = render(&[("band1_threshold", -30.0), ("band1_ratio", 8.0)], &input);
        let settled = &output[SR as usize..];
        assert!(tone(settled, 60.0) < 0.9 * 0.25, "the bass is turned down: {}", tone(settled, 60.0));
        assert!((tone(settled, 3_000.0) / 0.05 - 1.0).abs() < 0.03, "the quiet high band is untouched");

        let gains = multiband.band_gains_db();
        assert_eq!(gains.len(), 4);
        assert!(gains[0] < -10.0, "band 1 reports its reduction: {gains:?}");
        assert!(gains[1..].iter().all(|g| *g > -0.5), "{gains:?}");
        assert_eq!(multiband.gain_reduction_db(), Some(-gains[0]), "the deepest band is the effect's reduction");
    }

    #[test]
    fn a_soloed_band_is_the_only_one_heard() {
        let input = two_tones((60.0, 0.5), (3_000.0, 0.5), SR as usize);
        let (_, output) = render(&[&BYPASS_ALL[..], &[("band3_solo", 1.0)]].concat(), &input);
        let settled = &output[SR as usize..];
        assert!(tone(settled, 60.0) < 0.005, "band 1 is silent: {}", tone(settled, 60.0));
        assert!(tone(settled, 3_000.0) > 0.4, "band 3 plays: {}", tone(settled, 3_000.0));
    }

    #[test]
    fn three_bands_turn_the_top_two_as_one_with_the_settings_of_band_3() {
        let input = sine(10_000.0, 0.9, SR as usize);
        let set = [("bands", 0.0), ("band3_threshold", -40.0), ("band3_ratio", 10.0), ("band4_threshold", 0.0)];
        let (multiband, output) = render(&set, &input);
        assert_eq!(multiband.band_gains_db().len(), 3);
        assert!(tone(&output[SR as usize..], 10_000.0) < 0.9 * 0.1, "band 3's settings act on 10 kHz");
    }

    #[test]
    fn gain_and_output_raise_the_band() {
        let input = sine(500.0, 0.1, SR as usize);
        let (_, output) = render(
            &[&BYPASS_ALL[..], &[("band2_gain", 6.0), ("band2_bypass", 0.0), ("band2_threshold", 0.0), ("output", 6.0)]].concat(),
            &input,
        );
        assert!((tone(&output[SR as usize..], 500.0) / 0.4 - 1.0).abs() < 0.05, "+6 dB band, +6 dB output");
    }

    #[test]
    fn switching_bands_solo_bypass_and_splits_does_not_click() {
        let input = two_tones((80.0, 0.4), (2_500.0, 0.2), SR as usize);
        let baseline = max_step(&render(&[], &input).1);
        let mut multiband = build::<Multiband>([]);
        let mut signal = input.clone();
        let on = |yes: bool| if yes { 1.0 } else { 0.0 };
        for (block, chunk) in signal.chunks_mut(512).enumerate() {
            multiband.set_param("band2_solo", on(block % 3 == 0));
            multiband.set_param("band4_bypass", on(block % 2 == 0));
            multiband.set_param("bands", on(block % 4 < 2));
            multiband.set_param("split2", if block % 5 == 0 { 400.0 } else { 1_600.0 });
            multiband.process(chunk, None);
        }
        assert!(max_step(&signal) < baseline * 1.6, "{} with {baseline}", max_step(&signal));
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut multiband = build::<Multiband>(BYPASS_ALL);
        assert_eq!(multiband.latency_samples(), 0);
        assert_latency_at_impulse(&mut multiband);
    }
}
