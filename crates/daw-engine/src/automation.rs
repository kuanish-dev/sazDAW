//! The automation player: for every segment takes the lane values at the current position (the clip under it,
//! `lane_value_at`; between clips a lane leaves its param alone) and hands them to their
//! targets — node volume, pan, stage depth and height and connection levels to the mixer (as a per-segment override), effect and instrument
//! params to their objects (only when the value changes: `set_param` goes by string). Lanes held by live automation
//! writing are skipped: the param plays its project value, which the performer is moving.

use daw_contracts::{BlockTiming, EngineSnapshot, ParamTarget, lane_value_at};

use crate::instruments::InstrumentRack;
use crate::link::HeldLanes;
use crate::mixer::GraphMixer;

const MAX_LANES: usize = 256;

pub struct AutomationPlayer {
    /// The last sent value of each lane (`NaN` means not sent yet).
    last: Vec<f32>,
    active: bool,
}

impl AutomationPlayer {
    pub fn new() -> Self {
        Self { last: Vec::with_capacity(MAX_LANES), active: false }
    }

    /// A new snapshot has re-sent the params to the objects: everything automation sets must be sent again.
    pub fn forget_sent(&mut self, snapshot: &EngineSnapshot) {
        self.last.clear();
        self.last.resize(snapshot.project.automation.len().min(MAX_LANES), f32::NAN);
    }

    /// Applies automation for the segment. `true` means automation has just ended (a stop or the lanes are gone):
    /// the object params must return to the project values.
    pub fn apply(
        &mut self,
        snapshot: Option<&EngineSnapshot>,
        timing: &BlockTiming,
        held: &HeldLanes,
        mixer: &mut GraphMixer,
        instruments: &mut InstrumentRack,
    ) -> bool {
        mixer.clear_overrides();
        let lanes = snapshot.filter(|s| timing.playing && !s.project.automation.is_empty());
        let Some(snapshot) = lanes else {
            return std::mem::take(&mut self.active);
        };
        self.active = true;
        for (index, lane) in snapshot.project.automation.iter().take(MAX_LANES).enumerate() {
            if held.contains(lane.id) {
                self.forget(index);
                continue;
            }
            let Some(value) = lane_value_at(lane, timing.start_tick) else {
                continue;
            };
            match &lane.target {
                ParamTarget::NodeVolume { node } => mixer.override_volume(*node, value),
                ParamTarget::NodePan { node } => mixer.override_pan(*node, value),
                ParamTarget::NodeWidth { node } => mixer.override_width(*node, value),
                ParamTarget::StageDepth { node } => mixer.override_stage_depth(*node, value),
                ParamTarget::StageHeight { node } => mixer.override_stage_height(*node, value),
                ParamTarget::EdgeLevel { from, to, sidechain } => mixer.override_edge_level(*from, *to, *sidechain, value),
                ParamTarget::EffectParam { node, param } => {
                    if self.changed(index, value) {
                        mixer.set_effect_param(*node, param, value);
                    }
                }
                ParamTarget::InstrumentParam { track, param } => {
                    if self.changed(index, value) {
                        instruments.set_param(*track, param, value);
                    }
                }
            }
        }
        false
    }

    fn forget(&mut self, lane: usize) {
        if let Some(last) = self.last.get_mut(lane) {
            *last = f32::NAN;
        }
    }

    /// Remembers the lane value; `true` if it differs from the sent one.
    fn changed(&mut self, lane: usize, value: f32) -> bool {
        match self.last.get_mut(lane) {
            Some(last) if *last == value => false,
            Some(last) => {
                *last = value;
                true
            }
            None => true,
        }
    }
}
