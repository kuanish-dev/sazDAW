//! Track folder rules — pure functions over the model shared by `ProjectFolders` and `ProjectTracks`: a folder's
//! tracks stand in a row, and a folder without tracks does not live.

use std::collections::HashSet;

use daw_contracts::codes::markers;
use daw_contracts::{ContractError, FolderId, Project, TrackId};

use crate::ops;

/// Checks a folder's track list: not empty, no repeats, every track exists.
pub fn check_tracks(project: &Project, tracks: &[TrackId]) -> Result<(), ContractError> {
    if tracks.is_empty() {
        return Err(markers::FOLDER_WITHOUT_TRACKS.err());
    }
    let mut seen = HashSet::new();
    for &id in tracks {
        ops::track(project, id)?;
        if !seen.insert(id) {
            return Err(markers::FOLDER_TRACK_TWICE.with("id", id));
        }
    }
    Ok(())
}

/// Puts tracks `tracks` in a row, in the given order, into folder `folder` at the place of track `anchor` (one of
/// `tracks`), then gathers all folders (`gather`).
pub fn place(project: &mut Project, folder: FolderId, tracks: &[TrackId], anchor: TrackId) {
    let at = project.tracks.iter().position(|t| t.id == anchor).unwrap_or(0);
    let before = project.tracks.iter().take(at).filter(|t| tracks.contains(&t.id)).count();
    let mut block = Vec::with_capacity(tracks.len());
    for &id in tracks {
        if let Some(index) = project.tracks.iter().position(|t| t.id == id) {
            let mut track = project.tracks.remove(index);
            track.folder = Some(folder);
            block.push(track);
        }
    }
    let at = (at - before).min(project.tracks.len());
    project.tracks.splice(at..at, block);
    gather(project);
}

/// Gathers each folder's tracks in a row at the topmost of them (in their relative order) and removes folders without
/// tracks. Called after any edit of order or membership, so the rule is kept in one place.
pub fn gather(project: &mut Project) {
    let known: HashSet<FolderId> = project.folders.iter().map(|f| f.id).collect();
    let mut tracks = std::mem::take(&mut project.tracks);
    for track in &mut tracks {
        track.folder = track.folder.filter(|f| known.contains(f));
    }
    let mut done = HashSet::new();
    for track in &tracks {
        match track.folder {
            None => project.tracks.push(track.clone()),
            Some(folder) if done.insert(folder) => {
                project.tracks.extend(tracks.iter().filter(|t| t.folder == Some(folder)).cloned());
            }
            Some(_) => {}
        }
    }
    project.folders.retain(|f| done.contains(&f.id));
}

/// The topmost of `tracks` in the current order.
pub fn topmost(project: &Project, tracks: &[TrackId]) -> Option<TrackId> {
    project.tracks.iter().map(|t| t.id).find(|id| tracks.contains(id))
}
