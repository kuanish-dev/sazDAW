//! Live metering of the master, the meters a mixing or mastering engineer keeps open: loudness (ITU-R BS.1770 /
//! EBU R128), true peak and phase (`MasterMeter`, fed by the engine's `MasterTap`), and listening to the master in mono
//! (`MonoCheck`).
//!
//! Why the measuring runs on the control side and not on the audio thread: the meter is the export's own BS.1770 math
//! (`daw-render`), so a live reading and the exported file's report are the same numbers; the integrated loudness and
//! the loudness range gate over everything since the reset, a history that grows, which the audio thread may not
//! allocate; and the true peak oversamples every frame four times, work the audio callback is better without. The audio
//! thread only copies the stereo master into a preallocated ring of atomics, as cheap as the visualizer's window, and
//! the meter reads every frame from it.

use serde::Serialize;

/// How many frames the engine's master tap holds for the live meter: about 2.7 s at 48 kHz (0.7 s at 192 kHz), many
/// times the meter's reading interval, so it gets every frame even when its thread runs late.
pub const MASTER_TAP_FRAMES: usize = 1 << 17;

/// How many of the newest frames a reading carries for the vectorscope: about 11 ms at 48 kHz, a cycle of a low bass
/// note; the interface keeps the previous ones fading, so the picture stays dense.
pub const VECTOR_FRAMES: usize = 512;

/// At or above this true peak (dBTP) the master clips: a D/A converter or a lossy codec rebuilds the waveform above full
/// scale.
pub const CLIP_DBTP: f32 = 0.0;

/// A stretch of the master for the live meter (`MasterTap::frames_after`).
#[derive(Debug, Clone, PartialEq)]
pub struct MasterFrames {
    /// The number of the first frame in `samples`, counted from when the tap was opened. Later than the reader's `end`
    /// when it fell behind by more than [`MASTER_TAP_FRAMES`]: the frames between were lost.
    pub start: u64,
    /// Interleaved stereo (left, right), oldest first.
    pub samples: Vec<f32>,
    /// The frame (numbered like `start`) at which playback last started from the song start (tick 0), so a meter
    /// starts its integration there; `None` when it has not since the tap was opened.
    pub restart: Option<u64>,
    /// Stream sample rate, Hz.
    pub sample_rate: u32,
}

/// Every frame of the master for the live meter (implemented by the audio engine). Job: hand the meter the master's
/// sound without gaps. The frames are the mix as it is exported: after the master's effects and fader, before the
/// preview voice, the mono check, the ±1 clamp and the metronome. One reader: the live meter.
pub trait MasterTap: Send + Sync {
    /// Opens (`true`) or closes the tap. Closed at creation: the audio thread copies nothing. Opening starts the frame
    /// count from 0 and forgets the restart mark.
    fn open_tap(&self, on: bool);
    /// The frames captured after frame `end` (the end of the previous reading, 0 right after opening), at most the last
    /// [`MASTER_TAP_FRAMES`]. No samples while closed.
    fn frames_after(&self, end: u64) -> MasterFrames;
}

/// What the live meter shows now (`MasterMeter::reading`). Loudness is in LUFS (K-weighted, as the export measures it),
/// peaks in dBTP.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterReading {
    /// Momentary loudness: the last 400 ms. `None`: quieter than −70 LUFS, or less than 400 ms measured.
    pub momentary_lufs: Option<f32>,
    /// Short-term loudness: the last 3 s. `None` as for the momentary one, or less than 3 s measured.
    pub short_term_lufs: Option<f32>,
    /// Integrated loudness since the reset, gated (BS.1770-4): the number a streaming service normalizes by. `None`:
    /// nothing above the −70 LUFS gate yet.
    pub integrated_lufs: Option<f32>,
    /// Loudness range since the reset (EBU Tech 3342), LU: how far the short-term loudness moves between the quiet and
    /// the loud parts (its 10th to 95th percentile, gated). `None`: nothing above the gate in 3 s windows yet.
    pub range_lu: Option<f32>,
    /// The highest true peak of the last 400 ms. `None`: quieter than [`crate::SILENCE_DB`].
    pub true_peak_db: Option<f32>,
    /// The highest true peak since the reset, held.
    pub max_true_peak_db: Option<f32>,
    /// The true peak reached [`CLIP_DBTP`] since the reset.
    pub clipped: bool,
    /// Phase correlation of left and right over the last 400 ms: 1 the same in both (mono), 0 unrelated, −1 opposite
    /// (it cancels out in mono). `None`: silence.
    pub correlation: Option<f32>,
    /// Seconds measured since the reset: how long the integrated loudness and the range have been gathering.
    pub seconds: f64,
    /// The newest frames for the vectorscope: interleaved stereo (left, right), oldest first, at most
    /// [`VECTOR_FRAMES`].
    pub vector: Vec<f32>,
    /// Stream sample rate, Hz (0 before the first frame).
    pub sample_rate: u32,
}

/// The live master meter (implemented over the engine's `MasterTap`). Job: measure the master's loudness, peaks and
/// phase as it plays.
pub trait MasterMeter: Send + Sync {
    /// Starts (`true`) or stops measuring. Off at creation: while nobody looks, the engine copies nothing. Starting begins
    /// a fresh measurement. While on, the meter reads the master on its own, so every frame is measured however rarely
    /// `reading` is called; frames it still loses (a stall of seconds) are logged.
    fn watch(&self, on: bool);
    /// Starts the integrated loudness, the loudness range and the held true peak over (the "Reset" button). Playback
    /// starting from the song start does the same on its own, so a full play-through measures the whole song.
    fn reset(&self);
    /// The current readings; `None` while not watched.
    fn reading(&self) -> Option<MasterReading>;
}

/// Listening to the master in mono (implemented by the audio engine). Job: let the engineer hear how the mix sums to
/// mono. A monitoring mode of this run, like the song loop: not saved with the project and not exported (an offline
/// engine has its own switch, always off). The meters measure the stereo mix before it.
pub trait MonoCheck: Send + Sync {
    /// On: the speakers get the middle of the two channels, `(L + R) / 2`, in both; off: stereo.
    fn set_mono(&self, on: bool);
    fn is_mono(&self) -> bool;
}
