//! `ProjectMarkers`: the song layout — section markers on the timeline, always in increasing tick order.

use std::sync::Arc;

use daw_contracts::codes::markers;
use daw_contracts::{ContractError, MAX_CLIP_TICK, MAX_MARKER_NAME, MAX_MARKERS, Marker, MarkerControl, MarkerId, Project, tr};

use crate::{ProjectService, ops};

pub struct ProjectMarkers {
    project: Arc<ProjectService>,
}

impl ProjectMarkers {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

fn checked_name(name: &str) -> Result<String, ContractError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(markers::MARKER_NAME_EMPTY.err());
    }
    if name.chars().count() > MAX_MARKER_NAME {
        return Err(markers::MARKER_NAME_TOO_LONG.with("max", MAX_MARKER_NAME));
    }
    Ok(name.to_string())
}

fn check_tick(tick: u32) -> Result<(), ContractError> {
    if tick > MAX_CLIP_TICK {
        return Err(markers::MARKER_TICK_OUT_OF_RANGE.with("tick", tick).with("max", MAX_CLIP_TICK));
    }
    Ok(())
}

/// Two markers on one tick cannot be told apart on the ruler or when jumping, so the second one is refused.
fn check_free(project: &Project, tick: u32, except: Option<MarkerId>) -> Result<(), ContractError> {
    match project.markers.iter().find(|m| m.tick == tick && Some(m.id) != except) {
        Some(other) => Err(markers::MARKER_PLACE_TAKEN.with("name", &other.name).with("tick", tick)),
        None => Ok(()),
    }
}

fn sort(project: &mut Project) {
    project.markers.sort_by_key(|m| m.tick);
}

impl MarkerControl for ProjectMarkers {
    fn markers(&self) -> Vec<Marker> {
        self.project.read(|p| p.markers.clone())
    }

    fn add_marker(&self, tick: u32, name: &str) -> Result<MarkerId, ContractError> {
        check_tick(tick)?;
        let name = checked_name(name)?;
        self.project.edit(&tr!("Add marker", "Добавить маркер"), |project| {
            ops::limit(project.markers.len(), MAX_MARKERS, markers::TOO_MANY_MARKERS)?;
            check_free(project, tick, None)?;
            let id = project.alloc_id();
            project.markers.push(Marker { id, tick, name });
            sort(project);
            Ok(id)
        })
    }

    fn edit_marker(&self, id: MarkerId, tick: Option<u32>, name: Option<&str>) -> Result<(), ContractError> {
        if let Some(tick) = tick {
            check_tick(tick)?;
        }
        let name = name.map(checked_name).transpose()?;
        self.project.edit(&tr!("Edit marker", "Изменить маркер"), |project| {
            if let Some(tick) = tick {
                check_free(project, tick, Some(id))?;
            }
            let marker = project.markers.iter_mut().find(|m| m.id == id).ok_or_else(|| markers::MARKER_NOT_FOUND.with("id", id))?;
            if let Some(tick) = tick {
                marker.tick = tick;
            }
            if let Some(name) = name {
                marker.name = name;
            }
            sort(project);
            Ok(())
        })
    }

    fn remove_marker(&self, id: MarkerId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete marker", "Удалить маркер"), |project| {
            let index = project.markers.iter().position(|m| m.id == id).ok_or_else(|| markers::MARKER_NOT_FOUND.with("id", id))?;
            project.markers.remove(index);
            Ok(())
        })
    }
}
