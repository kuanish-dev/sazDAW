//! Writing automation live (`AutomationWriter`): param moves made while the song plays become lane points, and the
//! engine side of it (`AutomationHold`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, LaneId, ParamTarget};

/// How long a touch that has no release signal (a MIDI knob) stays "held" after its last move.
pub const TOUCH_IDLE_MS: u64 = 600;

/// Global automation write mode (not part of the project: it is a state of this run, like record arming).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AutomationWriteMode {
    /// Param moves are not written.
    #[default]
    Off,
    /// A param is written while its control is held; on release the lane plays again.
    Touch,
    /// Once moved, a param keeps writing its last value until the transport stops.
    Latch,
}

/// How a touch ends in `Touch` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TouchEnd {
    /// The control sends a release ([`AutomationWriter::release`]): a mouse on a slider.
    Release,
    /// No release signal: the touch ends [`TOUCH_IDLE_MS`] after the last move (a MIDI knob).
    Idle,
}

/// Automation writing. Job: turn param moves made while the transport plays into points of the param's lane clips.
///
/// While the mode is not `Off` and the transport plays, every [`touch`](Self::touch) writes the value at the current
/// play position into the target's lane (created if missing): into the clip there at its pattern position (a looped
/// clip's pattern changes for every repeat), or, outside clips, into a new clip that starts at the bar of the first
/// write and grows (loop = length) as writing goes on, up to the next clip. The span covered by one continuous touch
/// replaces the pattern points that were there. The param itself is moved by its owner as usual (the caller does that right after
/// `touch`, so the move joins the pass's undo step); while a target is being written the engine does not play its lane (the performer's value wins). Writing a
/// target stops on release or idle timeout (`Touch`) or when the transport stops (`Latch`; `Touch` too). Everything
/// one play pass changes in the project — written points and the param moves themselves — is ONE undo step that
/// closes when the transport stops or the mode becomes `Off`.
pub trait AutomationWriter: Send + Sync {
    fn mode(&self) -> AutomationWriteMode;
    /// Switching to `Off` ends writing at once (what was written stays).
    fn set_mode(&self, mode: AutomationWriteMode);
    /// A performer is moving `target` to `value` (called right before the param is set). Does nothing when the mode
    /// is `Off` or the transport is not playing.
    /// An invalid target (no such node, track or param) is the error of `AutomationControl::add_lane`; a non-number
    /// value is `ErrorKind::Invalid`.
    fn touch(&self, target: ParamTarget, value: f32, end: TouchEnd) -> Result<(), ContractError>;
    /// The control of `target` was released. In `Touch` mode writing of that target stops; in `Latch` mode nothing
    /// changes. An untouched target is ignored.
    fn release(&self, target: &ParamTarget);
    /// Releases every target touched with [`TouchEnd::Release`] (the mouse button went up anywhere).
    fn release_all(&self);
}

/// The engine side of writing: which lanes it must not play now. Implemented by the audio engine; cheap, never blocks.
pub trait AutomationHold: Send + Sync {
    /// Replaces the set of held lanes (an empty slice releases all). Only the first `MAX_HELD_LANES` count.
    fn hold(&self, lanes: &[LaneId]);
}

/// How many lanes can be held at once.
pub const MAX_HELD_LANES: usize = 64;
