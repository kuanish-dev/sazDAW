//! Project tracks: the list, plus each track's instrument and name (`TrackControl`).

use schemars::JsonSchema;
use serde::Deserialize;

use crate::codes::tracks as codes;
use crate::{ContractError, MonitorMode, Params, Track, TrackId};

/// How many tracks a project can have (the engine allocates as many channels).
pub const MAX_TRACKS: usize = 64;
/// Bounds of a track's own row height on the timeline (`Track::height`), pixels: from one line of the header (name and
/// buttons, the mixer strip hidden) to a tall row for detailed work.
pub const MIN_TRACK_HEIGHT: u16 = 36;
pub const MAX_TRACK_HEIGHT: u16 = 400;

/// Track and folder colors are names; the interface picks the shade for the theme. The set is closed so project colors
/// look the same in any theme and an AI does not have to guess HEX codes.
pub const TRACK_COLORS: [&str; 12] =
    ["red", "orange", "amber", "yellow", "lime", "green", "teal", "cyan", "blue", "indigo", "violet", "pink"];

/// Checks a color name from [`TRACK_COLORS`]; any other name is `tracks::UNKNOWN_COLOR` listing the allowed ones.
pub fn check_color(color: &str) -> Result<(), ContractError> {
    if TRACK_COLORS.contains(&color) {
        Ok(())
    } else {
        Err(codes::UNKNOWN_COLOR.with("color", color).with("colors", TRACK_COLORS.join(", ")))
    }
}

/// A new track for batch creation.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewTrack {
    pub name: String,
    /// Instrument kind; `None` means an audio track.
    pub kind: Option<String>,
    pub default_pitch: u8,
    /// Parameter values on top of the kind's defaults; unknown names are an error.
    #[serde(default)]
    pub params: Params,
}

/// Project tracks. Job: the list of tracks, their instrument and name; a track immediately gets a source node in the
/// graph connected `→ Master` (the track's volume, pan, mute and solo live on that node, see `GraphControl`).
/// Every mutating method is one undo step; on error the project does not change. An unknown track id is
/// `tracks::NOT_FOUND`.
pub trait TrackControl: Send + Sync {
    fn tracks(&self) -> Vec<Track>;
    /// Creates tracks in one undo step (e.g. when slicing a sample): all or none. Errors: `default_pitch > 127` is
    /// `ErrorKind::Limit`; an unknown kind is `tracks::UNKNOWN_INSTRUMENT`; params on an audio track or an unknown
    /// param name is `ErrorKind::Invalid` (values are clamped to the param's range); more than [`MAX_TRACKS`] tracks
    /// is `ErrorKind::Limit`.
    fn add_tracks(&self, tracks: Vec<NewTrack>) -> Result<Vec<TrackId>, ContractError>;
    /// One track with default params: `kind: Some(kind)` is an instrument track, `None` an audio track. Errors are as
    /// in `add_tracks`.
    fn add_track(&self, name: &str, kind: Option<&str>, default_pitch: u8) -> Result<TrackId, ContractError>;
    /// Removes the track together with its clips, source node (and its connections) and automation lanes; the track's
    /// folder is removed too if it becomes empty.
    fn remove_track(&self, id: TrackId) -> Result<(), ContractError>;
    /// Renames the track and the label of its source node in the graph.
    fn rename_track(&self, id: TrackId, name: &str) -> Result<(), ContractError>;
    /// Sets an instrument param value, clamped to the param's range. A non-number or an audio track is
    /// `ErrorKind::Invalid` (`tracks::NO_INSTRUMENT`), an unknown param name is `ErrorKind::Invalid`. Consecutive edits
    /// of the same param (moving a knob) merge into one undo step.
    fn set_instrument_param(&self, id: TrackId, name: &str, value: f32) -> Result<(), ContractError>;
    /// Changes the track's instrument kind: params become the new kind's defaults with `params` on top (clamped to the
    /// range, an unknown name is `ErrorKind::Invalid`); clips and notes stay, and so does the name unless a new `name`
    /// is given (it also changes the node label in the graph, like `rename_track`). Automation lanes of this track's
    /// instrument params are removed (the new kind has other params). The same kind, empty `params` and no `name`
    /// change nothing and create no undo step. An audio track or a frozen track (`InstrumentSpec::frozen`) is
    /// `ErrorKind::Invalid`, an unknown kind is `tracks::UNKNOWN_INSTRUMENT`. One undo step.
    fn set_instrument(&self, id: TrackId, kind: &str, params: Params, name: Option<&str>) -> Result<(), ContractError>;
    /// Moves the track to position `index` in the list (0 is the top); an `index` past the end is clamped to the end.
    /// The order of `tracks()` is the display order; it does not affect the sound. Moving does not change the track's
    /// folder: the tracks of each folder are then gathered together at the topmost of them (`FolderControl`), so a
    /// track from a folder stays in its block, and one moved above the folder pulls the whole folder up with it.
    fn move_track(&self, id: TrackId, index: usize) -> Result<(), ContractError>;
    /// Input monitoring mode of an audio track (`NodeKind::Audio::monitor`). An instrument track is
    /// `ErrorKind::Invalid`. The same mode changes nothing and creates no undo step. One undo step.
    fn set_monitor(&self, id: TrackId, mode: MonitorMode) -> Result<(), ContractError>;
    /// Track color: a name from [`TRACK_COLORS`] (anything else is `tracks::UNKNOWN_COLOR`), `None` means the color by
    /// row number. The same color creates no undo step. One undo step.
    fn set_color(&self, id: TrackId, color: Option<&str>) -> Result<(), ContractError>;
    /// The track's own row height on the timeline (`Track::height`), `MIN_TRACK_HEIGHT..=MAX_TRACK_HEIGHT` pixels
    /// (outside is `tracks::HEIGHT_OUT_OF_RANGE`); `None` returns the track to the interface setting. The same height
    /// creates no undo step; changes of one track's height within a moment merge into one step. One undo step.
    fn set_height(&self, id: TrackId, height: Option<u16>) -> Result<(), ContractError>;
}
