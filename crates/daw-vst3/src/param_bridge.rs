//! `ParamBridge`: "link the knobs in a plugin window to the project" — pure logic without VST3 pointers.
//!
//! The plugin window is a separate instance, so values have to travel both ways:
//! * from the window to the project (`begin`/`perform`/`end`): we ask the project to write the value with the same operations
//!   as the knobs, so it is heard, saved and undoable. At most ~30 writes per second per parameter; the last value
//!   is not lost (`flush` or `end` writes it). A gesture from `begin` to `end` is one undo step (a history group);
//! * from the project to the window (`pull`): automation and other panels change the project — the window picks up new values,
//!   but does not touch a knob being held right now (otherwise it would "run away" from under the finger).
//!
//! The window (the plugin's controller) speaks in normalized values 0..1, while the project keeps real ones (hertz, choice
//! index): at the project boundary values are converted by the window instance's `ParamScales`.
//!
//! The project knows only the first `MAX_EXPOSED_PARAMS` plugin parameters; edits of the rest do not go to the project.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use daw_contracts::{AppLog, ContractError, HistoryControl, L10n, Params};

use crate::journal;
use crate::param_name;
use crate::scale::ParamScales;

/// At least this much time passes between two writes of one parameter to the project (~30 times per second).
pub const MIN_WRITE_INTERVAL: Duration = Duration::from_millis(33);
/// A smaller value difference is not a change (the project keeps `f32`, the plugin `f64`).
const SAME: f64 = 1e-6;
const GROUP_LABEL: L10n = L10n { en: "Plugin parameter", ru: "Параметр плагина" };

/// Params of one target (an effect or an instrument) in the project.
pub trait ProjectParams: Send + Sync {
    /// Current values (`p<id>` -> real value); a missing name means the default. An error means the target is gone from the project.
    fn read(&self) -> Result<Params, ContractError>;
    fn write(&self, name: &str, value: f32) -> Result<(), ContractError>;
}

pub struct ParamBridge {
    project: Arc<dyn ProjectParams>,
    history: Arc<dyn HistoryControl>,
    log: Arc<dyn AppLog>,
    plugin: String,
    /// Parameters the project knows: id -> normalized default value.
    exposed: HashMap<u32, f64>,
    /// Conversion "normalized (window) <-> real (project)".
    scales: ParamScales,
    /// What the window shows now (what the window's controller knows).
    shown: HashMap<u32, f64>,
    /// Values not yet written to the project because of the rate limit.
    pending: HashMap<u32, f64>,
    last_write: HashMap<u32, Instant>,
    /// Knobs being held (between `begin` and `end`).
    editing: HashSet<u32>,
    group_open: bool,
    write_failed: bool,
    unknown_noted: bool,
}

impl ParamBridge {
    pub fn new(
        project: Arc<dyn ProjectParams>,
        history: Arc<dyn HistoryControl>,
        log: Arc<dyn AppLog>,
        plugin: String,
        exposed: HashMap<u32, f64>,
        scales: ParamScales,
    ) -> Self {
        Self {
            project,
            history,
            log,
            plugin,
            exposed,
            scales,
            shown: HashMap::new(),
            pending: HashMap::new(),
            last_write: HashMap::new(),
            editing: HashSet::new(),
            group_open: false,
            write_failed: false,
            unknown_noted: false,
        }
    }

    /// Whether the project knows parameter `id` (edits of the others live only in the plugin state).
    pub fn knows(&self, id: u32) -> bool {
        self.exposed.contains_key(&id)
    }

    /// The window got a whole new state: what it shows is unknown, so the next `pull` returns all project values.
    pub fn forget_shown(&mut self) {
        self.shown.clear();
    }

    /// Start of a gesture: the first knob opens a history group.
    pub fn begin(&mut self, id: u32) {
        if self.editing.is_empty() && !self.group_open {
            self.history.begin_group(GROUP_LABEL.get());
            self.group_open = true;
        }
        self.editing.insert(id);
    }

    /// A new knob value from the window (0..1).
    pub fn perform(&mut self, id: u32, value: f64, now: Instant) {
        if !value.is_finite() {
            return;
        }
        if !self.exposed.contains_key(&id) {
            self.note_unknown(id);
            return;
        }
        let value = value.clamp(0.0, 1.0);
        if !self.editing.contains(&id) && self.shown.get(&id).is_some_and(|shown| (shown - value).abs() < SAME) {
            return;
        }
        self.shown.insert(id, value);
        let due = self.last_write.get(&id).is_none_or(|at| now.duration_since(*at) >= MIN_WRITE_INTERVAL);
        if due {
            self.write(id, value, now);
        } else {
            self.pending.insert(id, value);
        }
    }

    /// End of a gesture: writes the knob's last value; when all knobs are released, the group closes.
    pub fn end(&mut self, id: u32, now: Instant) {
        if let Some(value) = self.pending.remove(&id) {
            self.write(id, value, now);
        }
        self.editing.remove(&id);
        if self.editing.is_empty() {
            self.close_group();
        }
    }

    /// Writes the held-back values whose interval has passed (called on a timer).
    pub fn flush(&mut self, now: Instant) {
        let due: Vec<(u32, f64)> = self
            .pending
            .iter()
            .filter(|(id, _)| self.last_write.get(id).is_none_or(|at| now.duration_since(*at) >= MIN_WRITE_INTERVAL))
            .map(|(id, value)| (*id, *value))
            .collect();
        for (id, value) in due {
            self.pending.remove(&id);
            self.write(id, value, now);
        }
    }

    /// Values the window should show because they differ in the project. Remembers them as shown.
    /// Knobs in a gesture and values not yet in the project are skipped. An error means the target is gone from the project.
    pub fn pull(&mut self) -> Result<Vec<(u32, f64)>, ContractError> {
        let values = self.project.read()?;
        let mut changes = Vec::new();
        for (&id, &default) in &self.exposed {
            if self.editing.contains(&id) || self.pending.contains_key(&id) {
                continue;
            }
            let wanted = values.get(&param_name::name(id)).map_or(default, |v| self.scales.to_normalized(id, f64::from(*v)));
            if self.shown.get(&id).is_none_or(|shown| (shown - wanted).abs() >= SAME) {
                self.shown.insert(id, wanted);
                changes.push((id, wanted));
            }
        }
        changes.sort_by_key(|(id, _)| *id);
        Ok(changes)
    }

    /// The window is closing: everything held back is written, an open group is closed (the plugin may not have sent `endEdit`).
    pub fn finish(&mut self, now: Instant) {
        let rest: Vec<(u32, f64)> = self.pending.drain().collect();
        for (id, value) in rest {
            self.write(id, value, now);
        }
        self.editing.clear();
        self.close_group();
    }

    fn close_group(&mut self) {
        if self.group_open {
            self.history.end_group();
            self.group_open = false;
        }
    }

    fn write(&mut self, id: u32, value: f64, now: Instant) {
        self.last_write.insert(id, now);
        let plain = self.scales.to_plain(id, value) as f32;
        if let Err(error) = self.project.write(&param_name::name(id), plain) {
            if !self.write_failed {
                self.write_failed = true;
                journal::error(
                    &*self.log,
                    format!("plugin window \"{}\": could not write a parameter to the project: {error}", self.plugin),
                );
            }
        }
    }

    fn note_unknown(&mut self, id: u32) {
        if !self.unknown_noted {
            self.unknown_noted = true;
            journal::warn(
                &*self.log,
                format!(
                    "plugin window \"{}\": the project does not know parameter {id} (only the first plugin parameters are exposed), its edits are not heard or saved",
                    self.plugin
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ParamScale;
    use crate::test_fakes::{History, Log, Project};
    use daw_contracts::LogLevel;

    struct Rig {
        bridge: ParamBridge,
        project: Arc<Project>,
        history: Arc<History>,
        log: Arc<Log>,
    }

    /// Parameters 7 (default 0.25) and 9 (default 0.5), project values as the plugin's (0..1).
    fn rig() -> Rig {
        rig_with(ParamScales::default())
    }

    fn rig_with(scales: ParamScales) -> Rig {
        let (project, history, log) = (Arc::new(Project::default()), Arc::new(History::default()), Arc::new(Log::default()));
        let exposed = HashMap::from([(7, 0.25), (9, 0.5)]);
        let bridge = ParamBridge::new(project.clone(), history.clone(), log.clone(), "Test".into(), exposed, scales);
        Rig { bridge, project, history, log }
    }

    #[test]
    fn project_gets_plain_values_and_the_window_normalized_ones() {
        let scales = ParamScales::new(vec![(7, ParamScale::continuous(|n| 1000.0 * n)), (9, ParamScale::indexed(2))]);
        let mut rig = rig_with(scales);
        rig.bridge.perform(7, 0.4, Instant::now());
        rig.bridge.perform(9, 1.0, Instant::now());
        assert_eq!(*rig.project.writes.lock().unwrap(), [("p7".to_string(), 400.0f32), ("p9".to_string(), 2.0)]);
        rig.project.values.lock().unwrap().insert("p7".into(), 750.0);
        rig.project.values.lock().unwrap().insert("p9".into(), 1.0);
        let changes = rig.bridge.pull().unwrap();
        assert_eq!(changes, [(7, 0.75), (9, 0.5)]);
    }

    fn at(ms: u64, base: Instant) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn first_pull_shows_defaults_and_project_values() {
        let mut rig = rig();
        rig.project.values.lock().unwrap().insert("p9".into(), 0.8);
        let changes = rig.bridge.pull().unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0], (7, 0.25));
        assert!((changes[1].1 - 0.8).abs() < 1e-6);
        assert!(rig.bridge.pull().unwrap().is_empty(), "nothing changed");
        rig.project.values.lock().unwrap().insert("p7".into(), 0.6);
        assert_eq!(rig.bridge.pull().unwrap().len(), 1, "only what changed outside");
    }

    #[test]
    fn gone_target_is_an_error() {
        let mut rig = rig();
        *rig.project.gone.lock().unwrap() = true;
        assert!(rig.bridge.pull().is_err());
    }

    #[test]
    fn writes_are_throttled_and_the_last_value_is_not_lost() {
        let mut rig = rig();
        let t = Instant::now();
        rig.bridge.begin(7);
        rig.bridge.perform(7, 0.30, t);
        rig.bridge.perform(7, 0.31, at(5, t));
        rig.bridge.perform(7, 0.32, at(10, t));
        assert_eq!(rig.project.writes.lock().unwrap().len(), 1, "the rest are held back");
        rig.bridge.flush(at(20, t));
        assert_eq!(rig.project.writes.lock().unwrap().len(), 1, "the interval has not passed yet");
        rig.bridge.flush(at(40, t));
        assert_eq!(rig.project.writes.lock().unwrap().last().unwrap().1, 0.32);
        rig.bridge.perform(7, 0.50, at(45, t));
        rig.bridge.end(7, at(46, t));
        let writes = rig.project.writes.lock().unwrap();
        assert_eq!(writes.last().unwrap().1, 0.5, "the end of a gesture writes the last value");
        assert_eq!(writes.len(), 3);
    }

    #[test]
    fn one_gesture_is_one_history_group() {
        let mut rig = rig();
        let t = Instant::now();
        rig.bridge.begin(7);
        rig.bridge.begin(9);
        rig.bridge.perform(7, 0.1, t);
        rig.bridge.end(7, at(1, t));
        assert_eq!(*rig.history.0.lock().unwrap(), ["begin Plugin parameter"], "the second knob is still held");
        rig.bridge.end(9, at(2, t));
        assert_eq!(*rig.history.0.lock().unwrap(), ["begin Plugin parameter", "end"]);
    }

    #[test]
    fn held_knob_and_unwritten_values_are_not_overwritten_by_pull() {
        let mut rig = rig();
        let t = Instant::now();
        rig.bridge.pull().unwrap();
        rig.bridge.begin(7);
        rig.bridge.perform(7, 0.9, t);
        rig.bridge.perform(7, 0.95, at(1, t));
        assert!(rig.bridge.pull().unwrap().is_empty(), "the knob is held, the project has the old value");
        rig.bridge.end(7, at(2, t));
        assert!(rig.bridge.pull().unwrap().is_empty(), "the project already got the window's value — no echo");
    }

    #[test]
    fn echo_of_a_value_we_set_ourselves_is_ignored() {
        let mut rig = rig();
        rig.bridge.pull().unwrap();
        rig.bridge.perform(7, 0.25, Instant::now());
        assert!(rig.project.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn unknown_parameter_is_noted_once_and_not_written() {
        let mut rig = rig();
        let t = Instant::now();
        rig.bridge.perform(500, 0.5, t);
        rig.bridge.perform(501, 0.5, t);
        assert!(rig.project.writes.lock().unwrap().is_empty());
        let entries = rig.log.recent(10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Warn);
    }

    #[test]
    fn finish_writes_the_rest_and_closes_a_dangling_group() {
        let mut rig = rig();
        let t = Instant::now();
        rig.bridge.begin(7);
        rig.bridge.perform(7, 0.3, t);
        rig.bridge.perform(7, 0.4, at(1, t));
        rig.bridge.finish(at(2, t));
        assert_eq!(rig.project.writes.lock().unwrap().last().unwrap().1, 0.4);
        assert_eq!(rig.history.0.lock().unwrap().last().unwrap(), "end");
    }

    #[test]
    fn non_finite_values_are_ignored() {
        let mut rig = rig();
        rig.bridge.perform(7, f64::NAN, Instant::now());
        assert!(rig.project.writes.lock().unwrap().is_empty());
    }
}
