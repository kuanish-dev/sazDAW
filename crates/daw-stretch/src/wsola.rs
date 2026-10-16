//! WSOLA (waveform similarity overlap-add): stretching audio in time without changing pitch.
//!
//! The output is built from ~40 ms Hann windows at half-window steps. Each next window is taken from the input near its
//! "nominal" place (input step = output step / stretch), but shifted within a quarter window by the offset at which it
//! best continues the previous window. This way phases line up and there is no "bubbling" echo. The offset is searched
//! coarsely on a decimated mono signal, then refined on the full one; all channels take the same offset (the stereo
//! image stays intact).

use std::f64::consts::PI;

/// Window length in seconds: about the period of the lowest audible tone and shorter than a drum hit.
const WINDOW_SECONDS: f64 = 0.04;
/// Decimation factor of the signal for the coarse offset search.
const DECIMATION: usize = 4;

/// Stretches interleaved audio by `stretch`; the output has exactly `round(frames × stretch)` frames.
pub(crate) fn stretch_time(frames: &[f32], channels: usize, sample_rate: u32, stretch: f64) -> Vec<f32> {
    let output_len = (frames.len() / channels.max(1)) as f64 * stretch;
    stretch_mapped(frames, channels, sample_rate, output_len.round() as usize, |out| out / stretch)
}

/// Stretching with a factor that changes along the audio: `source_at(output frame)` is the input frame that should
/// sound at that output frame (increasing). The output has exactly `output_len` frames. A constant stretch is the
/// special case `out / stretch`, so both paths give the same sound.
pub(crate) fn stretch_mapped(
    frames: &[f32],
    channels: usize,
    sample_rate: u32,
    output_len: usize,
    source_at: impl Fn(f64) -> f64,
) -> Vec<f32> {
    let channels = channels.max(1);
    let input_len = frames.len() / channels;
    let window_len = (((f64::from(sample_rate) * WINDOW_SECONDS) as usize).max(64) / (2 * DECIMATION)) * (2 * DECIMATION);
    let hop = window_len / 2;
    let tolerance = window_len / 4;
    let window: Vec<f64> = (0..window_len).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / window_len as f64).cos()).collect();
    let guide = Guide::new(frames, channels);

    let mut out = vec![0.0f64; (output_len + window_len) * channels];
    let mut weight = vec![0.0f64; output_len + window_len];
    let mut previous: Option<isize> = None;
    let mut out_pos = 0;
    while out_pos < output_len {
        let nominal = source_at(out_pos as f64).round() as isize;
        let pos = match previous {
            None => nominal,
            Some(prev) => guide.best_match(nominal, prev + hop as isize, tolerance, window_len),
        };
        for i in 0..window_len {
            let w = window[i];
            weight[out_pos + i] += w;
            let source = pos + i as isize;
            if source < 0 || source as usize >= input_len {
                continue;
            }
            let base = source as usize * channels;
            for c in 0..channels {
                out[(out_pos + i) * channels + c] += f64::from(frames[base + c]) * w;
            }
        }
        previous = Some(pos);
        out_pos += hop;
    }
    // Dividing by the window sum: Hann windows at half-window steps sum to 1 except at the very start, where the first
    // window has not reached full strength; there the division restores the original sound without a dip.
    (0..output_len * channels)
        .map(|i| {
            let w = weight[i / channels];
            if w > 1e-9 { (out[i] / w) as f32 } else { 0.0 }
        })
        .collect()
}

/// Mono signal for the offset search: full and decimated.
struct Guide {
    full: Vec<f32>,
    coarse: Vec<f32>,
}

impl Guide {
    fn new(frames: &[f32], channels: usize) -> Self {
        let full: Vec<f32> = frames.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect();
        let coarse = full.chunks(DECIMATION).map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32).collect();
        Self { full, coarse }
    }

    /// The window start near `nominal` (±`tolerance`) that best resembles the natural continuation of `target`.
    fn best_match(&self, nominal: isize, target: isize, tolerance: usize, window_len: usize) -> isize {
        let d = DECIMATION as isize;
        let coarse_tolerance = (tolerance / DECIMATION) as isize;
        let coarse_len = window_len / DECIMATION;
        let coarse_target = target.div_euclid(d);
        let coarse_nominal = nominal.div_euclid(d);
        // On a tie (silence) the nominal place stays.
        let mut best = (similarity(&self.coarse, coarse_nominal, coarse_target, coarse_len), coarse_nominal * d);
        for shift in -coarse_tolerance..=coarse_tolerance {
            let candidate = coarse_nominal + shift;
            let score = similarity(&self.coarse, candidate, coarse_target, coarse_len);
            if score > best.0 {
                best = (score, candidate * d);
            }
        }
        // Refinement on the full signal within one decimation step (over the first half of the window, the part that
        // overlaps the audio already built).
        let rough = best.1;
        let mut fine = (similarity(&self.full, rough, target, window_len / 2), rough);
        for candidate in rough - d..=rough + d {
            if (candidate - nominal).unsigned_abs() > tolerance {
                continue;
            }
            let score = similarity(&self.full, candidate, target, window_len / 2);
            if score > fine.0 {
                fine = (score, candidate);
            }
        }
        fine.1
    }
}

/// Normalized cross-correlation of `[a, a+len)` and `[b, b+len)`; silence past the signal edges.
fn similarity(signal: &[f32], a: isize, b: isize, len: usize) -> f64 {
    let at = |i: isize| if i < 0 { 0.0 } else { f64::from(signal.get(i as usize).copied().unwrap_or(0.0)) };
    let (mut cross, mut energy) = (0.0, 0.0);
    for i in 0..len as isize {
        let x = at(a + i);
        cross += x * at(b + i);
        energy += x * x;
    }
    if energy > 1e-12 { cross / energy.sqrt() } else { 0.0 }
}
