//! Undo and redo of project edits, edit groups and checkpoints (`HistoryControl`).

use crate::ContractError;

/// A point in the undo history returned by `HistoryControl::checkpoint`, for `HistoryControl::squash`.
pub type HistoryMark = u64;

/// Undo and redo of project edits.
///
/// **Groups.** Everything that changed the project between `begin_group` and the matching `end_group` becomes ONE undo
/// step (recording a take, building a part, a knob gesture in a plugin window). The group is shared by the project:
/// edits from any source made while it is open join it. Groups nest: inner `begin_group`/`end_group` only join the
/// outer one (the label is the outer one's), and the step is recorded when the outermost one closes.
/// `cancel_group` reverts the whole group (all levels); after it the remaining `end_group` calls do nothing.
///
/// **Checkpoints.** `checkpoint` + `squash` turn the steps made after a point into one step AFTER the fact: for a change
/// made of many separate calls (an AI request over MCP). Why not a group: a group held open between calls blocks the
/// user's undo, and a compound call failing midway cancels the whole group — everything the caller did before it.
/// A checkpoint holds nothing open: until `squash` the steps are ordinary ones.
pub trait HistoryControl: Send + Sync {
    /// Steps back; `false` means there is nothing to undo or a group is open (close or cancel it first).
    fn undo(&self) -> bool;
    /// Steps forward; `false` means there is nothing to redo or a group is open.
    fn redo(&self) -> bool;
    fn can_undo(&self) -> bool;
    fn can_redo(&self) -> bool;
    /// Label of the next undo action.
    fn undo_label(&self) -> Option<String>;
    /// Opens an edit group (or enters a nested one, see the trait docs).
    fn begin_group(&self, label: &str);
    /// Closes a group level. `true` means the outermost group was closed and an undo step recorded (the project
    /// changed); `false` means a nested level was closed, the project did not change, or there is no group.
    fn end_group(&self) -> bool;
    /// Reverts the open group entirely: the project returns to its state at the outermost `begin_group` and no undo
    /// step is recorded. Needed when a compound operation fails midway. Without a group it does nothing.
    fn cancel_group(&self);
    /// Treats the current state as the baseline, as for a freshly opened project: undo and redo history is empty,
    /// there are no unsaved edits, and an open group is closed without an undo step. Needed after building a project
    /// programmatically.
    fn mark_baseline(&self);
    /// Marks the current point in the history for `squash`. The next edit starts a new step: it does not merge with a
    /// knob edit made before the mark.
    fn checkpoint(&self) -> HistoryMark;
    /// Turns every undo step recorded after `mark` (from any source, a group closed meanwhile included) into ONE step
    /// labelled `label`; one undo then returns the project to its state at the mark. The redo history stays.
    /// `Ok(false)`: nothing was recorded after the mark. Fails with an empty label, while a group is open, and when the
    /// mark's point has left the history: undone past it, a project opened or created, or forgotten by the step limit.
    fn squash(&self, mark: HistoryMark, label: &str) -> Result<bool, ContractError>;
}
