//! Input capture: peaks for the meter and copying frames into a preallocated ring queue.
//!
//! The queue is SPSC: the producer (`InputFeed`) lives in the input audio callback and neither allocates nor waits
//! for locks; the consumer (`CaptureSource`) sits in `Shared` behind a `Mutex` and is read by `AudioCapture::drain`
//! (not the audio thread). Copying is switched on by a flag while recording; otherwise the callback only counts peaks.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use daw_contracts::{InputLevel, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::live::LiveTap;
use crate::peaks::InputPeaks;

/// How many seconds of sound fit in the queue before `drain` takes them.
const QUEUE_SECONDS: usize = 5;

/// What the input callback shares with the rest of the crate: peaks, the recording flag, the loss counter and the monitoring input.
#[derive(Clone)]
pub(crate) struct InputTap {
    peaks: Arc<InputPeaks>,
    /// On while recording; otherwise frames are not copied.
    enabled: Arc<AtomicBool>,
    /// Frames that did not fit in the queue since recording started.
    dropped: Arc<AtomicU64>,
    live: LiveTap,
}

impl InputTap {
    pub fn new(live: LiveTap) -> Self {
        Self { peaks: Arc::default(), enabled: Arc::default(), dropped: Arc::default(), live }
    }

    pub fn live(&self) -> &LiveTap {
        &self.live
    }

    /// Recording started: frames are copied to the queue, the loss counter starts from zero.
    pub fn begin_capture(&self) {
        self.dropped.store(0, Ordering::Relaxed);
        self.enabled.store(true, Ordering::Relaxed);
    }

    pub fn end_capture(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }

    /// Frames lost since recording started.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn level(&self) -> InputLevel {
        self.peaks.read()
    }

    /// The input closed: the meter drops to zero at once.
    pub fn reset_level(&self) {
        self.peaks.reset();
    }
}

/// The audio callback side. No method allocates or blocks.
pub(crate) struct InputFeed {
    tap: InputTap,
    producer: Producer<f32>,
    /// Channels of the device.
    in_channels: usize,
    /// Channels in the queue: the first two.
    out_channels: usize,
}

/// The reading side of the queue (not the audio thread).
pub(crate) struct CaptureSource {
    consumer: Consumer<f32>,
    format: StreamConfig,
}

/// A queue for the stream format: the device has `device_channels` channels, the first two go to the queue.
pub(crate) fn input_queue(tap: InputTap, sample_rate: u32, device_channels: u16) -> (InputFeed, CaptureSource) {
    let in_channels = usize::from(device_channels.max(1));
    let out_channels = in_channels.min(2);
    let capacity = sample_rate as usize * out_channels * QUEUE_SECONDS;
    let (producer, consumer) = RingBuffer::new(capacity);
    let format = StreamConfig { sample_rate, channels: out_channels as u16 };
    (InputFeed { tap, producer, in_channels, out_channels }, CaptureSource { consumer, format })
}

impl InputFeed {
    /// Body of the input callback: peaks and (while recording) a copy of the frames. Whole frames only: what fits is
    /// written, the rest counts as lost, and the queue never holds half a frame.
    pub fn feed(&mut self, interleaved: &[f32]) {
        self.tap.peaks.feed(interleaved, self.in_channels);
        self.tap.live.feed(interleaved, self.in_channels);
        if !self.tap.enabled.load(Ordering::Relaxed) {
            return;
        }
        let frames = interleaved.len() / self.in_channels;
        let fit = (self.producer.slots() / self.out_channels).min(frames);
        if fit < frames {
            self.tap.dropped.fetch_add((frames - fit) as u64, Ordering::Relaxed);
        }
        if fit == 0 {
            return;
        }
        let Ok(chunk) = self.producer.write_chunk_uninit(fit * self.out_channels) else { return };
        let (in_channels, out_channels) = (self.in_channels, self.out_channels);
        chunk.fill_from_iter(interleaved.chunks_exact(in_channels).take(fit).flat_map(|frame| frame[..out_channels].iter().copied()));
    }
}

impl CaptureSource {
    pub fn format(&self) -> StreamConfig {
        self.format
    }

    /// Throws away what was queued: frames of the previous take must not reach the new one.
    pub fn clear(&mut self) {
        let samples = self.consumer.slots();
        if let Ok(chunk) = self.consumer.read_chunk(samples) {
            chunk.commit_all();
        }
    }

    /// Moves everything queued to the end of `out`; returns the number of frames.
    pub fn drain(&mut self, out: &mut Vec<f32>) -> usize {
        let samples = self.consumer.slots();
        let Ok(chunk) = self.consumer.read_chunk(samples) else { return 0 };
        let (first, second) = chunk.as_slices();
        out.extend_from_slice(first);
        out.extend_from_slice(second);
        chunk.commit_all();
        samples / usize::from(self.format.channels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::live_queue;

    fn tap(enabled: bool) -> InputTap {
        let tap = InputTap::new(live_queue().0);
        if enabled {
            tap.begin_capture();
        }
        tap
    }

    #[test]
    fn stereo_frames_are_copied_interleaved_and_drained_once() {
        let (mut feed, mut source) = input_queue(tap(true), 48_000, 2);
        feed.feed(&[0.1, 0.2, 0.3, 0.4]);
        feed.feed(&[0.5, 0.6]);
        let mut out = Vec::new();
        assert_eq!(source.drain(&mut out), 3);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        assert_eq!(source.drain(&mut out), 0, "the second drain is empty");
        assert_eq!(source.format, StreamConfig { sample_rate: 48_000, channels: 2 });
    }

    #[test]
    fn mono_stays_mono_and_extra_channels_are_cut_to_two() {
        let (mut feed, mut source) = input_queue(tap(true), 44_100, 1);
        feed.feed(&[0.1, 0.2]);
        let mut out = Vec::new();
        assert_eq!(source.drain(&mut out), 2);
        assert_eq!(out, [0.1, 0.2]);

        let (mut feed, mut source) = input_queue(tap(true), 44_100, 4);
        feed.feed(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let mut out = Vec::new();
        assert_eq!(source.drain(&mut out), 2);
        assert_eq!(out, [1.0, 2.0, 5.0, 6.0]);
        assert_eq!(source.format.channels, 2);
    }

    #[test]
    fn nothing_is_copied_while_capture_is_off_but_peaks_still_work() {
        let tap = tap(false);
        let (mut feed, mut source) = input_queue(tap.clone(), 48_000, 2);
        feed.feed(&[0.9, 0.1]);
        assert_eq!(source.drain(&mut Vec::new()), 0);
        assert_eq!(tap.level().left, 0.9);
        assert_eq!(tap.dropped(), 0);
    }

    #[test]
    fn overflow_counts_lost_whole_frames_and_keeps_the_queue_consistent() {
        let tap = tap(true);
        let (mut feed, mut source) = input_queue(tap.clone(), 2, 2);
        let block: Vec<f32> = (0..8).flat_map(|i| [i as f32, -(i as f32)]).collect();
        feed.feed(&block);
        feed.feed(&block);
        assert_eq!(tap.dropped(), 6);
        let mut out = Vec::new();
        assert_eq!(source.drain(&mut out), 10);
        assert_eq!(out.len(), 20);
        assert_eq!(&out[16..], [0.0, -0.0, 1.0, -1.0], "the first frames of the second block were taken");
        feed.feed(&block[..2]);
        assert_eq!(source.drain(&mut Vec::new()), 1, "drain freed the space");
    }

    #[test]
    fn clear_throws_away_what_was_queued() {
        let (mut feed, mut source) = input_queue(tap(true), 48_000, 2);
        feed.feed(&[0.1, 0.2, 0.3, 0.4]);
        source.clear();
        assert_eq!(source.drain(&mut Vec::new()), 0);
        feed.feed(&[0.5, 0.6]);
        assert_eq!(source.drain(&mut Vec::new()), 1, "after clearing the queue accepts frames again");
    }

    #[test]
    fn a_partial_trailing_frame_is_ignored() {
        let (mut feed, mut source) = input_queue(tap(true), 48_000, 2);
        feed.feed(&[0.1, 0.2, 0.3]);
        let mut out = Vec::new();
        assert_eq!(source.drain(&mut out), 1);
        assert_eq!(out, [0.1, 0.2]);
    }
}
