//! Transport: playback control from any thread (`TransportControl`) and the audio thread's clock (`TransportClock`).

use serde::Serialize;

use crate::{BlockTiming, TempoMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportState {
    Stopped,
    Playing,
    Paused,
}

/// Transport control from any thread (UI, MIDI, scripts).
/// No method blocks.
///
/// There is no tempo or time signature here: they are the project's tempo map (`TempoControl`, `Project::tempo_map`),
/// and the clock receives it in every `advance` call from the engine snapshot. The loop also belongs to the project
/// (`ClipControl::set_loop_region`) and is only passed on to the transport, and so does the song end.
pub trait TransportControl: Send + Sync {
    /// Starts playback. From pause or stop with a cue set, playback starts at the cue (not where it paused); while
    /// already playing, nothing moves.
    fn play(&self);
    fn pause(&self);
    /// Stops playback and returns the playhead to the cue, or to the start when there is no cue.
    fn stop(&self);
    fn seek(&self, tick: u64);
    /// The start position ("cue", set by a click on the ruler): `play` from pause/stop and `stop` return here.
    /// `None` clears it. Setting it does not move the playhead by itself. Ticks above `u64::MAX - 1` are clamped.
    fn set_cue(&self, tick: Option<u64>);
    fn cue(&self) -> Option<u64>;
    /// Playback loop `[start, end)` in ticks; `None` means no loop. `end <= start` also turns the loop off; ticks above
    /// `u32::MAX` are clamped to it. Usually set by the project (see the trait docs).
    fn set_loop(&self, region: Option<(u64, u64)>);
    fn loop_region(&self) -> Option<(u64, u64)>;
    /// The song loop, on by default: playback that reaches the song end jumps back to tick 0 on the exact frame and
    /// plays on. The loop region wins while it is set; playback that is already at or past the song end runs on. A mode
    /// of the running app, like the metronome: it is not saved with the project.
    fn set_song_loop(&self, on: bool);
    fn song_loop(&self) -> bool;
    /// Where the song loop jumps back (`Project::song_loop_end`); `None` means an empty arrangement, nothing to loop.
    /// Set by the project on every edit, like the loop region. Ticks above `u32::MAX` are clamped to it.
    fn set_song_end(&self, end: Option<u64>);
    /// A recording asks playback to run straight past the song end, so the take can lengthen the song (the loop region
    /// still wraps). `true` adds a hold, `false` removes one (never below zero); the song loop waits while any hold is
    /// left, so each recorder pairs its own calls.
    fn hold_song_loop(&self, hold: bool);
    /// What playback wraps around now: the loop region, otherwise `(0, song end)` while the song loop is on and not
    /// held; `None` means playback runs on.
    fn wrap_region(&self) -> Option<(u64, u64)>;
    fn state(&self) -> TransportState;
    fn position_ticks(&self) -> u64;
}

/// Longest segment inside a smooth tempo ramp: a segment's tempo is constant, and the shorter the segment, the closer
/// the notes inside it are to the exact time (at 48 kHz this is ~10 ms; the error is a fraction of a millisecond).
pub const RAMP_SEGMENT_FRAMES: usize = 512;

/// The audio thread side. Called from the realtime callback: no allocations or locks.
pub trait TransportClock: Send {
    /// Advances time by at most `max_frames` frames along tempo map `map` and returns the window.
    /// `timing.frames >= 1` if `max_frames >= 1`. The segment is shorter than `max_frames` when it has to jump at the
    /// end of what it wraps around (`TransportControl::wrap_region`: the loop or the song) or change tempo at a map
    /// point (tempo, time signature); inside a smooth tempo ramp the segment is
    /// no longer than [`RAMP_SEGMENT_FRAMES`]. The segment end is computed exactly from the map
    /// (`TempoMap::tick_after_frames`), so the position does not drift; within a segment `ticks_per_frame` is constant
    /// (in a ramp, the segment average).
    fn advance(&mut self, max_frames: usize, sample_rate: u32, map: &TempoMap) -> BlockTiming;
}
