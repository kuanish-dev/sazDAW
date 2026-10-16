//! `StateSync`: "when to write the plugin window state to the project and when to take it from the project" — pure logic without VST3.
//!
//! The window is a separate plugin instance, and that is where the user picks a preset. The sounding instance gets
//! state only by being recreated (`PluginStates::set`), so it cannot be written on every knob move (the sound would
//! break up): it is written `SETTLE` after the plugin's last change message (parameter reload, an edit of a parameter
//! the project does not know), and on window close and before saving the project — if the window state differs
//! from the last agreed one. Knobs the project knows go to the project as params anyway.

use std::sync::Arc;
use std::time::{Duration, Instant};

use daw_contracts::PluginState;

/// Silence after the plugin's last message after which the window state is written to the project.
pub const SETTLE: Duration = Duration::from_secs(1);

#[derive(Default)]
pub struct StateSync {
    /// The target's state in the project that the window knows of (got on open or undo, or wrote itself).
    project: Option<Arc<PluginState>>,
    /// The window bytes at that moment: if the window state matches them, there is nothing to write.
    window: Option<PluginState>,
    /// The plugin's last change message not yet written.
    changed_at: Option<Instant>,
}

impl StateSync {
    /// The plugin reported that its state changed.
    pub fn changed(&mut self, now: Instant) {
        self.changed_at = Some(now);
    }

    /// Whether it is time to write the window state: `SETTLE` of silence has passed since the message.
    pub fn due(&self, now: Instant) -> bool {
        self.changed_at.is_some_and(|at| now.duration_since(at) >= SETTLE)
    }

    /// The project state changed not through this window (undo, redo): the window should load the project state.
    pub fn outside(&self, project: Option<&Arc<PluginState>>) -> bool {
        project != self.project.as_ref()
    }

    /// Whether the window state `captured` differs from the last agreed one (only then is it worth writing).
    pub fn differs(&self, captured: &PluginState) -> bool {
        self.window.as_ref() != Some(captured)
    }

    /// Window and project agree: the project keeps `project`, the window has bytes `window`. Messages up to now are accounted for.
    pub fn agreed(&mut self, project: Option<Arc<PluginState>>, window: Option<PluginState>) {
        self.project = project;
        self.window = window;
        self.changed_at = None;
    }

    /// The message is accounted for without a write (nothing to write, or the write failed — it is already logged).
    pub fn settle(&mut self) {
        self.changed_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(bytes: &[u8]) -> PluginState {
        PluginState { component: bytes.to_vec(), controller: Vec::new() }
    }

    #[test]
    fn a_change_is_written_after_a_second_of_silence() {
        let mut sync = StateSync::default();
        let t = Instant::now();
        assert!(!sync.due(t), "nothing to write without messages");
        sync.changed(t);
        sync.changed(t + Duration::from_millis(600));
        assert!(!sync.due(t + Duration::from_millis(1200)), "the countdown starts from the last message");
        assert!(sync.due(t + Duration::from_millis(1700)));
        sync.settle();
        assert!(!sync.due(t + Duration::from_secs(5)));
    }

    #[test]
    fn only_a_different_window_state_is_worth_writing() {
        let mut sync = StateSync::default();
        assert!(sync.differs(&state(b"a")), "the window has not agreed with anything yet");
        sync.agreed(None, Some(state(b"a")));
        assert!(!sync.differs(&state(b"a")), "opened and closed the window: the project is not touched");
        assert!(sync.differs(&state(b"b")));
    }

    #[test]
    fn project_state_changed_elsewhere_is_noticed() {
        let mut sync = StateSync::default();
        let first = Arc::new(state(b"1"));
        sync.agreed(Some(first.clone()), Some(state(b"window")));
        assert!(!sync.outside(Some(&first)));
        assert!(!sync.outside(Some(&Arc::new(state(b"1")))), "contents are compared, not pointers");
        assert!(sync.outside(None), "an undo removed the state");
        assert!(sync.outside(Some(&Arc::new(state(b"2")))));
    }

    #[test]
    fn agreement_forgets_pending_messages() {
        let mut sync = StateSync::default();
        let t = Instant::now();
        sync.changed(t);
        sync.agreed(None, None);
        assert!(!sync.due(t + SETTLE), "messages that came while loading the state are already accounted for");
    }
}
