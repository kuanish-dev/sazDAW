//! De-esser: hears the sibilance band (a band-pass around `freq`, by default in the 4–10 kHz of "s" and "sh") and turns
//! it down while it is louder than the threshold, by at most `range`. Split-band mode takes the reduction off that band
//! only: `input − (1 − g) · band` (a 0-dB band-pass and its notch sum to the input, so with no reduction the sound is
//! untouched, phase included); wideband mode turns the whole sound down by the same gain, which keeps a voice's tone
//! but ducks it. "Listen" plays the detected band alone, to find the frequency by ear. The detector is the compressor's
//! curve (`dynamics.rs`) with a fixed fast attack and short release, because sibilants are short. Mode, listen and the
//! frequency glide, so switching does not click. Reports its gain reduction. No lookahead, no latency.
//! The graph in the interface draws the band and the response it makes now (`app/src/effects/deEsserCurve.ts`): a
//! change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, StreamConfig};

use crate::biquad::{Biquad, Coefficients};
use crate::dynamics::{Follower, GainComputer, peak_db};
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, choice, clamp_param, db_to_gain, number, switch};

/// Width of the detected band: about an octave and a half around the frequency (−3 dB at 0.6× and 1.6×).
const BAND_Q: f32 = 1.0;
/// Above the threshold the band is turned down hard, and `range` caps it: a de-esser should act on the "s" only.
const RATIO: f32 = 6.0;
const KNEE_DB: f32 = 4.0;
const ATTACK_MS: f32 = 0.5;
const RELEASE_MS: f32 = 60.0;

const MODES: [L10n; 2] = [L10n { en: "Split-band", ru: "Полоса" }, L10n { en: "Wideband", ru: "Весь сигнал" }];

const PARAMS: [ParamInfo; 5] = [
    number("freq", L10n { en: "Frequency, Hz", ru: "Частота, Гц" }, 2_000.0, 12_000.0, 6_500.0),
    number("threshold", L10n { en: "Threshold, dB", ru: "Порог, дБ" }, -60.0, 0.0, -30.0),
    number("range", L10n { en: "Range, dB", ru: "Глубина, дБ" }, 0.0, 24.0, 10.0),
    choice("mode", L10n { en: "Mode", ru: "Режим" }, &MODES, 0.0),
    switch("listen", L10n { en: "Listen to the band", ru: "Слушать полосу" }),
];

pub struct DeEsser {
    sample_rate: f32,
    smoothing: f32,
    curve: GainComputer,
    range: f32,
    log_freq: Smoothed,
    band: Coefficients,
    bands: [Biquad; 2],
    /// 1 split-band, 0 wideband.
    split: Smoothed,
    /// 1 while the detected band is played alone.
    listen: Smoothed,
    /// What is taken off now, dB (0 or more).
    reduction: Follower,
}

impl DeEsser {
    fn update(&mut self) {
        self.smoothing = smooth_coefficient(self.sample_rate);
        self.reduction.set_times(ATTACK_MS, RELEASE_MS, self.sample_rate);
    }
}

impl EffectKind for DeEsser {
    const KIND: &'static str = "de_esser";
    const LABEL: L10n = L10n { en: "De-esser", ru: "Де-эссер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut de_esser = Self {
            sample_rate: config.sample_rate as f32,
            smoothing: 0.0,
            curve: GainComputer { threshold: 0.0, ratio: RATIO, knee: KNEE_DB },
            range: 0.0,
            log_freq: Smoothed::new(6_500f32.ln()),
            band: Coefficients::PASS,
            bands: [Biquad::default(); 2],
            split: Smoothed::new(1.0),
            listen: Smoothed::new(0.0),
            reduction: Follower::default(),
        };
        de_esser.update();
        de_esser.reset();
        de_esser
    }
}

impl Effect for DeEsser {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "freq" => self.log_freq.set(value.ln()),
            "threshold" => self.curve.threshold = value,
            "range" => self.range = value,
            "mode" => self.split.set(if value >= 1.0 { 0.0 } else { 1.0 }),
            "listen" => self.listen.set(if value >= 0.5 { 1.0 } else { 0.0 }),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.log_freq.snap();
        self.split.snap();
        self.listen.snap();
        self.band = Coefficients::band_pass(self.sample_rate, self.log_freq.next(self.smoothing).exp(), BAND_Q);
        self.bands = [Biquad::default(); 2];
        self.reduction.reset();
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            if !self.log_freq.is_settled() {
                self.band = Coefficients::band_pass(self.sample_rate, self.log_freq.next(self.smoothing).exp(), BAND_Q);
            }
            let band = [0, 1].map(|c| self.bands[c].process(&self.band, frame[c]));
            let reduction = self.reduction.follow(self.curve.reduction(peak_db(band[0], band[1])).min(self.range));
            let gain = db_to_gain(-reduction);
            let split = self.split.next(self.smoothing);
            let listen = self.listen.next(self.smoothing);
            for (sample, band) in frame.iter_mut().zip(band) {
                let reduced = split * (*sample - (1.0 - gain) * band) + (1.0 - split) * gain * *sample;
                *sample = reduced + listen * (band - reduced);
            }
        }
    }

    fn gain_reduction_db(&self) -> Option<f32> {
        Some(self.reduction.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, sine, tone};

    /// A voice-like mix: a low tone and a loud "s" at the band's centre.
    fn voice(s_amplitude: f32) -> Vec<f32> {
        sine(200.0, 0.3, SR as usize).iter().zip(sine(6_500.0, s_amplitude, SR as usize)).map(|(a, b)| a + b).collect()
    }

    fn render(set: &[(&str, f32)], input: &[f32]) -> (DeEsser, Vec<f32>) {
        let mut de_esser = build::<DeEsser>(set.iter().copied());
        let mut output = input.to_vec();
        de_esser.process(&mut output, None);
        (de_esser, output)
    }

    fn kept(output: &[f32], hz: f32, amplitude: f32) -> f32 {
        tone(&output[output.len() / 2..], hz) / amplitude
    }

    #[test]
    fn quiet_sibilance_passes_untouched() {
        let (de_esser, output) = render(&[], &voice(0.005));
        assert!((kept(&output, 6_500.0, 0.005) - 1.0).abs() < 0.01);
        assert!((kept(&output, 200.0, 0.3) - 1.0).abs() < 0.01);
        assert!(de_esser.gain_reduction_db().unwrap() < 0.1);
    }

    #[test]
    fn loud_sibilance_is_turned_down_by_at_most_the_range_and_split_band_spares_the_lows() {
        let (de_esser, output) = render(&[], &voice(0.5));
        assert!((kept(&output, 6_500.0, 0.5) - 0.316).abs() < 0.03, "{}", kept(&output, 6_500.0, 0.5));
        assert!((kept(&output, 200.0, 0.3) - 1.0).abs() < 0.03, "the lows stay: {}", kept(&output, 200.0, 0.3));
        assert!((de_esser.gain_reduction_db().unwrap() - 10.0).abs() < 0.5);
    }

    #[test]
    fn wideband_turns_the_whole_sound_down() {
        let (_, output) = render(&[("mode", 1.0)], &voice(0.5));
        assert!((kept(&output, 6_500.0, 0.5) - 0.316).abs() < 0.03);
        assert!((kept(&output, 200.0, 0.3) - 0.316).abs() < 0.03, "{}", kept(&output, 200.0, 0.3));
    }

    #[test]
    fn listen_plays_the_detected_band_alone() {
        let (_, output) = render(&[("listen", 1.0), ("range", 0.0)], &voice(0.5));
        assert!((kept(&output, 6_500.0, 0.5) - 1.0).abs() < 0.02);
        assert!(kept(&output, 200.0, 0.3) < 0.05, "the lows are gone: {}", kept(&output, 200.0, 0.3));
    }

    #[test]
    fn switching_mode_listen_and_frequency_does_not_click() {
        let input = voice(0.5);
        let held = |set: &[(&str, f32)]| max_step(&render(set, &input).1);
        let baseline =
            [held(&[]), held(&[("mode", 1.0)]), held(&[("listen", 1.0)]), held(&[("freq", 4_000.0)])].into_iter().fold(0.0, f32::max);
        let mut de_esser = build::<DeEsser>([]);
        let mut signal = input.clone();
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            de_esser.set_param("mode", (block % 2) as f32);
            de_esser.set_param("listen", ((block / 3) % 2) as f32);
            de_esser.set_param("freq", if block % 5 == 0 { 4_000.0 } else { 8_000.0 });
            de_esser.process(chunk, None);
        }
        assert!(max_step(&signal) < baseline * 1.5, "{} with {baseline}", max_step(&signal));
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut de_esser = build::<DeEsser>([]);
        assert_eq!(de_esser.latency_samples(), 0);
        assert_latency_at_impulse(&mut de_esser);
    }
}
