//! Dynamic EQ: four peaking bands, each turning only while its own band is loud. A band listens through a band-pass at
//! its frequency and Q, to the signal itself or to the sidechain key; above the threshold (the compressor's soft-knee
//! curve, `dynamics.rs`) it moves by `(level − threshold) · (1 − 1/ratio)` dB in the direction of its `gain`, and at
//! most by `gain` itself. A negative gain cuts the band only when it gets loud (taming a resonance or harshness), a
//! positive one lifts it only when it is there; below the threshold the band is flat, so a new dynamic EQ changes
//! nothing. How far each band is engaged (0…1) follows its attack and release, and the band's filter — the EQ's peaking
//! biquad — is recomputed for its current gain while it moves. The bands report their current gains
//! (`Effect::band_gains_db`). No lookahead, no latency.
//! The graph in the interface draws the response with the bands at full gain and as they are now
//! (`app/src/effects/dynamicEqCurve.ts`): a change here must be mirrored there.

use daw_contracts::{Effect, L10n, MAX_EFFECT_BANDS, ParamInfo, StreamConfig};

use crate::biquad::{Biquad, Coefficients};
use crate::dynamics::{Follower, GainComputer, peak_db};
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, clamp_param, number, switch};

const BANDS: usize = MAX_EFFECT_BANDS;
/// The knee of every band's curve, dB.
const KNEE_DB: f32 = 6.0;
/// A band whose gain moved less than this since its filter was computed keeps the filter, dB.
const RECOMPUTE_DB: f32 = 0.01;

/// The table: seven params per band (`band<n>_…`, numbered from 1 as the interface shows them, spread over the range
/// by default) and the sidechain switch.
macro_rules! dynamic_eq_params {
    ($(($n:literal, $freq:literal)),*) => {
        [
            $(
                number(concat!("band", $n, "_freq"), L10n { en: concat!($n, ": Freq, Hz"), ru: concat!($n, ": Частота, Гц") }, 20.0, 20_000.0, $freq),
                number(concat!("band", $n, "_q"), L10n::same(concat!($n, ": Q")), 0.3, 10.0, 1.0),
                number(concat!("band", $n, "_gain"), L10n { en: concat!($n, ": Gain, dB"), ru: concat!($n, ": Усиление, дБ") }, -18.0, 18.0, 0.0),
                number(concat!("band", $n, "_threshold"), L10n { en: concat!($n, ": Threshold, dB"), ru: concat!($n, ": Порог, дБ") }, -60.0, 0.0, -24.0),
                number(concat!("band", $n, "_ratio"), L10n::same(concat!($n, ": Ratio")), 1.0, 20.0, 4.0),
                number(concat!("band", $n, "_attack"), L10n { en: concat!($n, ": Attack, ms"), ru: concat!($n, ": Attack, мс") }, 0.1, 100.0, 5.0),
                number(concat!("band", $n, "_release"), L10n { en: concat!($n, ": Release, ms"), ru: concat!($n, ": Release, мс") }, 10.0, 1_000.0, 120.0),
            )*
            switch("sidechain", L10n::same("Sidechain")),
        ]
    };
}

const PARAMS: [ParamInfo; 29] = dynamic_eq_params![(1, 100.0), (2, 500.0), (3, 2_000.0), (4, 8_000.0)];

/// `band2_gain` → band index 1 and "gain".
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
    /// Frequency as a natural log and Q, smoothed: the filters follow them frame by frame while they move.
    log_freq: Smoothed,
    q: Smoothed,
    /// The most the band turns, dB (signed), smoothed: a sign change glides through 0 dB.
    gain: Smoothed,
    /// How far the band is engaged now (0…1 of `gain`), with the band's attack and release.
    engaged: Follower,
    detector: Coefficients,
    detect: [Biquad; 2],
    /// The peaking filter for `applied` dB, and its state per channel.
    filter: Coefficients,
    filters: [Biquad; 2],
    applied: f32,
}

impl Band {
    fn new(freq: f32) -> Self {
        Self {
            curve: GainComputer { threshold: 0.0, ratio: 1.0, knee: KNEE_DB },
            attack_ms: 5.0,
            release_ms: 120.0,
            log_freq: Smoothed::new(freq.ln()),
            q: Smoothed::new(1.0),
            gain: Smoothed::new(0.0),
            engaged: Follower::default(),
            detector: Coefficients::PASS,
            detect: [Biquad::default(); 2],
            filter: Coefficients::PASS,
            filters: [Biquad::default(); 2],
            applied: 0.0,
        }
    }

    /// Both filters for the band's frequency and Q now, the peak at `gain_db`.
    fn compute(&mut self, sample_rate: f32, freq: f32, q: f32, gain_db: f32) {
        self.detector = Coefficients::band_pass(sample_rate, freq, q);
        self.filter = Coefficients::peaking(sample_rate, freq, gain_db, q);
        self.applied = gain_db;
    }
}

pub struct DynamicEq {
    sample_rate: f32,
    smoothing: f32,
    use_sidechain: bool,
    bands: [Band; BANDS],
    /// Each band's gain now, dB, for `band_gains_db`.
    gains: [f32; BANDS],
}

impl DynamicEq {
    fn update_times(&mut self) {
        self.smoothing = smooth_coefficient(self.sample_rate);
        for band in &mut self.bands {
            band.engaged.set_times(band.attack_ms, band.release_ms, self.sample_rate);
        }
    }
}

impl EffectKind for DynamicEq {
    const KIND: &'static str = "dynamic_eq";
    const LABEL: L10n = L10n { en: "Dynamic EQ", ru: "Динамический эквалайзер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut eq = Self {
            sample_rate: config.sample_rate as f32,
            smoothing: 0.0,
            use_sidechain: false,
            bands: [100.0, 500.0, 2_000.0, 8_000.0].map(Band::new),
            gains: [0.0; BANDS],
        };
        eq.update_times();
        eq.reset();
        eq
    }
}

impl Effect for DynamicEq {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update_times();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        if name == "sidechain" {
            self.use_sidechain = value >= 0.5;
            return;
        }
        let Some((index, what)) = band_param(name) else {
            return;
        };
        let band = &mut self.bands[index];
        match what {
            "freq" => band.log_freq.set(value.ln()),
            "q" => band.q.set(value),
            "gain" => band.gain.set(value),
            "threshold" => band.curve.threshold = value,
            "ratio" => band.curve.ratio = value,
            "attack" => band.attack_ms = value,
            "release" => band.release_ms = value,
            _ => {}
        }
        self.update_times();
    }

    fn reset(&mut self) {
        for band in &mut self.bands {
            band.log_freq.snap();
            band.q.snap();
            band.gain.snap();
            band.engaged.reset();
            band.detect = [Biquad::default(); 2];
            band.filters = [Biquad::default(); 2];
            let (freq, q) = (band.log_freq.next(self.smoothing).exp(), band.q.next(self.smoothing));
            band.compute(self.sample_rate, freq, q, 0.0);
        }
        self.gains = [0.0; BANDS];
    }

    fn process(&mut self, interleaved: &mut [f32], sidechain: Option<&[f32]>) {
        let mut key = sidechain.filter(|_| self.use_sidechain).map(|key| key.chunks_exact(2));
        for frame in interleaved.chunks_exact_mut(2) {
            let source = key.as_mut().and_then(Iterator::next).map_or([frame[0], frame[1]], |k| [k[0], k[1]]);
            for (band, now) in self.bands.iter_mut().zip(&mut self.gains) {
                let moving = !(band.log_freq.is_settled() && band.q.is_settled());
                let (freq, q) = (band.log_freq.next(self.smoothing).exp(), band.q.next(self.smoothing));
                let gain = band.gain.next(self.smoothing);
                let heard = [0, 1].map(|c| band.detect[c].process(&band.detector, source[c]));
                let amount = band.curve.reduction(peak_db(heard[0], heard[1]));
                let depth = gain.abs();
                let wanted = if depth > 1e-3 { (amount / depth).min(1.0) } else { 0.0 };
                *now = band.engaged.follow(wanted) * gain;
                if moving || (*now - band.applied).abs() > RECOMPUTE_DB {
                    band.compute(self.sample_rate, freq, q, *now);
                }
                for (state, sample) in band.filters.iter_mut().zip(frame.iter_mut()) {
                    *sample = state.process(&band.filter, *sample);
                }
            }
        }
    }

    fn band_gains_db(&self) -> &[f32] {
        &self.gains
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, sine, tone};

    fn render(set: &[(&str, f32)], input: &[f32], key: Option<&[f32]>) -> (DynamicEq, Vec<f32>) {
        let mut eq = build::<DynamicEq>(set.iter().copied());
        let mut output = input.to_vec();
        eq.process(&mut output, key);
        (eq, output)
    }

    /// The level of `hz` in the settled second half, relative to the input amplitude.
    fn kept(output: &[f32], hz: f32, amplitude: f32) -> f32 {
        tone(&output[output.len() / 2..], hz) / amplitude
    }

    const CUT_AT_1K: [(&str, f32); 4] = [("band3_freq", 1_000.0), ("band3_gain", -12.0), ("band3_threshold", -30.0), ("band3_ratio", 4.0)];

    #[test]
    fn a_new_dynamic_eq_changes_nothing() {
        let input = sine(1_000.0, 0.9, SR as usize);
        let (_, output) = render(&[], &input, None);
        assert!((kept(&output, 1_000.0, 0.9) - 1.0).abs() < 0.001);
    }

    #[test]
    fn a_band_cuts_only_while_it_is_loud_and_at_most_by_its_gain() {
        let (_, quiet) = render(&CUT_AT_1K, &sine(1_000.0, 0.01, SR as usize), None);
        assert!((kept(&quiet, 1_000.0, 0.01) - 1.0).abs() < 0.01, "−40 dB is below the threshold: untouched");

        let (eq, loud) = render(&CUT_AT_1K, &sine(1_000.0, 0.5, SR as usize), None);
        assert!((kept(&loud, 1_000.0, 0.5) - 0.25).abs() < 0.03, "{}", kept(&loud, 1_000.0, 0.5));
        assert!((eq.band_gains_db()[2] + 12.0).abs() < 0.5, "band 3 reports its gain: {:?}", eq.band_gains_db());
        assert!(eq.band_gains_db().iter().enumerate().all(|(i, g)| i == 2 || *g == 0.0));
    }

    #[test]
    fn a_band_leaves_other_frequencies_alone() {
        let input: Vec<f32> = sine(1_000.0, 0.5, SR as usize).iter().zip(sine(100.0, 0.2, SR as usize)).map(|(a, b)| a + b).collect();
        let (_, output) = render(&CUT_AT_1K, &input, None);
        assert!((kept(&output, 100.0, 0.2) - 1.0).abs() < 0.03, "{}", kept(&output, 100.0, 0.2));
    }

    #[test]
    fn a_positive_gain_lifts_the_band_only_while_it_is_loud() {
        let lift = [("band3_freq", 1_000.0), ("band3_gain", 6.0), ("band3_threshold", -30.0), ("band3_ratio", 4.0)];
        let (_, quiet) = render(&lift, &sine(1_000.0, 0.01, SR as usize), None);
        assert!((kept(&quiet, 1_000.0, 0.01) - 1.0).abs() < 0.01);
        let (_, loud) = render(&lift, &sine(1_000.0, 0.3, SR as usize), None);
        assert!((kept(&loud, 1_000.0, 0.3) - 2.0).abs() < 0.1, "+6 dB: {}", kept(&loud, 1_000.0, 0.3));
    }

    #[test]
    fn the_sidechain_key_drives_the_bands() {
        let set = [&CUT_AT_1K[..], &[("sidechain", 1.0)]].concat();
        let input = sine(1_000.0, 0.01, SR as usize);
        let (_, keyed) = render(&set, &input, Some(&sine(1_000.0, 0.5, SR as usize)));
        assert!(kept(&keyed, 1_000.0, 0.01) < 0.3, "a loud key cuts a quiet signal");
        let silent = vec![0.0; input.len()];
        let (_, free) = render(&set, &input, Some(&silent));
        assert!((kept(&free, 1_000.0, 0.01) - 1.0).abs() < 0.01, "a silent key leaves it alone");
    }

    #[test]
    fn moving_a_band_does_not_click() {
        let input = sine(900.0, 0.5, SR as usize);
        let held = |gain: f32, freq: f32| {
            max_step(&render(&[&CUT_AT_1K[..], &[("band3_gain", gain), ("band3_freq", freq)]].concat(), &input, None).1)
        };
        let baseline = [(-12.0, 700.0), (-12.0, 1_300.0), (12.0, 700.0), (12.0, 1_300.0)]
            .map(|(gain, freq)| held(gain, freq))
            .into_iter()
            .fold(0.0, f32::max);
        let mut eq = build::<DynamicEq>(CUT_AT_1K);
        let mut signal = input.clone();
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            eq.set_param("band3_gain", if block % 2 == 0 { -12.0 } else { 12.0 });
            eq.set_param("band3_freq", if block % 3 == 0 { 700.0 } else { 1_300.0 });
            eq.process(chunk, None);
        }
        assert!(max_step(&signal) < baseline * 1.5, "{} with {baseline}", max_step(&signal));
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut eq = build::<DynamicEq>([]);
        assert_eq!(eq.latency_samples(), 0);
        assert_latency_at_impulse(&mut eq);
    }
}
