//! Song layout (`MarkerControl`): section markers on the timeline, such as "Intro", "Verse", "Drop".

use crate::{ContractError, Marker, MarkerId};

/// How many markers a project can have.
pub const MAX_MARKERS: usize = 256;
/// Longest marker name (in characters).
pub const MAX_MARKER_NAME: usize = 64;

/// Song layout. Job: where each section of the song starts.
///
/// A marker is a tick and a name; the `markers()` list is always in increasing tick order. Markers do not affect the
/// sound. Errors: a tick beyond [`crate::MAX_CLIP_TICK`] is `ErrorKind::Limit`; an empty name (after trimming spaces)
/// or one longer than [`MAX_MARKER_NAME`] characters is `ErrorKind::Invalid`; another marker already at this tick is
/// `ErrorKind::Invalid`; more than [`MAX_MARKERS`] is `ErrorKind::Limit`; an unknown id is `ErrorKind::NotFound`.
/// Every mutating method is one undo step; on error the project does not change.
pub trait MarkerControl: Send + Sync {
    fn markers(&self) -> Vec<Marker>;
    /// A new section marker at tick `tick` (the name is trimmed of spaces).
    fn add_marker(&self, tick: u32, name: &str) -> Result<MarkerId, ContractError>;
    /// Moves and/or renames the marker; `None` leaves the field unchanged. If nothing changed, no undo step.
    fn edit_marker(&self, id: MarkerId, tick: Option<u32>, name: Option<&str>) -> Result<(), ContractError>;
    fn remove_marker(&self, id: MarkerId) -> Result<(), ContractError>;
}
