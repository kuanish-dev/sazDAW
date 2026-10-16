//! The preview voice: plays a region of a sample bypassing the channels and the transport.

use std::sync::Arc;

use daw_contracts::SampleData;

const GAIN: f32 = 0.9;

#[derive(Default)]
pub struct PreviewVoice {
    data: Option<Arc<SampleData>>,
    /// The position in file frames.
    position: f64,
    from: usize,
    to: usize,
    looping: bool,
}

impl PreviewVoice {
    /// Starts the region `[from, to)`; returns the previous sample (it must be freed outside the audio thread).
    pub fn start(&mut self, data: Arc<SampleData>, from: usize, to: usize, looping: bool) -> Option<Arc<SampleData>> {
        let frames = data.frame_count();
        let to = to.min(frames);
        let from = from.min(to.saturating_sub(1));
        self.position = from as f64;
        self.from = from;
        self.to = to;
        self.looping = looping;
        self.data.replace(data)
    }

    pub fn stop(&mut self) -> Option<Arc<SampleData>> {
        self.data.take()
    }

    pub fn is_playing(&self) -> bool {
        self.data.is_some()
    }

    /// The position for the interface.
    pub fn position(&self) -> Option<usize> {
        self.data.as_ref().map(|_| self.position as usize)
    }

    /// Adds the sound to the stereo buffer `interleaved`. Returns the sample if the voice has finished.
    pub fn render(&mut self, interleaved: &mut [f32], device_rate: u32) -> Option<Arc<SampleData>> {
        let data = self.data.clone()?;
        let ratio = f64::from(data.sample_rate) / f64::from(device_rate);
        let (from, to) = (self.from as f64, self.to as f64);
        let mut finished = false;
        for frame in interleaved.chunks_exact_mut(2) {
            if self.position + 1.0 >= to {
                if self.looping && to - from >= 2.0 {
                    self.position -= to - 1.0 - from;
                } else {
                    finished = true;
                    break;
                }
            }
            let (left, right) = data.stereo_lerp(self.position);
            frame[0] += left * GAIN;
            frame[1] += right * GAIN;
            self.position += ratio;
        }
        drop(data);
        if finished { self.data.take() } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(frames: usize) -> Arc<SampleData> {
        Arc::new(SampleData { sample_rate: 48_000, channels: 1, frames: (0..frames).map(|i| i as f32 / frames as f32).collect() })
    }

    #[test]
    fn plays_the_region_and_reports_the_end() {
        let mut voice = PreviewVoice::default();
        assert!(voice.start(ramp(1000), 200, 300, false).is_none());
        let mut out = vec![0.0; 400 * 2];
        let ended = voice.render(&mut out, 48_000);
        assert!(ended.is_some(), "the region has ended");
        assert!(!voice.is_playing());
        assert!((out[0] - 0.2 * GAIN).abs() < 0.01, "the region start: {}", out[0]);
        assert!(out[99 * 2] < 0.3 * GAIN + 0.01);
        assert!(out[150 * 2..].iter().all(|v| *v == 0.0), "silence after the region");
    }

    #[test]
    fn loops_inside_the_region_until_stopped() {
        let mut voice = PreviewVoice::default();
        voice.start(ramp(1000), 200, 300, true);
        let mut out = vec![0.0; 5000 * 2];
        assert!(voice.render(&mut out, 48_000).is_none());
        assert!(voice.is_playing());
        assert!(out.iter().step_by(2).all(|v| *v >= 0.2 * GAIN - 0.01 && *v <= 0.3 * GAIN + 0.01));
        assert!(voice.position().unwrap() >= 200 && voice.position().unwrap() < 300);
        assert!(voice.stop().is_some());
        assert_eq!(voice.position(), None);
    }

    #[test]
    fn resamples_to_the_device_rate() {
        let mut voice = PreviewVoice::default();
        voice.start(ramp(1000), 0, 1000, false);
        let mut out = vec![0.0; 500 * 2];
        voice.render(&mut out, 96_000); // the device is twice as fast, so the file goes twice as slow
        assert!((out[499 * 2] - 0.25 * GAIN).abs() < 0.02, "{}", out[499 * 2]);
    }

    #[test]
    fn restarting_returns_the_previous_sample_for_release() {
        let mut voice = PreviewVoice::default();
        voice.start(ramp(100), 0, 100, false);
        assert!(voice.start(ramp(200), 0, 100, false).is_some());
    }
}
