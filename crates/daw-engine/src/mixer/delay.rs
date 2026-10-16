//! Compensation (PDC) delay lines: a store of preallocated ring buffers. A graph connection that needs a delay gets a
//! line while the delay is needed; the line is bound to the connection (from, to, sidechain), so project edits that do
//! not change the paths do not cut off the sound in the line, and a new delay on the same line crossfades from the old
//! one over a segment (nudging a channel's time alignment earlier moves every other path's delay without a click).

use daw_contracts::NodeId;

/// The longest delay of one connection in frames (~0.68 s at 48 kHz: with room for limiters and linear-phase EQs).
/// The limit is in frames, not seconds: the memory is allocated before the stream rate is known.
pub const MAX_PDC_FRAMES: usize = 1 << 15;
/// How many connections can be delayed at once (only connections bypassing a longer path are delayed).
/// All memory is 32 × 256 KB = 8 MB per engine; it is written right at creation so that the first write to a line
/// from the audio thread does not hit page faults.
pub const MAX_DELAY_LINES: usize = 32;

/// Whose line it is: the connection `from → to` (a sidechain is a separate connection).
pub type EdgeKey = (NodeId, NodeId, bool);

struct DelayLine {
    /// Stereo, interleaved; `MAX_PDC_FRAMES` frames.
    ring: Vec<f32>,
    /// The frame the next input is written to.
    write: usize,
    /// 1..=`MAX_PDC_FRAMES`.
    delay: usize,
    /// The delay the line played with before the current one; the next segment crossfades from it.
    previous: usize,
    owner: Option<EdgeKey>,
    /// Taken in the current layout.
    used: bool,
}

impl DelayLine {
    /// A fresh line for a new path: the old sound of another connection must not be heard.
    fn reset(&mut self, owner: EdgeKey, delay: usize) {
        self.ring.fill(0.0);
        self.write = 0;
        self.owner = Some(owner);
        self.delay = delay;
        self.previous = delay;
        self.used = true;
    }

    /// The stereo frame `delay` frames before the one about to be written.
    fn read(&self, delay: usize) -> (f32, f32) {
        let read = (self.write + MAX_PDC_FRAMES - delay) % MAX_PDC_FRAMES * 2;
        (self.ring[read], self.ring[read + 1])
    }

    /// Adds input `source` delayed by `delay` frames to `target`, with a level going smoothly from `start` to `end`;
    /// after a delay change the segment crossfades from the previous delay to the new one.
    fn run(&mut self, source: &[f32], target: &mut [f32], (start, end): (f32, f32)) {
        let frames = (source.len() / 2).max(1) as f32;
        let step = (end - start) / frames;
        let mut level = start;
        for (index, (input, output)) in source.chunks_exact(2).zip(target.chunks_exact_mut(2)).enumerate() {
            level += step;
            // Read first, then write: this way a delay of exactly the ring length works too.
            let (mut left, mut right) = self.read(self.delay);
            if self.previous != self.delay {
                let (old_left, old_right) = self.read(self.previous);
                let progress = (index + 1) as f32 / frames;
                left = old_left + (left - old_left) * progress;
                right = old_right + (right - old_right) * progress;
            }
            output[0] += left * level;
            output[1] += right * level;
            self.ring[self.write * 2] = input[0];
            self.ring[self.write * 2 + 1] = input[1];
            self.write = (self.write + 1) % MAX_PDC_FRAMES;
        }
        self.previous = self.delay;
    }
}

pub struct DelayLines {
    lines: Vec<DelayLine>,
}

impl DelayLines {
    pub fn new() -> Self {
        let lines = (0..MAX_DELAY_LINES)
            .map(|_| {
                // `resize` writes zeros explicitly (while `vec![0.0; n]` may take pages the system has not handed out yet).
                let mut ring = Vec::with_capacity(MAX_PDC_FRAMES * 2);
                ring.resize(MAX_PDC_FRAMES * 2, 0.0);
                DelayLine { ring, write: 0, delay: 1, previous: 1, owner: None, used: false }
            })
            .collect();
        Self { lines }
    }

    /// The start of a new layout: all lines are free but remember their previous owners.
    pub fn begin(&mut self) {
        for line in &mut self.lines {
            line.used = false;
        }
    }

    /// The line that already served this connection (its sound continues); a new delay is set, and the line glides to
    /// it.
    pub fn keep(&mut self, owner: EdgeKey, delay: usize) -> Option<usize> {
        let index = self.lines.iter().position(|l| !l.used && l.owner == Some(owner))?;
        let line = &mut self.lines[index];
        line.used = true;
        line.delay = delay.clamp(1, MAX_PDC_FRAMES);
        Some(index)
    }

    /// A free line for a new connection; `None` means the lines ran out. Called after `keep` for all connections, so a
    /// free line with a previous owner is no longer needed by it.
    pub fn take(&mut self, owner: EdgeKey, delay: usize) -> Option<usize> {
        let index = self.lines.iter().position(|l| !l.used)?;
        self.lines[index].reset(owner, delay.clamp(1, MAX_PDC_FRAMES));
        Some(index)
    }

    /// The end of a layout: free lines forget their owners.
    pub fn finish(&mut self) {
        for line in &mut self.lines {
            if !line.used {
                line.owner = None;
            }
        }
    }

    pub fn run(&mut self, index: usize, source: &[f32], target: &mut [f32], levels: (f32, f32)) {
        if let Some(line) = self.lines.get_mut(index) {
            line.run(source, target, levels);
        }
    }
}
