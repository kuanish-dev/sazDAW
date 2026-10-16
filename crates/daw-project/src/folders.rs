//! `ProjectFolders`: track folders — which tracks are shown together under one label. The rules (tracks in a row, no
//! empty folders) are in `folder_rules.rs`.

use std::sync::Arc;

use daw_contracts::codes::markers;
use daw_contracts::{ContractError, Folder, FolderControl, FolderEdit, FolderId, Project, Strip, TrackId, check_color, tr};

use crate::folder_rules;
use crate::{ProjectService, ops};

pub struct ProjectFolders {
    project: Arc<ProjectService>,
}

impl ProjectFolders {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }

    /// Changes the strip (mute/solo) of the source node of every track in the folder in one edit.
    fn set_strips(&self, label: &str, id: FolderId, change: impl Fn(&mut Strip)) -> Result<(), ContractError> {
        self.project.edit(label, |project| {
            folder_mut(project, id)?;
            let nodes: Vec<_> =
                project.tracks.iter().filter(|t| t.folder == Some(id)).filter_map(|t| ops::source_node(project, t.id)).collect();
            for node in nodes {
                if let Some(strip) = ops::node_mut(project, node)?.kind.strip_mut() {
                    change(strip);
                }
            }
            Ok(())
        })
    }
}

fn folder_mut(project: &mut Project, id: FolderId) -> Result<&mut Folder, ContractError> {
    project.folders.iter_mut().find(|f| f.id == id).ok_or_else(|| markers::FOLDER_NOT_FOUND.with("id", id))
}

fn checked_name(name: &str) -> Result<String, ContractError> {
    let name = name.trim();
    if name.is_empty() { Err(markers::FOLDER_NAME_EMPTY.err()) } else { Ok(name.to_string()) }
}

impl FolderControl for ProjectFolders {
    fn folders(&self) -> Vec<Folder> {
        self.project.read(|p| p.folders.clone())
    }

    fn create_folder(&self, name: &str, tracks: &[TrackId]) -> Result<FolderId, ContractError> {
        let name = checked_name(name)?;
        self.project.edit(&tr!("Create folder", "Создать папку"), |project| {
            folder_rules::check_tracks(project, tracks)?;
            let anchor = folder_rules::topmost(project, tracks).ok_or_else(|| markers::FOLDER_WITHOUT_TRACKS.err())?;
            let id = project.alloc_id();
            project.folders.push(Folder { id, name, collapsed: false, color: None });
            folder_rules::place(project, id, tracks, anchor);
            Ok(id)
        })
    }

    fn edit_folder(&self, id: FolderId, edit: FolderEdit) -> Result<(), ContractError> {
        let name = edit.name.as_deref().map(checked_name).transpose()?;
        if let Some(Some(color)) = &edit.color {
            check_color(color)?;
        }
        self.project.edit(&tr!("Edit folder", "Изменить папку"), |project| {
            let folder = folder_mut(project, id)?;
            if let Some(name) = name {
                folder.name = name;
            }
            if let Some(collapsed) = edit.collapsed {
                folder.collapsed = collapsed;
            }
            if let Some(color) = edit.color {
                folder.color = color;
            }
            Ok(())
        })
    }

    fn remove_folder(&self, id: FolderId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Remove folder", "Убрать папку"), |project| {
            folder_mut(project, id)?;
            project.folders.retain(|f| f.id != id);
            for track in project.tracks.iter_mut().filter(|t| t.folder == Some(id)) {
                track.folder = None;
            }
            Ok(())
        })
    }

    fn set_folder_tracks(&self, id: FolderId, tracks: &[TrackId]) -> Result<(), ContractError> {
        self.project.edit(&tr!("Folder tracks", "Дорожки папки"), |project| {
            folder_mut(project, id)?;
            folder_rules::check_tracks(project, tracks)?;
            let staying: Vec<TrackId> =
                project.tracks.iter().filter(|t| t.folder == Some(id) && tracks.contains(&t.id)).map(|t| t.id).collect();
            let anchor = staying
                .first()
                .copied()
                .or_else(|| folder_rules::topmost(project, tracks))
                .ok_or_else(|| markers::FOLDER_WITHOUT_TRACKS.err())?;
            // Tracks not listed leave the folder and stay where they are.
            for track in project.tracks.iter_mut().filter(|t| t.folder == Some(id) && !tracks.contains(&t.id)) {
                track.folder = None;
            }
            folder_rules::place(project, id, tracks, anchor);
            Ok(())
        })
    }

    fn set_folder_mute(&self, id: FolderId, mute: bool) -> Result<(), ContractError> {
        self.set_strips(&tr!("Mute folder", "Mute папки"), id, |strip| strip.mute = mute)
    }

    fn set_folder_solo(&self, id: FolderId, solo: bool) -> Result<(), ContractError> {
        self.set_strips(&tr!("Solo folder", "Solo папки"), id, |strip| strip.solo = solo)
    }
}
