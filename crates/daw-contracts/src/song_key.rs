//! The song's key (`KeyControl`): the tonic and the scale the song is written in.

use crate::{ContractError, ScaleKey};

/// The song's key. Job: know which key the song is in (`Project::key`): a tonic and a scale, or none. The key changes
/// no sound by itself; melodic step lines count their scale degrees in it (`ClipControl::add_step_clip`).
///
/// A tonic above 11 is `notes::SCALE_ROOT_OUT_OF_RANGE`. A change is one undo step; setting the key the song already
/// has creates none.
pub trait KeyControl: Send + Sync {
    fn key(&self) -> Option<ScaleKey>;
    /// Sets the key; `None` removes it.
    fn set_key(&self, key: Option<ScaleKey>) -> Result<(), ContractError>;
}
