//! `folders.*` methods: thin adapters over `FolderControl` (track folders).

use daw_contracts::{FolderEdit, FolderId, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Id;

#[derive(Deserialize, JsonSchema)]
struct Create {
    name: String,
    /// The folder's tracks in the desired order (at least one); they line up in place of the topmost of them.
    tracks: Vec<TrackId>,
}

#[derive(Deserialize, JsonSchema)]
struct Edit {
    id: FolderId,
    #[serde(flatten)]
    edit: FolderEdit,
}

#[derive(Deserialize, JsonSchema)]
struct SetTracks {
    id: FolderId,
    /// The folder's full new contents in order; tracks not in the list leave the folder.
    tracks: Vec<TrackId>,
}

#[derive(Deserialize, JsonSchema)]
struct SetFlag {
    id: FolderId,
    on: bool,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "folders.create",
        "Creates a track folder (e.g. \"Drums\" from kick, snare and hats): the tracks line up under the folder header; returns the id. View only — sound and graph don't change; a track can be in only one folder.",
        |c, p: Create| c.folders.create_folder(&p.name, &p.tracks),
    );
    table.add(
        "folders.edit",
        "Edits a folder: name (name), whether it is collapsed (collapsed), color (color — one of red, orange, amber, yellow, lime, green, teal, cyan, blue, indigo, violet, pink; null for no color).",
        |c, p: Edit| c.folders.edit_folder(p.id, p.edit),
    );
    table.add("folders.remove", "Removes a folder; its tracks stay where they are.", |c, p: Id| c.folders.remove_folder(p.id));
    table.add(
        "folders.setTracks",
        "Sets the whole contents and order of a folder (this is how a track is put into a folder at a given place); an empty list is an error, use folders.remove for that.",
        |c, p: SetTracks| c.folders.set_folder_tracks(p.id, &p.tracks),
    );
    table.add("folders.setMute", "Mutes (on) or unmutes all tracks of a folder in one undo step.", |c, p: SetFlag| {
        c.folders.set_folder_mute(p.id, p.on)
    });
    table.add("folders.setSolo", "Solos (on) or unsolos all tracks of a folder in one undo step.", |c, p: SetFlag| {
        c.folders.set_folder_solo(p.id, p.on)
    });
}
