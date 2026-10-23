//! `ProjectStage` (`StageControl`): the virtual stage — where each channel stands in the shared room (its strip's `pan`
//! and `Strip::stage`) and the room itself (`Project::room`). The sound of it is the engine's (`daw-engine/src/stage/`).

use std::sync::Arc;

use daw_contracts::codes::graph;
use daw_contracts::{ContractError, MAX_ON_STAGE, NodeId, Project, Room, StageControl, StagePlace, Strip, tr};

use crate::{ProjectService, ops};

pub struct ProjectStage {
    project: Arc<ProjectService>,
}

impl ProjectStage {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

fn number(value: f32) -> Result<f32, ContractError> {
    ops::finite(value, graph::STAGE_NOT_A_NUMBER)
}

/// The strip of a node that can stand on the stage (a source or a bus).
fn strip_mut(project: &mut Project, id: NodeId) -> Result<&mut Strip, ContractError> {
    ops::node_mut(project, id)?.kind.strip_mut().ok_or_else(|| graph::NO_STAGE_PLACE.with("id", id))
}

fn on_stage(project: &Project) -> usize {
    project.graph.nodes.iter().filter(|n| n.kind.strip().is_some_and(|s| s.stage.is_some())).count()
}

impl StageControl for ProjectStage {
    fn room(&self) -> Room {
        self.project.read(|p| p.room)
    }

    fn place(&self, node: NodeId, pan: f32, place: StagePlace) -> Result<(), ContractError> {
        let pan = number(pan)?.clamp(-1.0, 1.0);
        let place = StagePlace { depth: number(place.depth)?, height: number(place.height)? }.clamped();
        self.project.edit_keyed(&tr!("Move on stage", "Переместить на сцене"), Some(format!("stage:{node}")), |project| {
            let placed = on_stage(project);
            let strip = strip_mut(project, node)?;
            if strip.stage.is_none() {
                ops::limit(placed, MAX_ON_STAGE, graph::TOO_MANY_ON_STAGE)?;
            }
            strip.pan = pan;
            strip.stage = Some(place);
            Ok(())
        })
    }

    fn take_off(&self, node: NodeId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Take off the stage", "Убрать со сцены"), |project| {
            strip_mut(project, node)?.stage = None;
            ops::drop_stage_lanes(project, node);
            Ok(())
        })
    }

    fn set_room(&self, room: Room) -> Result<(), ContractError> {
        let room = Room { size: number(room.size)?, reverb: number(room.reverb)?, listening: room.listening }.clamped();
        self.project.edit_keyed(&tr!("Room", "Зал"), Some("stage:room".into()), |project| {
            project.room = room;
            Ok(())
        })
    }
}
