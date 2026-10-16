//! `ProjectMidiMappings` (`MidiMappingControl`): which MIDI controller knob moves which parameter — the mappings in the
//! project (`Project::midi_mappings`), learning (MIDI learn) and moving the parameter when the knob turns.
//!
//! The parameter moves just like with a knob in the interface: through the parameter owners (`GraphControl`,
//! `TrackControl`, `StageControl`) — an undo step merged with neighbouring moves. Right before that the move is reported to the
//! automation writer (`AutomationWriter::touch`, a knob has no release: `TouchEnd::Idle`), which decides whether it
//! becomes lane points. The parameter range is remembered when learning (`min`/`max` of the mapping), so
//! turning the knob never consults the catalogs.

use std::sync::{Arc, Mutex};

use daw_contracts::codes::{graph, midi, tracks};
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AutomationWriter, ContractError, EffectCatalog, GraphControl, InstrumentCatalog, LogEntry, MAX_CC_NUMBER, MAX_LEVEL, MAX_WIDTH,
    MidiMapping, MidiMappingControl, NodeKind, ParamTarget, Project, StageControl, StagePlace, TouchEnd, TrackControl, tr,
};

use crate::ProjectService;
use crate::ops;

/// Parameter owners and catalogs (parameter ranges of effects and instruments).
pub struct MidiMappingDeps {
    pub graph: Arc<dyn GraphControl>,
    pub tracks: Arc<dyn TrackControl>,
    /// A channel's depth and height on the virtual stage.
    pub stage: Arc<dyn StageControl>,
    pub instruments: Arc<dyn InstrumentCatalog>,
    pub effects: Arc<dyn EffectCatalog>,
    /// Knob moves made while the song plays may be written as automation.
    pub writer: Arc<dyn AutomationWriter>,
    pub log: Arc<dyn AppLog>,
}

pub struct ProjectMidiMappings {
    project: Arc<ProjectService>,
    deps: MidiMappingDeps,
    /// The target being learned (not part of the project: learning is interface state of this run).
    learning: Mutex<Option<ParamTarget>>,
}

impl ProjectMidiMappings {
    pub fn new(project: Arc<ProjectService>, deps: MidiMappingDeps) -> Self {
        Self { project, deps, learning: Mutex::new(None) }
    }

    /// The full range of the target's parameter in its units.
    fn range(&self, project: &Project, target: &ParamTarget) -> Result<(f32, f32), ContractError> {
        match target {
            ParamTarget::NodeVolume { .. } | ParamTarget::EdgeLevel { .. } => Ok((0.0, MAX_LEVEL)),
            ParamTarget::NodePan { .. } | ParamTarget::StageDepth { .. } | ParamTarget::StageHeight { .. } => Ok((-1.0, 1.0)),
            ParamTarget::NodeWidth { .. } => Ok((0.0, MAX_WIDTH)),
            ParamTarget::EffectParam { node, param } => {
                let NodeKind::Effect { effect, .. } = &ops::node(project, *node)?.kind else {
                    return Err(graph::NODE_NOT_EFFECT.with("id", node));
                };
                let info = ops::param_info(self.deps.effects.kinds(), effect, param, graph::UNKNOWN_EFFECT_PARAM)?;
                Ok((info.min, info.max))
            }
            ParamTarget::InstrumentParam { track, param } => {
                let kind = ops::instrument_kind(project, *track)?;
                let info = ops::param_info(self.deps.instruments.kinds(), &kind, param, tracks::UNKNOWN_INSTRUMENT_PARAM)?;
                Ok((info.min, info.max))
            }
        }
    }

    /// Sets the target's parameter like a knob in the interface does.
    fn apply(&self, target: &ParamTarget, value: f32) -> Result<(), ContractError> {
        let graph = &self.deps.graph;
        match target {
            ParamTarget::NodeVolume { node } => graph.set_volume(*node, value),
            ParamTarget::NodePan { node } => graph.set_pan(*node, value),
            ParamTarget::NodeWidth { node } => graph.set_width(*node, value),
            ParamTarget::StageDepth { node } => {
                let (pan, place) = self.project.read(|p| ops::stage_spot(p, *node))?;
                self.deps.stage.place(*node, pan, StagePlace { depth: value, ..place })
            }
            ParamTarget::StageHeight { node } => {
                let (pan, place) = self.project.read(|p| ops::stage_spot(p, *node))?;
                self.deps.stage.place(*node, pan, StagePlace { height: value, ..place })
            }
            ParamTarget::EffectParam { node, param } => graph.set_effect_param(*node, param, value),
            ParamTarget::InstrumentParam { track, param } => self.deps.tracks.set_instrument_param(*track, param, value),
            // The edge already exists (the target was checked): another `connect` only changes its level.
            ParamTarget::EdgeLevel { from, to, sidechain } => graph.connect(*from, *to, value, *sidechain),
        }
    }

    /// Learning is armed: the CC is assigned to the target being learned.
    fn finish_learning(&self, target: ParamTarget, channel: u8, cc: u8) {
        let result = self.project.edit(&tr!("Assign MIDI controller", "Назначить MIDI-контроллер"), |project| {
            ops::validate_target(project, &target)?;
            let (min, max) = self.range(project, &target)?;
            project.midi_mappings.retain(|m| m.target != target);
            project.midi_mappings.push(MidiMapping { cc, channel: Some(channel), target, min, max });
            Ok(())
        });
        if let Err(error) = result {
            // The target may have been deleted while waiting for the knob: no mapping is created, the cause goes to the log.
            self.deps.log.record(LogEntry::warn("midi", format!("MIDI controller CC {cc} was not assigned: {error}")));
        }
    }
}

impl MidiMappingControl for ProjectMidiMappings {
    fn mappings(&self) -> Vec<MidiMapping> {
        self.project.read(|p| p.midi_mappings.iter().filter(|m| ops::validate_target(p, &m.target).is_ok()).cloned().collect())
    }

    fn learn(&self, target: ParamTarget) -> Result<(), ContractError> {
        self.project.read(|p| ops::validate_target(p, &target))?;
        *lock(&self.learning) = Some(target);
        Ok(())
    }

    fn cancel_learn(&self) {
        *lock(&self.learning) = None;
    }

    fn learning(&self) -> Option<ParamTarget> {
        lock(&self.learning).clone()
    }

    fn remove_mapping(&self, target: &ParamTarget) -> Result<(), ContractError> {
        self.project.edit(
            &tr!("Remove MIDI controller assignment", "Убрать назначение MIDI-контроллера"),
            |project| {
                let before = project.midi_mappings.len();
                project.midi_mappings.retain(|m| m.target != *target);
                if project.midi_mappings.len() == before {
                    return Err(midi::NO_MAPPING.err());
                }
                Ok(())
            },
        )
    }

    fn control_change(&self, channel: u8, cc: u8, value: u8) -> bool {
        if cc > MAX_CC_NUMBER {
            return false;
        }
        if let Some(target) = lock(&self.learning).take() {
            self.finish_learning(target, channel, cc);
            return true;
        }
        let level = f32::from(value.min(127)) / 127.0;
        let moves: Vec<(ParamTarget, f32)> = self.project.read(|p| {
            p.midi_mappings
                .iter()
                .filter(|m| m.cc == cc && m.channel.is_none_or(|c| c == channel) && ops::validate_target(p, &m.target).is_ok())
                .map(|m| (m.target.clone(), m.min + (m.max - m.min) * level))
                .collect()
        });
        // Edits come after the read: the project lock must not be held, the parameter owners take it themselves.
        for (target, value) in &moves {
            // The writer hears of the move first: while it writes, the move joins the pass's undo step.
            if let Err(error) = self.deps.writer.touch(target.clone(), *value, TouchEnd::Idle) {
                let message = format!("MIDI controller CC {cc}: the move was not written as automation: {error}");
                self.deps.log.record(LogEntry::warn("midi", message));
            }
            if let Err(error) = self.apply(target, *value) {
                self.deps.log.record(LogEntry::warn("midi", format!("MIDI controller CC {cc}: the parameter was not changed: {error}")));
            }
        }
        !moves.is_empty()
    }
}
