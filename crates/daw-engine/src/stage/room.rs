//! The shared room's diffuse tail: a feedback delay network — eight delay lines whose outputs are mixed by a Hadamard
//! matrix and fed back, each losing highs to the walls and as much level per pass as makes the room ring for
//! `RoomShape::ring_seconds`. Every channel on the stage feeds it, so the whole stage sounds like one space. The line
//! lengths are fixed (the room size changes how long it rings and how dark it is, never a length, so moving the size
//! slider never clicks). It sleeps once nothing has fed it for longer than it rings.

use super::dsp::{OnePole, one_pole};
use super::geometry::RoomShape;

const LINES: usize = 8;
/// Line lengths, ms: no two share a factor, so their echoes do not pile up on the same moments (a flutter).
const LENGTHS_MS: [f32; LINES] = [29.7, 37.1, 41.1, 43.7, 47.9, 53.3, 59.9, 67.1];
/// The tail's output level: each channel sums four lines.
const OUTPUT: f32 = 0.5;
/// After its input stops, the tail runs this many ring times (by then it is 90 dB down) before it sleeps.
const RING_OUT: f32 = 1.5;

pub struct RoomTail {
    lines: [Vec<f32>; LINES],
    positions: [usize; LINES],
    /// The level each line keeps per pass.
    feedback: [f32; LINES],
    damping: [OnePole; LINES],
    /// The walls' lowpass coefficient.
    wall: f32,
    /// Frames since anything fed the tail, and how many it rings before it sleeps.
    quiet: usize,
    ringing: usize,
    sample_rate: f32,
}

/// The 8-point Hadamard transform, normalized: an orthogonal mix, so the network loses level only by `feedback`.
fn hadamard(values: &mut [f32; LINES]) {
    let mut span = 1;
    while span < LINES {
        for start in (0..LINES).step_by(span * 2) {
            for i in start..start + span {
                let (a, b) = (values[i], values[i + span]);
                values[i] = a + b;
                values[i + span] = a - b;
            }
        }
        span *= 2;
    }
    let scale = (LINES as f32).sqrt().recip();
    for value in values {
        *value *= scale;
    }
}

impl RoomTail {
    /// A tail with its lines for this sample rate (allocates: only outside the audio thread).
    pub fn new(sample_rate: f32, shape: &RoomShape) -> Self {
        let mut tail = Self {
            lines: LENGTHS_MS.map(|ms| vec![0.0; ((ms / 1000.0 * sample_rate) as usize).max(1)]),
            positions: [0; LINES],
            feedback: [0.0; LINES],
            damping: [OnePole::default(); LINES],
            wall: 1.0,
            quiet: usize::MAX,
            ringing: 0,
            sample_rate,
        };
        tail.set_shape(shape);
        tail
    }

    pub fn set_shape(&mut self, shape: &RoomShape) {
        let rate = self.sample_rate;
        for (feedback, line) in self.feedback.iter_mut().zip(&self.lines) {
            *feedback = 10f32.powf(-3.0 * line.len() as f32 / (rate * shape.ring_seconds));
        }
        self.wall = one_pole(shape.wall_hz, rate);
        self.ringing = (shape.ring_seconds * RING_OUT * rate) as usize + self.lines.iter().map(Vec::len).max().unwrap_or(0);
    }

    /// Adds the room's sound for `input` (stereo, interleaved; `fed` says whether anything was put into it) to `output`.
    pub fn process(&mut self, input: &[f32], fed: bool, output: &mut [f32]) {
        if fed {
            self.quiet = 0;
        } else if self.quiet >= self.ringing {
            return;
        }
        for (into, out) in input.chunks_exact(2).zip(output.chunks_exact_mut(2)) {
            let mut sound = [0.0f32; LINES];
            for ((value, line), position) in sound.iter_mut().zip(&self.lines).zip(&self.positions) {
                *value = line[*position];
            }
            out[0] += OUTPUT * (sound[0] + sound[2] + sound[4] + sound[6]);
            out[1] += OUTPUT * (sound[1] + sound[3] + sound[5] + sound[7]);
            for ((value, damping), feedback) in sound.iter_mut().zip(&mut self.damping).zip(&self.feedback) {
                *value = damping.process(*value, self.wall) * feedback;
            }
            hadamard(&mut sound);
            for (index, ((line, position), value)) in self.lines.iter_mut().zip(&mut self.positions).zip(sound).enumerate() {
                line[*position] = value + into[index % 2];
                *position = (*position + 1) % line.len();
            }
        }
        if !fed {
            self.quiet = self.quiet.saturating_add(input.len() / 2);
        }
    }
}
