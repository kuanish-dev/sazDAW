//! The channel utility on the audio thread: a source's or bus's polarity, width, mono and fine time alignment
//! (`Strip::polarity`, `width`, `mono`, `delay_ms`). The mixer runs it on the channel's sound before the fader, like a
//! console's input section, so the pre-fader sends and the inserts hear the result.
//!
//! * Every change glides over one segment, so nothing clicks: a polarity flip fades that side through silence, a new
//!   width (mono is width 0) moves the side gain, and a new delay crossfades from the old tap to the new one.
//! * Only "later" is a delay here. A channel set earlier plays undelayed and reports its lead as latency to delay
//!   compensation (`lead`, read by `latency.rs`), which holds the rest of the mix back instead.
//! * Each channel with a strip owns a slot with two delay lines that always take its sound in, so a delay set later
//!   plays real history at once. The slot follows the node id across snapshots (`compile.rs` carries it like the
//!   gains); the lines are allocated only by `new` and `prepare`, outside the audio callback.
//! * Neutral settings leave the sound exactly as it is.

use daw_contracts::{MAX_CHANNEL_DELAY_MS, MAX_NODES, Polarity};

use crate::stage::dsp::DelayLine;

/// What a channel's utility does this segment.
#[derive(Clone, Copy)]
pub struct Utility {
    pub polarity: Polarity,
    /// The gain of the side (L − R): the width, or 0 when the channel is mono.
    pub side: f32,
    /// The time alignment in milliseconds: positive later, negative earlier.
    pub delay_ms: f32,
}

/// The gains and delay a channel played with at the end of a segment: the next one glides from them.
#[derive(Clone, Copy, PartialEq)]
struct Applied {
    left: f32,
    right: f32,
    side: f32,
    delay: usize,
}

const NEUTRAL: Applied = Applied { left: 1.0, right: 1.0, side: 1.0, delay: 0 };

struct Channel {
    lines: [DelayLine; 2],
    /// `None` until the channel's first segment, which starts right at its settings.
    applied: Option<Applied>,
    /// Bound to a node in the snapshot being applied.
    used: bool,
}

impl Channel {
    fn new(frames: usize) -> Self {
        Self { lines: [DelayLine::new(frames), DelayLine::new(frames)], applied: None, used: false }
    }
}

pub struct ChannelUtilities {
    channels: Vec<Channel>,
    sample_rate: f32,
    /// The longest delay the lines hold, in frames.
    max_frames: usize,
}

fn max_frames(sample_rate: f32) -> usize {
    (MAX_CHANNEL_DELAY_MS * sample_rate / 1000.0).ceil() as usize
}

fn sign(inverted: bool) -> f32 {
    if inverted { -1.0 } else { 1.0 }
}

fn glide(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

/// The line's sound `to` frames ago, crossfading from `from` frames ago while the delay changes.
fn tap(line: &DelayLine, from: usize, to: usize, progress: f32) -> f32 {
    let now = line.read(to as f32);
    if from == to { now } else { glide(line.read(from as f32), now, progress) }
}

impl ChannelUtilities {
    /// One slot per possible node (allocates: only outside the audio thread).
    pub fn new(sample_rate: f32) -> Self {
        let frames = max_frames(sample_rate);
        Self { channels: (0..MAX_NODES).map(|_| Channel::new(frames)).collect(), sample_rate, max_frames: frames }
    }

    /// A new sample rate: the lines are made anew for it; the channels keep their slots and start from their settings.
    pub fn prepare(&mut self, sample_rate: f32) {
        let frames = max_frames(sample_rate);
        for channel in &mut self.channels {
            *channel = Channel { used: channel.used, ..Channel::new(frames) };
        }
        self.sample_rate = sample_rate;
        self.max_frames = frames;
    }

    /// The time alignment in whole frames at the stream rate, within what the lines hold either way.
    fn frames(&self, delay_ms: f32) -> isize {
        let limit = self.max_frames as isize;
        ((delay_ms * self.sample_rate / 1000.0).round() as isize).clamp(-limit, limit)
    }

    /// How many frames a channel set `delay_ms` earlier leads the rest of the mix: its latency for delay compensation.
    pub fn lead(&self, delay_ms: f32) -> u32 {
        (-self.frames(delay_ms)).max(0) as u32
    }

    /// Binding a snapshot, step 1: every slot is free until a node keeps it.
    pub fn begin(&mut self) {
        for channel in &mut self.channels {
            channel.used = false;
        }
    }

    /// The slot a node carried from the previous snapshot stays its own.
    pub fn keep(&mut self, slot: usize) {
        if let Some(channel) = self.channels.get_mut(slot) {
            channel.used = true;
        }
    }

    /// Binding a snapshot, step 2 (after every `keep`): a free slot, cleared, for a channel new to the mixer.
    pub fn take(&mut self) -> Option<usize> {
        let slot = self.channels.iter().position(|channel| !channel.used)?;
        let channel = &mut self.channels[slot];
        channel.lines.iter_mut().for_each(DelayLine::clear);
        channel.applied = None;
        channel.used = true;
        Some(slot)
    }

    /// Runs the channel in `slot` over its interleaved stereo `buffer`.
    pub fn process(&mut self, slot: usize, buffer: &mut [f32], utility: Utility) {
        let delay = self.frames(utility.delay_ms).max(0) as usize;
        let Some(channel) = self.channels.get_mut(slot) else {
            return;
        };
        let target =
            Applied { left: sign(utility.polarity.invert_left), right: sign(utility.polarity.invert_right), side: utility.side, delay };
        let start = channel.applied.unwrap_or(target);
        channel.applied = Some(target);
        let [left_line, right_line] = &mut channel.lines;
        if start == NEUTRAL && target == NEUTRAL {
            // The lines still take the sound in, so a delay set later has history to play.
            for frame in buffer.chunks_exact(2) {
                left_line.push(frame[0]);
                right_line.push(frame[1]);
            }
            return;
        }
        let frames = (buffer.len() / 2).max(1) as f32;
        for (index, frame) in buffer.chunks_exact_mut(2).enumerate() {
            let progress = (index + 1) as f32 / frames;
            let left = frame[0] * glide(start.left, target.left, progress);
            let right = frame[1] * glide(start.right, target.right, progress);
            let mid = (left + right) * 0.5;
            let side = (left - right) * 0.5 * glide(start.side, target.side, progress);
            left_line.push(mid + side);
            right_line.push(mid - side);
            frame[0] = tap(left_line, start.delay, target.delay, progress);
            frame[1] = tap(right_line, start.delay, target.delay, progress);
        }
    }
}
