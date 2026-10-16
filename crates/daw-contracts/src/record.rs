//! Recording: notes played live (`RecordControl`) and input sound onto audio tracks (`AudioRecordControl`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Clip, ContractError, TrackId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordStatus {
    /// Recording is armed: notes played during playback go into the pattern.
    pub armed: bool,
    /// Recording is happening right now (armed and the transport is playing).
    pub capturing: bool,
    /// Notes recorded in the current or last take.
    pub take_notes: usize,
    /// Note-start quantize while recording, in ticks; 0 means off.
    pub quantize_ticks: u32,
}

/// Recording notes played live (computer keyboard, MIDI keyboard) into a clip on the selected track.
pub trait RecordControl: Send + Sync {
    fn set_armed(&self, armed: bool);
    /// Grid quantize for the start of recorded notes: from 0 (off) to one bar `BAR_TICKS`, otherwise
    /// `ErrorKind::Limit`. Applies to notes recorded after the call.
    fn set_quantize(&self, ticks: u32) -> Result<(), ContractError>;
    fn status(&self) -> RecordStatus;
}

/// Which input channels an audio track monitors and records. The input provides at most the device's first two
/// channels; a mono input's only channel counts as both left and right, so any choice records it (a mono file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum InputChannels {
    /// Pair 1–2: a stereo file (mono for a mono input).
    #[default]
    Stereo,
    /// Channel 1 only: a mono file.
    Left,
    /// Channel 2 only: a mono file.
    Right,
}

impl InputChannels {
    /// Input frame (left, right; equal for a mono input) → the track's left and right sound.
    pub fn sides(self, left: f32, right: f32) -> (f32, f32) {
        match self {
            InputChannels::Stereo => (left, right),
            InputChannels::Left => (left, left),
            InputChannels::Right => (right, right),
        }
    }

    /// Channel count of the recorded file for an input with `input_channels` channels.
    pub fn file_channels(self, input_channels: u16) -> u16 {
        if self == InputChannels::Stereo && input_channels >= 2 { 2 } else { 1 }
    }
}

/// One audio track's input as seen by recording: whether it is armed and which channels it uses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRecordState {
    pub track: TrackId,
    pub armed: bool,
    pub input: InputChannels,
    /// The track's input peak level 0…1 (over its channels); 0 while the input is closed.
    pub left: f32,
    pub right: f32,
}

/// What is happening while recording audio.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRecordStatus {
    /// A take is in progress (from `start` to `stop`).
    pub recording: bool,
    /// The take hit the length limit: the input is already closed, the sound is saved on `stop`.
    pub full: bool,
    pub seconds: f32,
    /// Input peak level 0…1 (channels 1 and 2).
    pub left: f32,
    pub right: f32,
    /// Frames lost (overflow): the recording has a gap; the UI warns about it.
    pub dropped_frames: u64,
    /// Audio tracks that are armed or have an input chosen (the rest are unarmed, `Stereo`).
    pub tracks: Vec<TrackRecordState>,
    /// Why input monitoring is silent (input and output sample rates differ, the input failed to open); `None` if all
    /// is well.
    pub monitor_problem: Option<String>,
}

/// Recording input sound onto audio tracks. Job: collect the input sound during a take and, on stop, place it on every
/// armed track (a WAV in `samples/`, a sample, a clip or a new take of a clip) in one undo step.
/// Arming and input choice are session state and are not saved in the project.
pub trait AudioRecordControl: Send + Sync {
    /// Arms or disarms recording on an audio track. A missing track is `tracks::NOT_FOUND`; an instrument track or a
    /// take in progress is `ErrorKind::Invalid`. An armed track opens the input (levels, monitoring in "auto" mode).
    fn set_armed(&self, track: TrackId, armed: bool) -> Result<(), ContractError>;
    /// Which input channels the track records and monitors. Errors as in `set_armed`.
    fn set_input(&self, track: TrackId, input: InputChannels) -> Result<(), ContractError>;
    /// Starts a take on all armed tracks at once from the current transport position. If the transport is stopped,
    /// starts it; stopping the take stops the transport only if we started it. Tracks deleted since then are
    /// silently disarmed. Errors (the project is unchanged): no armed audio tracks, the project has no file (nowhere
    /// to put the sound), the input failed to open (no input device), a take is already in progress.
    fn start(&self) -> Result<(), ContractError>;
    /// Stops the take and returns the created or changed clips (one per track). The first frames, as many as the
    /// recording latency (`AudioSettings::record_latency_ms`) plus plugin delay compensation, are cut BEFORE the WAV is
    /// written. A take over an audio clip on the same track that the recording covers at least halfway, or lies
    /// entirely inside (of several, the most covered one), becomes that clip's new active take
    /// (`TakeControl::add_take`): the clip does not move or change length, and the sound is aligned to its start (cut
    /// before it, silence in front if later). Otherwise it becomes a new clip at the start bar. Loop recording: when
    /// playback jumped back at the end of the loop region (as it was at `start`) during the take, the take is cut at
    /// the jumps and every pass becomes a take of one clip on the loop region — the clip the loop covers by the same
    /// rule, or a new one at the loop start; the last pass that played to the loop end is active, and a pass cut short
    /// by stop stays as a take that does not play. A pass with no sound inside the clip (it ended before the clip
    /// starts) is not kept, just as any take loses its part before the clip. An empty take or one shorter than the latency is an error and the
    /// project is unchanged. If one track's clip fails, the others are still saved and the error lists the failures.
    fn stop(&self) -> Result<Vec<Clip>, ContractError>;
    fn status(&self) -> AudioRecordStatus;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MonitorMode, MonitorStatus};

    #[test]
    fn input_channels_pick_sides_and_file_width() {
        assert_eq!(InputChannels::Stereo.sides(0.1, 0.2), (0.1, 0.2));
        assert_eq!(InputChannels::Left.sides(0.1, 0.2), (0.1, 0.1));
        assert_eq!(InputChannels::Right.sides(0.1, 0.2), (0.2, 0.2));
        assert_eq!(InputChannels::Stereo.file_channels(2), 2);
        assert_eq!(InputChannels::Stereo.file_channels(1), 1, "a mono input records mono");
        assert_eq!(InputChannels::Right.file_channels(2), 1);
    }

    #[test]
    fn monitor_modes_and_problems() {
        assert!(!MonitorMode::Off.hears_input(true));
        assert!(MonitorMode::On.hears_input(false));
        assert!(MonitorMode::Auto.hears_input(true) && !MonitorMode::Auto.hears_input(false));
        let ok = MonitorStatus { listening: 1, input_rate: 48_000, output_rate: 48_000 };
        assert_eq!(ok.problem(), None);
        assert!(MonitorStatus { input_rate: 44_100, ..ok }.problem().unwrap().contains("44100"));
        assert!(MonitorStatus { input_rate: 0, ..ok }.problem().is_some(), "a closed input is a problem");
        assert_eq!(MonitorStatus { listening: 0, input_rate: 0, ..ok }.problem(), None, "nobody listens");
    }
}
