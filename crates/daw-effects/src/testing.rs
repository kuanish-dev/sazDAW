//! Helpers for the effect tests.

pub const SR: u32 = 48_000;

/// A stereo sine of the given frequency and amplitude.
pub fn sine(hz: f32, amplitude: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * hz * i as f32 / SR as f32).sin() * amplitude;
            [v, v]
        })
        .collect()
}

pub fn rms(buffer: &[f32]) -> f32 {
    (buffer.iter().map(|v| v * v).sum::<f32>() / buffer.len().max(1) as f32).sqrt()
}

pub fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |a, b| a.max(b.abs()))
}

/// A stereo impulse: one in the first frame, silence after.
pub fn impulse(frames: usize) -> Vec<f32> {
    let mut buffer = vec![0.0; frames * 2];
    buffer[0] = 1.0;
    buffer[1] = 1.0;
    buffer
}

/// The largest step between neighbouring frames of one channel: a click or zipper noise shows up as a spike.
pub fn max_step(buffer: &[f32]) -> f32 {
    buffer.windows(3).map(|w| (w[2] - w[0]).abs()).fold(0.0, f32::max)
}

/// Amplitude of the `hz` component in the left channel (one DFT bin). Exact when the buffer holds a whole number of periods.
pub fn tone(buffer: &[f32], hz: f32) -> f32 {
    let w = std::f64::consts::TAU * f64::from(hz) / f64::from(SR);
    let (re, im) = buffer.iter().step_by(2).enumerate().fold((0.0, 0.0), |(re, im), (i, &v)| {
        let phase = w * i as f64;
        (re + f64::from(v) * phase.cos(), im - f64::from(v) * phase.sin())
    });
    let frames = (buffer.len() / 2).max(1) as f64;
    ((re * re + im * im).sqrt() * 2.0 / frames) as f32
}
