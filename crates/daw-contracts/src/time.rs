//! Time: the tick grid (`PPQ`), tempo (`Bpm`), time signature and the time window of one audio block.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContractError;
use crate::codes::tempo as codes;

/// Ticks per quarter note. The shared time grid for the whole app.
pub const PPQ: u32 = 960;

/// Tempo in beats per minute.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "f64", into = "f64")]
pub struct Bpm(f64);

impl Bpm {
    pub const MIN: f64 = 20.0;
    pub const MAX: f64 = 999.0;
    pub const DEFAULT: Bpm = Bpm(130.0);

    pub fn new(value: f64) -> Result<Self, ContractError> {
        if value.is_finite() && (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(codes::INVALID_TEMPO.with("bpm", value).with("min", Self::MIN).with("max", Self::MAX))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }

    /// How many ticks pass in one frame at sample rate `sample_rate`.
    /// The only formula for converting ticks to frames: frames = ticks / this, ticks = frames × this.
    pub fn ticks_per_frame(self, sample_rate: u32) -> f64 {
        self.0 / 60.0 * f64::from(PPQ) / f64::from(sample_rate.max(1))
    }
}

impl TryFrom<f64> for Bpm {
    type Error = ContractError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Bpm> for f64 {
    fn from(bpm: Bpm) -> f64 {
        bpm.0
    }
}

impl Default for Bpm {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct RawSignature {
    numerator: u8,
    denominator: u8,
}

/// Time signature. The denominator is a power of two from 1 to 32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "RawSignature", into = "RawSignature")]
pub struct TimeSignature {
    numerator: u8,
    denominator: u8,
}

impl TryFrom<RawSignature> for TimeSignature {
    type Error = ContractError;

    fn try_from(raw: RawSignature) -> Result<Self, Self::Error> {
        Self::new(raw.numerator, raw.denominator)
    }
}

impl From<TimeSignature> for RawSignature {
    fn from(signature: TimeSignature) -> Self {
        RawSignature { numerator: signature.numerator, denominator: signature.denominator }
    }
}

impl TimeSignature {
    pub const FOUR_FOUR: TimeSignature = TimeSignature { numerator: 4, denominator: 4 };

    pub fn new(numerator: u8, denominator: u8) -> Result<Self, ContractError> {
        let valid = (1..=32).contains(&numerator) && (1..=32).contains(&denominator) && denominator.is_power_of_two();
        if valid {
            Ok(Self { numerator, denominator })
        } else {
            Err(codes::INVALID_SIGNATURE.with("numerator", numerator).with("denominator", denominator))
        }
    }

    pub fn numerator(self) -> u8 {
        self.numerator
    }

    pub fn denominator(self) -> u8 {
        self.denominator
    }

    /// Length of one beat in ticks.
    pub fn ticks_per_beat(self) -> f64 {
        f64::from(PPQ) * 4.0 / f64::from(self.denominator)
    }

    /// Length of a bar in ticks.
    pub fn ticks_per_bar(self) -> f64 {
        self.ticks_per_beat() * f64::from(self.numerator)
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::FOUR_FOUR
    }
}

/// The time window of one audio segment.
///
/// The tick range is half-open: `[start_tick, start_tick + frames * ticks_per_frame)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockTiming {
    /// How many frames this segment covers (may be fewer than requested if the loop end falls on the boundary).
    pub frames: usize,
    /// Whether playback is running in this segment.
    pub playing: bool,
    pub start_tick: f64,
    pub ticks_per_frame: f64,
    /// Tempo at the segment start (inside a smooth ramp it changes from segment to segment).
    pub tempo: Bpm,
    /// The segment's time signature: segments are split at signature changes, so it is constant within a segment.
    pub time_signature: TimeSignature,
    /// The tick `time_signature` starts at (a bar start): bars are counted from it.
    pub signature_start: f64,
    /// What playback wraps around `(start, end)` in ticks — the loop region, or the song while the song loop is on
    /// (`TransportControl::wrap_region`) — if playback is running (for plugins: `kCycleActive`).
    pub loop_region: Option<(u64, u64)>,
}

impl BlockTiming {
    /// Start of the bar containing `tick` (by the segment's time signature).
    pub fn bar_start(&self, tick: f64) -> f64 {
        let bar = self.time_signature.ticks_per_bar();
        self.signature_start + ((tick - self.signature_start) / bar).floor() * bar
    }

    pub fn end_tick(&self) -> f64 {
        self.start_tick + self.ticks_per_frame * self.frames as f64
    }

    /// The segment frame at which moment `tick` sounds: the first frame not earlier than it (with a margin for rounding
    /// errors; a moment before the segment start is frame 0). One formula for notes, audio clips and the metronome, so
    /// everything sounds in time.
    pub fn frame_at(&self, tick: f64) -> usize {
        if self.ticks_per_frame <= 0.0 {
            return 0;
        }
        ((tick - self.start_tick) / self.ticks_per_frame - 1e-6).ceil().max(0.0) as usize
    }
}
