//! Native plugin windows: the contract "show the plugin's own interface".

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, NodeId, TrackId};

/// Whose interface to open: an effect sits in a graph node, an instrument on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EditorTarget {
    Effect { node: NodeId },
    Instrument { track: TrackId },
}

/// Id of an open plugin window session.
pub type EditorId = u32;

/// The window's parent view (on macOS a pointer to an `NSView`) the plugin inserts its interface into.
/// A number rather than a pointer so the contract does not pull in platform types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeView(pub usize);

/// What the window needs to know about a freshly created plugin interface. Sizes are in logical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSession {
    pub id: EditorId,
    /// Plugin name, for the window title.
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Whether the plugin allows resizing its window.
    pub resizable: bool,
}

/// Native plugin windows. Job: drive a plugin interface from creation to closing and tie it to the project.
///
/// Order: `open` → (the owner creates the window) `attach` → while the window is open, `sync` on a timer and `resize`
/// when the window size changes → `close`. All methods are called only from the app's main thread: plugin interfaces
/// require it.
///
/// Contract:
/// * the interface lives in a SEPARATE plugin instance, sound goes through the main one (in the engine); the window is
///   a "view onto the parameters";
/// * edits in the window reach the project through the same operations as the knobs (`GraphControl::set_effect_param`,
///   `TrackControl::set_instrument_param`), so they are heard, saved and undoable; one gesture (grab a knob, release
///   it) is one undo step;
/// * parameter changes from outside (automation, another panel) are picked up by the window on `sync`;
/// * for a built-in effect or instrument, and for a plugin without its own window, `open` returns a clear error
///   (`plugins::BUILTIN_EFFECT_HAS_NO_WINDOW`, `BUILTIN_INSTRUMENT_HAS_NO_WINDOW`, `NO_EDITOR`);
/// * the project does not know plugin parameters beyond the first `MAX_EXPOSED_PARAMS`: their edits in the window do
///   not reach the project; they, like a preset chosen in the window, are saved as part of the plugin state
///   (`PluginStates`);
/// * state: on opening, the window first receives the target's saved state (window and sound match). If the plugin
///   reports a change (parameters reloaded, a parameter unknown to the project edited), one second after the last
///   report (on `sync`) the window's state is written to the project (`PluginStates::set`; the sounding instance is
///   recreated). On window close and in `store_states` the state is written if it differs from what the window last
///   received or wrote. A target state changed from outside (undo) is picked up by the window on `sync`.
pub trait PluginEditors: Send + Sync {
    /// Creates the plugin interface for the target. One target, one window: a second `open` of the same target is
    /// `plugins::EDITOR_ALREADY_OPEN`.
    fn open(&self, target: EditorTarget) -> Result<EditorSession, ContractError>;
    /// Inserts the plugin interface into the window's parent view.
    fn attach(&self, id: EditorId, parent: NativeView) -> Result<(), ContractError>;
    /// The window was resized: tells the plugin the new size.
    fn resize(&self, id: EditorId, width: u32, height: u32) -> Result<(), ContractError>;
    /// Timer tick: pulls parameter values from the project into the window. `Some((width, height))` means the plugin
    /// asked to resize the window; an error means the target is gone from the project (time to close the window).
    fn sync(&self, id: EditorId) -> Result<Option<(u32, u32)>, ContractError>;
    /// Closes the interface and frees the plugin instance (a changed state goes to the project first).
    /// An unknown id is not an error (the window may be closed twice).
    fn close(&self, id: EditorId);
    /// Writes the changed state of all open windows to the project without waiting for a pause (before saving the
    /// project). Write failures go to the log: they do not cancel saving the project.
    fn store_states(&self);
}

/// The app's plugin windows as seen by whoever saves or renders the project. Job: hand the state of open plugin
/// windows to the project now, so a preset just picked in a window is in the saved file or the rendered sound
/// (windows write it on their own only after a second of quiet). Called from any thread but the app's main thread:
/// the windows' work runs there and the caller waits for it.
///
/// Contract: `flush` returns once every open window's changed state is in the project (`PluginEditors::store_states`,
/// run on the main thread); with no open windows it returns at once. Failures go to the log and do not stop the
/// caller. It must not be called while holding the project lock (the windows edit the project). Autosave does not
/// flush: it runs every several seconds, and the windows catch up within a second anyway.
pub trait PluginStateFlush: Send + Sync {
    fn flush(&self);
}
