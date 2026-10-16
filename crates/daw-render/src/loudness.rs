//! ITU-R BS.1770-4 / EBU R128 measuring, pure math shared by every meter of the app: the export's loudness and true
//! peak (`export/loudness.rs`), the project level analysis (`analysis/meter.rs`) and the live master meter (`live/`).
//! One place, so a live reading, an analysis and an exported file's report are the same numbers.
//!
//! `Loudness` K-weights both channels and keeps the energy of every 100 ms step since its start or its `restart`: the
//! momentary (400 ms) and short-term (3 s) loudness are the newest steps, the integrated loudness and the loudness range
//! (EBU Tech 3342) gate over all of them. `TruePeak` oversamples to find the peak between samples.

use daw_contracts::SILENCE_DB;

/// 100 ms steps in a 400 ms block: the momentary loudness and the integrated loudness's gating blocks.
const STEPS_PER_BLOCK: usize = 4;
/// 100 ms steps in the 3 s short-term window (also the loudness range's windows).
const STEPS_PER_SHORT_TERM: usize = 30;
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = 10.0;
/// The loudness range gates 20 LU below the mean of its windows, wider than the integrated loudness (EBU Tech 3342).
const RANGE_RELATIVE_GATE_LU: f64 = 20.0;
/// The loudness range is the distance between these percentiles of the gated short-term loudness: the quietest tenth
/// (fade-outs, breaks) and the loudest twentieth (a single hit) do not stretch it.
const RANGE_LOW_PERCENTILE: f64 = 0.10;
const RANGE_HIGH_PERCENTILE: f64 = 0.95;

/// A second-order filter section (transposed direct form II), `a0 = 1`.
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    const fn new(b: [f64; 3], a: [f64; 2]) -> Self {
        Self { b, a, z: [0.0; 2] }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The BS.1770 K-filter for `rate`: a +4 dB shelf above ~1.5 kHz (head effect) and a cut below ~38 Hz (RLB).
/// The standard gives coefficients only for 48 kHz; for any rate they come from the analog prototypes through the
/// bilinear transform (as in libebur128), matching the table to 1e-8 at 48 kHz. Every meter measures at its own rate.
fn k_filter(rate: u32) -> [Biquad; 2] {
    let rate = f64::from(rate);
    let (f0, gain_db, q) = (1_681.974_450_955_533, 3.999_843_853_973_347, 0.707_175_236_955_419_6);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad::new(
        [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = Biquad::new([1.0, -2.0, 1.0], [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0]);
    [shelf, high_pass]
}

/// Loudness in LUFS from the mean power of K-weighted audio (summed over channels; left and right weigh 1).
fn lufs(power: f64) -> f64 {
    -0.691 + 10.0 * power.log10()
}

/// Whether a block's power passes the absolute gate (−70 LUFS).
fn above_gate(power: f64) -> bool {
    power > 0.0 && lufs(power) > ABSOLUTE_GATE_LUFS
}

fn mean(powers: &[f64]) -> f64 {
    powers.iter().sum::<f64>() / powers.len() as f64
}

pub(crate) fn round_tenth(db: f64) -> f32 {
    ((db * 10.0).round() / 10.0) as f32
}

/// BS.1770-4 loudness: a K-filter per channel and the power of 100 ms steps. Blocks of 400 ms every 100 ms give the
/// integrated loudness (an absolute gate at −70 LUFS, then a relative gate 10 LU below the loudness of the blocks that
/// passed); the newest steps give the momentary and short-term loudness, 3 s windows the loudness range.
pub(crate) struct Loudness {
    /// Left and right channel filters: the shelf, then the cut.
    filters: [[Biquad; 2]; 2],
    /// Frames in a 100 ms step.
    step_length: usize,
    /// Energy of the finished 100 ms steps (sum of squares of both channels): those since the start or the restart, and
    /// before a restart the last short-term window, so the momentary and short-term loudness run on across it.
    steps: Vec<f64>,
    /// Where the integration starts in `steps`.
    from: usize,
    step_energy: f64,
    step_frames: usize,
}

impl Loudness {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let filters = k_filter(sample_rate);
        Self { filters: [filters; 2], step_length: sample_rate as usize / 10, steps: Vec::new(), from: 0, step_energy: 0.0, step_frames: 0 }
    }

    /// Adds a frame; `true` when it finished a 100 ms step (the readings change only then).
    pub(crate) fn push(&mut self, left: f32, right: f32) -> bool {
        for (channel, sample) in [left, right].into_iter().enumerate() {
            let [shelf, high_pass] = &mut self.filters[channel];
            let weighted = high_pass.process(shelf.process(f64::from(sample)));
            self.step_energy += weighted * weighted;
        }
        self.step_frames += 1;
        if self.step_frames < self.step_length {
            return false;
        }
        self.steps.push(self.step_energy);
        self.step_energy = 0.0;
        self.step_frames = 0;
        true
    }

    /// Starts the integration over (to within the step under way): the integrated loudness and the range forget
    /// everything before; the momentary and short-term loudness run on.
    pub(crate) fn restart(&mut self) {
        let older = self.steps.len().saturating_sub(STEPS_PER_SHORT_TERM);
        self.steps.drain(..older);
        self.from = self.steps.len();
    }

    /// Mean power of the newest `count` steps; `None` while fewer were measured.
    fn newest_power(&self, count: usize) -> Option<f64> {
        let start = self.steps.len().checked_sub(count)?;
        Some(self.steps[start..].iter().sum::<f64>() / (count * self.step_length) as f64)
    }

    /// Mean powers of every window of `count` consecutive steps since the start of the integration (one per step).
    fn window_powers(&self, count: usize) -> impl Iterator<Item = f64> + '_ {
        let frames = (count * self.step_length) as f64;
        self.steps[self.from..].windows(count).map(move |window| window.iter().sum::<f64>() / frames)
    }

    /// The last 400 ms; `None` below the absolute gate or before 400 ms were measured.
    pub(crate) fn momentary(&self) -> Option<f32> {
        self.newest_power(STEPS_PER_BLOCK).filter(|&p| above_gate(p)).map(|p| round_tenth(lufs(p)))
    }

    /// The last 3 s; `None` below the absolute gate or before 3 s were measured.
    pub(crate) fn short_term(&self) -> Option<f32> {
        self.newest_power(STEPS_PER_SHORT_TERM).filter(|&p| above_gate(p)).map(|p| round_tenth(lufs(p)))
    }

    /// `None`: no block passed the absolute gate (silence) or the audio is shorter than a 400 ms block.
    pub(crate) fn integrated(&self) -> Option<f32> {
        let loud: Vec<f64> = self.window_powers(STEPS_PER_BLOCK).filter(|&p| above_gate(p)).collect();
        if loud.is_empty() {
            return None;
        }
        let relative = lufs(mean(&loud)) - RELATIVE_GATE_LU;
        let gated: Vec<f64> = loud.into_iter().filter(|&p| lufs(p) > relative).collect();
        Some(round_tenth(lufs(mean(&gated))))
    }

    /// Loudness range (EBU Tech 3342) in LU: the short-term loudness of every 3 s window since the start of the
    /// integration, gated at −70 LUFS and 20 LU below their mean, from its 10th to its 95th percentile. `None`: no window
    /// passed the gate.
    pub(crate) fn range(&self) -> Option<f32> {
        let loud: Vec<f64> = self.window_powers(STEPS_PER_SHORT_TERM).filter(|&p| above_gate(p)).collect();
        if loud.is_empty() {
            return None;
        }
        let relative = lufs(mean(&loud)) - RANGE_RELATIVE_GATE_LU;
        // Not empty: the loudest window is never below the mean.
        let mut levels: Vec<f64> = loud.into_iter().map(lufs).filter(|&level| level > relative).collect();
        levels.sort_by(f64::total_cmp);
        let percentile = |p: f64| levels[((levels.len() - 1) as f64 * p).round() as usize];
        Some(round_tenth(percentile(RANGE_HIGH_PERCENTILE) - percentile(RANGE_LOW_PERCENTILE)))
    }
}

/// How many times `TruePeak` oversamples and how many neighbouring samples the interpolation takes.
const OVERSAMPLING: usize = 4;
const TAPS: usize = 12;

/// True peak (BS.1770-4, annex 2): the audio is oversampled 4 times (Hann-windowed sinc over 12 neighbouring samples)
/// and the peak is searched between samples too, where a signal rebuilt by a DAC or a codec rises.
/// The sample peak understates it: a sine at a quarter of the sample rate shifted by 45° reads 3 dB quieter.
pub(crate) struct TruePeak {
    /// The last `TAPS` samples of each channel, newest first.
    history: [[f32; TAPS]; 2],
    /// Interpolation weights for each of the `OVERSAMPLING` points between samples.
    phases: [[f32; TAPS]; OVERSAMPLING],
    peak: f32,
}

impl TruePeak {
    pub(crate) fn new() -> Self {
        let half = (TAPS / 2) as f64;
        let mut phases = [[0.0; TAPS]; OVERSAMPLING];
        for (p, phase) in phases.iter_mut().enumerate() {
            // A point between samples `half` and `half - 1` back; the distance to sample `k` back is `k - half + fraction`.
            let fraction = p as f64 / OVERSAMPLING as f64;
            let weights: Vec<f64> = (0..TAPS)
                .map(|k| {
                    let x = k as f64 - half + fraction;
                    let sinc = if x == 0.0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                    let window = f64::midpoint(1.0, (std::f64::consts::PI * x / (half + 0.5)).cos());
                    sinc * window
                })
                .collect();
            // The weights sum to one, so a constant signal stays the same at every point.
            let sum: f64 = weights.iter().sum();
            for (tap, weight) in phase.iter_mut().zip(weights) {
                *tap = (weight / sum) as f32;
            }
        }
        Self { history: [[0.0; TAPS]; 2], phases, peak: 0.0 }
    }

    pub(crate) fn push(&mut self, left: f32, right: f32) {
        for (history, sample) in self.history.iter_mut().zip([left, right]) {
            history.copy_within(0..TAPS - 1, 1);
            history[0] = sample;
            for phase in &self.phases {
                let value: f32 = phase.iter().zip(history.iter()).map(|(w, x)| w * x).sum();
                self.peak = self.peak.max(value.abs());
            }
        }
    }

    /// True peak in dBTP; `None` when quieter than [`SILENCE_DB`].
    pub(crate) fn peak_db(&self) -> Option<f64> {
        decibels(self.peak)
    }

    /// The peak (linear) since the previous `take` or the start; the next one starts from silence (a live meter keeps one
    /// per 100 ms step).
    pub(crate) fn take(&mut self) -> f32 {
        std::mem::take(&mut self.peak)
    }
}

/// A linear peak in dB; `None` when quieter than [`SILENCE_DB`].
pub(crate) fn decibels(peak: f32) -> Option<f64> {
    let db = 20.0 * f64::from(peak).log10();
    (db >= SILENCE_DB.into()).then_some(db)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Test signals are made at this rate unless a test says otherwise.
    pub(crate) const RATE: u32 = 48_000;

    /// A sine of `hz` with peak `peak_db` dBFS in the left and/or right channel, interleaved, at [`RATE`].
    pub(crate) fn sine(hz: f32, peak_db: f32, seconds: f32, left: bool, right: bool) -> Vec<f32> {
        let amplitude = 10f32.powf(peak_db / 20.0);
        let frames = (RATE as f32 * seconds) as usize;
        (0..frames)
            .flat_map(|i| {
                let v = amplitude * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin();
                [if left { v } else { 0.0 }, if right { v } else { 0.0 }]
            })
            .collect()
    }

    fn measured(interleaved: &[f32]) -> Loudness {
        let mut meter = Loudness::new(RATE);
        for frame in interleaved.chunks_exact(2) {
            meter.push(frame[0], frame[1]);
        }
        meter
    }

    fn loudness(interleaved: &[f32]) -> Option<f32> {
        measured(interleaved).integrated()
    }

    fn close(actual: Option<f32>, expected: f32) -> bool {
        actual.is_some_and(|a| (a - expected).abs() <= 0.1)
    }

    /// EBU Tech 3341, test 1: a 1 kHz sine at -23 dBFS in both channels is -23.0 LUFS.
    #[test]
    fn ebu_reference_sine_reads_minus_23_lufs() {
        let measured = loudness(&sine(1000.0, -23.0, 20.0, true, true));
        assert!(close(measured, -23.0), "{measured:?}");
    }

    /// A 1 kHz sine at -20 dBFS: -20 LUFS in both channels, 3 dB quieter (-23 LUFS) in one channel (BS.1770: a 0 dBFS
    /// sine in one channel is -3.01 LUFS).
    #[test]
    fn minus_20_dbfs_sine_reads_minus_20_in_stereo_and_minus_23_in_one_channel() {
        assert!(close(loudness(&sine(1000.0, -20.0, 10.0, true, true)), -20.0));
        let one = loudness(&sine(1000.0, -20.0, 10.0, true, false));
        assert!(close(one, -23.0), "{one:?}");
        assert_eq!(loudness(&sine(1000.0, -20.0, 10.0, false, true)), one, "channels are equal");
    }

    /// Gating: silence and short audio have no loudness; quiet pauses do not pull the loudness down.
    #[test]
    fn gating_ignores_silence_and_quiet_parts() {
        assert_eq!(loudness(&vec![0.0; 48_000 * 2 * 5]), None);
        assert_eq!(loudness(&sine(1000.0, -20.0, 0.3, true, true)), None, "shorter than a 400 ms block");
        let mut song = sine(1000.0, -20.0, 10.0, true, true);
        song.extend(vec![0.0; 48_000 * 2 * 10]);
        song.extend(sine(1000.0, -60.0, 10.0, true, true));
        // Blocks at the sound/silence boundary pass the gate and lower the result slightly: -20.1.
        let measured = loudness(&song).unwrap();
        assert!((measured + 20.0).abs() <= 0.2, "{measured}");
    }

    /// The K-filter cuts the very low end: 20 Hz at the same amplitude is clearly quieter than 1 kHz.
    #[test]
    fn k_weighting_discounts_deep_bass() {
        let bass = loudness(&sine(20.0, -20.0, 10.0, true, true)).unwrap();
        assert!(bass < -30.0, "{bass}");
    }

    /// K-filter coefficients computed for 48 kHz match the BS.1770 table.
    #[test]
    fn k_filter_formula_matches_the_48k_table() {
        let [shelf, high_pass] = k_filter(48_000);
        let table_shelf =
            ([1.535_124_859_586_97, -2.691_696_189_406_38, 1.198_392_810_852_85], [-1.690_659_293_182_41, 0.732_480_774_215_85]);
        let table_high_pass = ([1.0, -2.0, 1.0], [-1.990_047_454_833_98, 0.990_072_250_366_21]);
        for (filter, (b, a)) in [(shelf, table_shelf), (high_pass, table_high_pass)] {
            for (x, y) in filter.b.iter().chain(&filter.a).zip(b.iter().chain(&a)) {
                assert!((x - y).abs() < 1e-7, "{x} ≠ {y}");
            }
        }
    }

    /// Loudness does not depend on the sample rate: the -23 dBFS reference sine at 44.1 and 96 kHz is -23 LUFS too.
    #[test]
    fn loudness_reads_the_same_at_other_sample_rates() {
        for rate in [44_100u32, 96_000] {
            let mut meter = Loudness::new(rate);
            let amplitude = 10f32.powf(-23.0 / 20.0);
            for i in 0..rate as usize * 10 {
                let v = amplitude * (std::f32::consts::TAU * 1000.0 * i as f32 / rate as f32).sin();
                meter.push(v, v);
            }
            assert!(close(meter.integrated(), -23.0), "{rate}: {:?}", meter.integrated());
        }
    }

    /// The momentary loudness is the last 400 ms and the short-term the last 3 s: after 10 s at -20 and 1 s at -30, the
    /// momentary reads -30 while the short-term still holds two seconds of the louder part.
    #[test]
    fn momentary_and_short_term_are_the_newest_windows() {
        let mut audio = sine(1000.0, -20.0, 10.0, true, true);
        audio.extend(sine(1000.0, -30.0, 1.0, true, true));
        let meter = measured(&audio);
        assert!(close(meter.momentary(), -30.0), "{:?}", meter.momentary());
        // Mean power of 2 s at -20 and 1 s at -30: 10·log10((2 + 0.1) / 3) ≈ -1.5 dB below -20.
        assert!(close(meter.short_term(), -21.5), "{:?}", meter.short_term());

        let short = measured(&sine(1000.0, -20.0, 2.0, true, true));
        assert!(close(short.momentary(), -20.0));
        assert_eq!(short.short_term(), None, "less than 3 s measured");
        assert_eq!(measured(&vec![0.0; 48_000 * 2 * 4]).momentary(), None, "silence is below the gate");
    }

    /// EBU Tech 3342, test 1: 20 s of a 1 kHz sine at -20 dBFS, then 20 s at -30 dBFS, has a loudness range of 10 LU
    /// (±1); a steady tone has none.
    #[test]
    fn loudness_range_follows_ebu_tech_3342() {
        let mut audio = sine(1000.0, -20.0, 20.0, true, true);
        audio.extend(sine(1000.0, -30.0, 20.0, true, true));
        let range = measured(&audio).range().unwrap();
        assert!((range - 10.0).abs() <= 1.0, "{range}");
        assert_eq!(measured(&sine(1000.0, -20.0, 10.0, true, true)).range(), Some(0.0));
        assert_eq!(measured(&sine(1000.0, -20.0, 2.0, true, true)).range(), None, "no 3 s window yet");
    }

    /// A restart forgets the integration but not the newest windows.
    #[test]
    fn a_restart_starts_the_integration_over() {
        let mut meter = measured(&sine(1000.0, -20.0, 10.0, true, true));
        meter.restart();
        assert_eq!((meter.integrated(), meter.range()), (None, None));
        assert!(close(meter.momentary(), -20.0) && close(meter.short_term(), -20.0), "the windows run on");
        for frame in sine(1000.0, -30.0, 5.0, true, true).chunks_exact(2) {
            meter.push(frame[0], frame[1]);
        }
        assert!(close(meter.integrated(), -30.0), "only what came after: {:?}", meter.integrated());
    }

    /// A sine at a quarter of the rate shifted by 45°: samples are ±0.707 (-3 dB), the true peak is 0 dBTP.
    /// A low sine: the true peak equals the sample peak.
    #[test]
    fn true_peak_finds_the_peak_between_samples() {
        let mut meter = TruePeak::new();
        for i in 0..4_800 {
            let v = (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin();
            meter.push(v, 0.0);
        }
        let peak = meter.peak_db().unwrap();
        assert!(peak.abs() < 0.2, "{peak}");

        let mut low = TruePeak::new();
        for frame in sine(100.0, -6.0, 1.0, true, true).chunks_exact(2) {
            low.push(frame[0], frame[1]);
        }
        assert!((low.peak_db().unwrap() + 6.0).abs() < 0.05);
        assert_eq!(TruePeak::new().peak_db(), None, "silence");
    }

    #[test]
    fn taking_the_true_peak_starts_the_next_from_silence() {
        let mut meter = TruePeak::new();
        for frame in sine(100.0, -6.0, 0.1, true, true).chunks_exact(2) {
            meter.push(frame[0], frame[1]);
        }
        assert!((decibels(meter.take()).unwrap() + 6.0).abs() < 0.05);
        assert_eq!(meter.peak_db(), None);
    }
}
