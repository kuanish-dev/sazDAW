//! Track folders (`FolderControl`): labeled groups of consecutive tracks that can be collapsed.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ContractError, Folder, FolderId, TrackId};

/// A partial folder edit; a field that is not passed stays unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct FolderEdit {
    pub name: Option<String>,
    /// Collapse (`true`) or expand the folder.
    pub collapsed: Option<bool>,
    /// A color from [`crate::TRACK_COLORS`]: `Some(Some(..))` sets it, `Some(None)` (`null` in JSON) removes it.
    #[serde(deserialize_with = "crate::clips::present")]
    #[allow(clippy::option_option, reason = "three JSON cases: no field, `null` (remove the color) and a value")]
    pub color: Option<Option<String>>,
}

/// Track folders. Job: which tracks are shown together under one label.
///
/// A folder is only a view: it does not change the sound, the processing order or graph nodes. A track lies in at most
/// one folder (`Track::folder`), and a folder's tracks always stand together in the list: after every edit of the
/// membership or order they are gathered at the topmost of them in their relative order. A folder cannot be empty:
/// once its last track is removed from it (by deleting it or moving it to another folder), the folder is deleted.
/// Errors: an unknown folder or track is `ErrorKind::NotFound`; an empty track list or a repeated track in it, or an
/// empty name (after trimming spaces) is `ErrorKind::Invalid`; a color not from [`crate::TRACK_COLORS`] is
/// `tracks::UNKNOWN_COLOR`. Every mutating method is one undo step; on error the project does not change; an edit that
/// changes nothing creates no undo step.
pub trait FolderControl: Send + Sync {
    fn folders(&self) -> Vec<Folder>;
    /// A new folder of tracks `tracks` (they leave their previous folders): they line up in the order of `tracks` at
    /// the position of the topmost of them.
    fn create_folder(&self, name: &str, tracks: &[TrackId]) -> Result<FolderId, ContractError>;
    fn edit_folder(&self, id: FolderId, edit: FolderEdit) -> Result<(), ContractError>;
    /// Removes the folder; its tracks stay in place outside any folder.
    fn remove_folder(&self, id: FolderId) -> Result<(), ContractError>;
    /// Sets the folder's membership and order to exactly `tracks`: new tracks leave their previous folders, tracks
    /// not listed leave the folder and stay in place. The block takes the position of the topmost of the folder's
    /// current tracks that remain in it (if none, the topmost of `tracks`). This puts a track into a folder at the
    /// right spot in one step.
    fn set_folder_tracks(&self, id: FolderId, tracks: &[TrackId]) -> Result<(), ContractError>;
    /// Mutes all tracks of the folder (on their source nodes, like `GraphControl::set_mute`) in one undo step.
    fn set_folder_mute(&self, id: FolderId, mute: bool) -> Result<(), ContractError>;
    /// Solos all tracks of the folder in one undo step.
    fn set_folder_solo(&self, id: FolderId, solo: bool) -> Result<(), ContractError>;
}
