//! `ProjectTakes`: takes of an audio clip — which of the sounds recorded at this place the clip plays on each piece
//! (whole or as a comp of pieces). The rules are pure functions in `take_rules.rs`; takes never change the clip's
//! place or length.

use std::sync::Arc;

use daw_contracts::{Clip, ClipId, CompSegment, ContractError, SampleId, TakeControl, tr};

use crate::{ProjectService, ops, take_rules};

pub struct ProjectTakes {
    project: Arc<ProjectService>,
}

impl ProjectTakes {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

impl TakeControl for ProjectTakes {
    fn add_take(&self, clip: ClipId, sample: SampleId) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("New take", "Новый дубль"), |project| {
            ops::check_sample(project, sample)?;
            let target = ops::clip_mut(project, clip)?;
            take_rules::add(&mut target.content, sample)?;
            Ok(target.clone())
        })
    }

    fn set_active_take(&self, clip: ClipId, take: SampleId) -> Result<Clip, ContractError> {
        // An already active take leaves the project unchanged, so the core records no undo step.
        self.project.edit(&tr!("Choose take", "Выбрать дубль"), |project| {
            let target = ops::clip_mut(project, clip)?;
            take_rules::activate(&mut target.content, take)?;
            Ok(target.clone())
        })
    }

    fn remove_take(&self, clip: ClipId, take: SampleId) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("Delete take", "Удалить дубль"), |project| {
            let target = ops::clip_mut(project, clip)?;
            take_rules::remove(&mut target.content, take)?;
            Ok(target.clone())
        })
    }

    fn set_comp(&self, clip: ClipId, segments: Vec<CompSegment>) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("Comp takes", "Склейка дублей"), |project| {
            let target = ops::clip_mut(project, clip)?;
            take_rules::set_comp(&mut target.content, target.length_ticks, segments)?;
            Ok(target.clone())
        })
    }

    fn choose_take_range(&self, clip: ClipId, from: u32, to: u32, take: SampleId) -> Result<Clip, ContractError> {
        self.project.edit(&tr!("Take segment", "Кусок дубля"), |project| {
            let target = ops::clip_mut(project, clip)?;
            take_rules::choose_range(&mut target.content, target.length_ticks, from, to, take)?;
            Ok(target.clone())
        })
    }
}
