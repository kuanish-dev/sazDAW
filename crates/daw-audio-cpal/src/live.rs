//! Monitoring input: a second ring queue of the input (besides the recording queue) that the engine reads on the
//! output thread (`LiveInput`). There is one queue for the whole run: its reading side goes to the engine once, and its
//! writing side passes from one input stream to the next (device change) through `Arc<Mutex<..>>`.
//!
//! The input callback takes the writing side with `try_lock`, like the output takes the engine
//! (`backend::render_block`): it is busy only at the moment input streams are swapped, and then the block simply does
//! not reach monitoring. No allocations, no waiting. While the engine does not want the input (`set_wanted(false)`),
//! frames are not copied.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use daw_contracts::LiveInput;
use rtrb::{Consumer, Producer, RingBuffer};

/// Queue capacity in stereo frames (~0.34 s at 48 kHz): the engine keeps only a block and a small margin in it,
/// the rest covers the output falling behind for a while.
const LIVE_FRAMES: usize = 16_384;

/// Shared by the input callback, the host and the engine: whether the input is wanted and its rate.
#[derive(Default)]
struct LiveState {
    wanted: AtomicBool,
    /// Rate of the open input; 0 means the input is closed.
    rate: AtomicU32,
}

/// The writing side for the input callback (cloned into every new input stream).
#[derive(Clone)]
pub(crate) struct LiveTap {
    producer: Arc<Mutex<Producer<f32>>>,
    state: Arc<LiveState>,
}

/// The reading side: the engine gets it.
pub(crate) struct LiveReader {
    consumer: Consumer<f32>,
    state: Arc<LiveState>,
}

pub(crate) fn live_queue() -> (LiveTap, LiveReader) {
    let (producer, consumer) = RingBuffer::new(LIVE_FRAMES * 2);
    let state = Arc::new(LiveState::default());
    (LiveTap { producer: Arc::new(Mutex::new(producer)), state: state.clone() }, LiveReader { consumer, state })
}

impl LiveTap {
    /// Whether the engine wants the input (tells the host to open or close it).
    pub fn wanted(&self) -> bool {
        self.state.wanted.load(Ordering::Relaxed)
    }

    /// The host opened (rate) or closed (0) the input.
    pub fn set_rate(&self, rate: u32) {
        self.state.rate.store(rate, Ordering::Relaxed);
    }

    /// From the input callback: the first two channels as stereo frames (mono goes to both); what does not fit is
    /// dropped (the engine keeps the queue almost empty anyway).
    pub fn feed(&self, interleaved: &[f32], channels: usize) {
        if channels == 0 || !self.wanted() {
            return;
        }
        let Ok(mut producer) = self.producer.try_lock() else { return };
        let frames = (producer.slots() / 2).min(interleaved.len() / channels);
        let Ok(chunk) = producer.write_chunk_uninit(frames * 2) else { return };
        let right = usize::from(channels > 1);
        chunk.fill_from_iter(interleaved.chunks_exact(channels).take(frames).flat_map(|frame| [frame[0], frame[right]]));
    }
}

impl LiveInput for LiveReader {
    fn set_wanted(&mut self, wanted: bool) {
        self.state.wanted.store(wanted, Ordering::Relaxed);
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.state.rate.load(Ordering::Relaxed)).filter(|rate| *rate > 0)
    }

    fn available(&self) -> usize {
        self.consumer.slots() / 2
    }

    fn read(&mut self, out: &mut [f32]) -> usize {
        let frames = (out.len() / 2).min(self.available());
        let Ok(chunk) = self.consumer.read_chunk(frames * 2) else { return 0 };
        let (first, second) = chunk.as_slices();
        out[..first.len()].copy_from_slice(first);
        out[first.len()..first.len() + second.len()].copy_from_slice(second);
        chunk.commit_all();
        frames
    }

    fn skip(&mut self, frames: usize) {
        let samples = (frames * 2).min(self.consumer.slots());
        if let Ok(chunk) = self.consumer.read_chunk(samples) {
            chunk.commit_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_copied_until_the_engine_wants_the_input() {
        let (tap, mut reader) = live_queue();
        tap.feed(&[0.1, 0.2], 2);
        assert_eq!(reader.available(), 0);
        reader.set_wanted(true);
        assert!(tap.wanted());
        tap.feed(&[0.1, 0.2, 0.3, 0.4], 2);
        let mut out = [0.0; 8];
        assert_eq!(reader.read(&mut out), 2);
        assert_eq!(&out[..4], [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn mono_goes_to_both_sides_and_extra_channels_are_dropped() {
        let (tap, mut reader) = live_queue();
        reader.set_wanted(true);
        tap.feed(&[0.5, 0.6], 1);
        tap.feed(&[1.0, 2.0, 3.0], 3);
        let mut out = [0.0; 6];
        assert_eq!(reader.read(&mut out), 3);
        assert_eq!(out, [0.5, 0.5, 0.6, 0.6, 1.0, 2.0]);
    }

    #[test]
    fn rate_skip_and_overflow() {
        let (tap, mut reader) = live_queue();
        assert_eq!(reader.sample_rate(), None, "the input is closed");
        tap.set_rate(48_000);
        assert_eq!(reader.sample_rate(), Some(48_000));
        reader.set_wanted(true);
        tap.feed(&vec![0.0; (LIVE_FRAMES + 10) * 2], 2);
        assert_eq!(reader.available(), LIVE_FRAMES, "the excess did not fit");
        reader.skip(LIVE_FRAMES - 3);
        assert_eq!(reader.available(), 3);
    }
}
