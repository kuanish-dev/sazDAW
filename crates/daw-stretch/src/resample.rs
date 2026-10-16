//! Resampling audio to another frame count (at the same rate): this is how pitch changes. Interpolation is cubic
//! (Catmull-Rom); when squeezing (pitch up) the audio first goes through a low-pass filter, otherwise high frequencies
//! would fold down as a "metallic" sound.

use std::f64::consts::PI;

/// Half the low-pass filter length in samples.
const FILTER_HALF: isize = 16;

/// Interleaved audio from `frames` into exactly `output_len` frames.
pub(crate) fn resample(frames: &[f32], channels: usize, output_len: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let input_len = frames.len() / channels;
    if input_len == 0 || output_len == 0 {
        return vec![0.0; output_len * channels];
    }
    let step = input_len as f64 / output_len as f64;
    let filtered;
    let source = if step > 1.0 {
        filtered = low_pass(frames, channels, 0.5 / step);
        &filtered
    } else {
        frames
    };
    let at = |frame: isize, channel: usize| {
        let frame = frame.clamp(0, input_len as isize - 1) as usize;
        f64::from(source[frame * channels + channel])
    };
    let mut out = Vec::with_capacity(output_len * channels);
    for i in 0..output_len {
        let position = i as f64 * step;
        let index = position.floor() as isize;
        let t = position - index as f64;
        for c in 0..channels {
            let (p0, p1, p2, p3) = (at(index - 1, c), at(index, c), at(index + 1, c), at(index + 2, c));
            let value = p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)));
            out.push(value as f32);
        }
    }
    out
}

/// A low-pass filter with cutoff `cutoff` (a fraction of the sample rate, < 0.5): Blackman window × sinc.
fn low_pass(frames: &[f32], channels: usize, cutoff: f64) -> Vec<f32> {
    let taps: Vec<f64> = (-FILTER_HALF..=FILTER_HALF)
        .map(|k| {
            let x = k as f64;
            let sinc = if k == 0 { 2.0 * cutoff } else { (2.0 * PI * cutoff * x).sin() / (PI * x) };
            let n = (x + FILTER_HALF as f64) / (2.0 * FILTER_HALF as f64);
            sinc * (0.42 - 0.5 * (2.0 * PI * n).cos() + 0.08 * (4.0 * PI * n).cos())
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    let input_len = (frames.len() / channels) as isize;
    let mut out = vec![0.0f32; frames.len()];
    for frame in 0..input_len {
        for c in 0..channels {
            let mut acc = 0.0;
            for (j, tap) in taps.iter().enumerate() {
                let source = frame + j as isize - FILTER_HALF;
                if (0..input_len).contains(&source) {
                    acc += tap * f64::from(frames[source as usize * channels + c]);
                }
            }
            out[frame as usize * channels + c] = (acc / sum) as f32;
        }
    }
    out
}
