//! The virtual stage (`StageControl`): where each mixer channel stands in one shared room, and the room itself.
//!
//! A channel (a track's source or a bus) placed on the stage is played by the engine as a point source in a room:
//! its left/right is the strip's `pan` (from the left wall to the right wall), [`StagePlace`] adds how far ahead of or
//! behind the listener it stands and how high. The listener stands in the middle of the room, ears halfway up, facing
//! ahead, so a sound can stand all around them. A channel off the stage plays with plain pan, as before. The sound
//! model is the engine's (`daw-engine/src/stage/`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, NodeId};

/// How many channels can stand on the stage at once (the engine preallocates one spatializer per place).
pub const MAX_ON_STAGE: usize = 64;

/// Where a channel stands on the stage, besides its left/right (the strip's `pan`: −1 the left wall … 1 the right wall).
/// All three coordinates are 0 at the listener's head.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StagePlace {
    /// −1 the wall behind the listener … 0 level with the listener's ears … 1 the wall ahead.
    pub depth: f32,
    /// −1 the floor … 0 the listener's ear height … 1 the ceiling.
    pub height: f32,
}

impl StagePlace {
    /// Where a channel first stands: a few steps in front of the listener, at ear height.
    pub const DEFAULT: StagePlace = StagePlace { depth: 0.5, height: 0.0 };

    /// The place with both values in their ranges.
    pub fn clamped(self) -> Self {
        Self { depth: self.depth.clamp(-1.0, 1.0), height: self.height.clamp(-1.0, 1.0) }
    }
}

/// What the stage is rendered for: the direction cues differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Listening {
    /// Two loudspeakers: equal-power panning by left/right (time differences between the ears would smear on speakers).
    /// Two speakers in front cannot put a sound behind the listener: a place behind sounds like its mirror ahead.
    #[default]
    Speakers,
    /// Headphones: binaural cues all around the head — the far ear hears the sound later, quieter and duller, and the
    /// outer ear's tone tells ahead from behind and up from down, so a place behind the listener sounds behind.
    Headphones,
}

/// The shared room every channel on the stage plays in, so the whole stage sounds like one space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Room {
    /// 0 a small studio room (6 × 5 × 3 m) … 1 a concert hall (30 × 24 × 10 m): the walls, the ceiling and how long the
    /// room rings.
    pub size: f32,
    /// 0 no reflections at all (an open-air stage, only the direct sound) … 1 hard, live walls.
    pub reverb: f32,
    pub listening: Listening,
}

impl Default for Room {
    fn default() -> Self {
        Self { size: 0.4, reverb: 0.4, listening: Listening::Speakers }
    }
}

impl Room {
    /// The room with its values in their ranges.
    pub fn clamped(self) -> Self {
        Self { size: self.size.clamp(0.0, 1.0), reverb: self.reverb.clamp(0.0, 1.0), listening: self.listening }
    }
}

/// The virtual stage. Job: where each sound stands in the shared room.
///
/// A channel stands on the stage while its strip has a [`StagePlace`] (`Strip::stage`); its left/right is the strip's
/// `pan`, so the mixer's pan knob, pan automation and MIDI learn move it between the walls. Depth and height can be
/// automated too (`ParamTarget::StageDepth`/`StageHeight`; such lanes exist only while the channel stands on the stage).
///
/// Errors: an unknown node is `graph::NODE_NOT_FOUND`; a node without a strip (an effect, the master) is
/// `graph::NO_STAGE_PLACE`; a value that is not a finite number is `graph::STAGE_NOT_A_NUMBER`; placing one more channel
/// than [`MAX_ON_STAGE`] is `graph::TOO_MANY_ON_STAGE`. Values out of range are clamped. On error nothing changes.
pub trait StageControl: Send + Sync {
    fn room(&self) -> Room;
    /// Puts the channel at this spot on the stage, or moves it there: `pan` is its left/right (−1…1), `place` its depth
    /// and height. Consecutive moves of one channel merge into one undo step (a drag).
    fn place(&self, node: NodeId, pan: f32, place: StagePlace) -> Result<(), ContractError>;
    /// Takes the channel off the stage: it plays with plain pan again (at the pan it stood at), and its depth and height
    /// automation lanes are removed. A channel that is not on the stage stays as it is (no undo step). One undo step.
    fn take_off(&self, node: NodeId) -> Result<(), ContractError>;
    /// Changes the shared room. Consecutive changes merge into one undo step (a slider).
    fn set_room(&self, room: Room) -> Result<(), ContractError>;
}
