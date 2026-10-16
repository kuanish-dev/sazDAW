//! Input monitoring (audio thread): the input sound is added to the node buffer of every audio track that listens to it
//! (mode `On`, or `Auto` with recording armed); from there it goes through the track fader and effects, like its clips.
//!
//! Input and output are different streams (sometimes different devices), their blocks do not arrive in step, and the
//! clocks drift slightly. So reading uses a small "jitter buffer": start once the input holds a block plus a margin;
//! when frames run short, the block tail is silent, the loss is counted (`EngineLosses::monitor_underruns`) and the
//! margin doubles (up to a limit); when noticeably more than the margin piles up, the excess is dropped so the latency
//! does not grow. If the input rate differs from the output rate, the input is not monitored at all (no real-time
//! resampling); `MonitorStatus` shows this. No allocations and no locks: the buffer is preallocated, the input is `LiveInput`.

use daw_contracts::{InputChannels, LiveInput, MAX_TRACKS};

use crate::SEGMENT_FRAMES;
use crate::link::MonitorTable;
use crate::mixer::GraphMixer;

/// The initial margin of frames beyond a block: small, so the monitoring latency is barely noticeable.
const START_MARGIN: usize = 128;
/// The margin grows no further (~85 ms at 48 kHz): beyond that monitoring makes no sense.
const MAX_MARGIN: usize = 4096;

pub struct InputListener {
    input: Option<Box<dyn LiveInput>>,
    /// The input block that was read (stereo).
    block: Vec<f32>,
    /// The listening nodes of this segment: node index and channels.
    targets: Vec<(usize, InputChannels)>,
    margin: usize,
    /// The input has built up its margin and blocks come in a row; after a shortage, wait for the margin again.
    primed: bool,
}

impl InputListener {
    pub fn new() -> Self {
        Self {
            input: None,
            block: vec![0.0; SEGMENT_FRAMES * 2],
            targets: Vec::with_capacity(MAX_TRACKS),
            margin: START_MARGIN,
            primed: false,
        }
    }

    /// A new input; the previous one is returned to be freed outside the audio thread.
    pub fn replace(&mut self, input: Box<dyn LiveInput>) -> Option<Box<dyn LiveInput>> {
        self.primed = false;
        self.input.replace(input)
    }

    /// Adds the input to the buffers of the listening tracks. The input table and the output rate come from the link.
    /// `true` means frames ran short (a loss for the log).
    pub fn render(&mut self, mixer: &mut GraphMixer, table: &MonitorTable, frames: usize, output_rate: u32) -> bool {
        let wanted = self.collect_targets(mixer, table);
        let Some(input) = self.input.as_mut() else {
            table.report(self.targets.len() as u32, 0);
            return false;
        };
        input.set_wanted(wanted);
        let rate = input.sample_rate();
        table.report(self.targets.len() as u32, rate.unwrap_or(0));
        if self.targets.is_empty() || rate != Some(output_rate) {
            // Nobody listens, so the input does not pile up: otherwise the first monitoring would sound late.
            input.skip(input.available());
            self.primed = false;
            return false;
        }
        let frames = frames.min(SEGMENT_FRAMES);
        let available = input.available();
        if !self.primed {
            if available < frames + self.margin {
                return false;
            }
            self.primed = true;
        }
        let target = frames + self.margin;
        if available > target + self.margin + SEGMENT_FRAMES {
            input.skip(available - target);
        }
        let block = &mut self.block[..frames * 2];
        let got = input.read(block);
        let underrun = got < frames;
        if underrun {
            block[got * 2..].fill(0.0);
            self.primed = false;
            self.margin = (self.margin * 2).min(MAX_MARGIN);
        }
        for &(index, channels) in &self.targets {
            if let Some(buffer) = mixer.buffer_mut(index, frames) {
                for (out, frame) in buffer.chunks_exact_mut(2).zip(self.block.chunks_exact(2)) {
                    let (left, right) = channels.sides(frame[0], frame[1]);
                    out[0] += left;
                    out[1] += right;
                }
            }
        }
        underrun
    }

    /// The listening nodes of the segment; `true` means the input is needed (someone listens or recording is armed: levels and a fast record start).
    fn collect_targets(&mut self, mixer: &GraphMixer, table: &MonitorTable) -> bool {
        self.targets.clear();
        let mut armed = false;
        for index in 0..mixer.node_count() {
            let Some((track, mode)) = mixer.audio_source(index) else { continue };
            let input = table.get(track);
            armed |= input.armed;
            if mode.hears_input(input.armed) && self.targets.len() < self.targets.capacity() {
                self.targets.push((index, input.channels));
            }
        }
        armed || !self.targets.is_empty()
    }
}
