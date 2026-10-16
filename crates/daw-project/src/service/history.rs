//! Undo history: a stack of full project states, edit groups and checkpoints (the core's `HistoryControl`).

use std::collections::VecDeque;

use daw_contracts::codes::history;
use daw_contracts::{ContractError, HistoryControl, HistoryMark, HistorySteps, Project};

use super::ProjectService;

/// How many undo steps are kept; the oldest are forgotten.
const LIMIT: usize = 200;

struct Entry {
    /// Unique for the whole session: a `HistoryMark` names the step that was on top of the stack.
    seq: u64,
    label: String,
    project: Project,
}

#[derive(Default)]
pub(super) struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    /// The last number given out to a step or to `root`; steps are numbered from 1.
    last_seq: u64,
    /// The mark of the bottom of the undo stack (the state before its oldest step). It changes when that state is
    /// lost (history cleared, the oldest step forgotten), so marks of lost points stop matching anything.
    root: HistoryMark,
}

/// An open edit group: its label, the state before the first edit and the nesting depth.
pub(super) struct Group {
    label: String,
    before: Project,
    depth: usize,
}

impl History {
    /// Remembers the state before an edit; redo is cleared.
    pub(super) fn record(&mut self, label: &str, before: Project) {
        self.last_seq += 1;
        self.undo.push_back(Entry { seq: self.last_seq, label: label.to_string(), project: before });
        if self.undo.len() > LIMIT
            && let Some(oldest) = self.undo.pop_front()
        {
            // The state before the oldest step is gone; the bottom is now the state after it.
            self.root = oldest.seq;
        }
        self.redo.clear();
    }

    pub(super) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_seq += 1;
        self.root = self.last_seq;
    }

    /// The point the project is at: the step on top of the stack, or the bottom.
    fn mark(&self) -> HistoryMark {
        self.undo.back().map_or(self.root, |entry| entry.seq)
    }

    /// Merges the steps above `mark` into one labelled `label`; `None` means the mark's point has left the history.
    fn squash(&mut self, mark: HistoryMark, label: &str) -> Option<bool> {
        let from = if mark == self.root { 0 } else { self.undo.iter().position(|entry| entry.seq == mark)? + 1 };
        let mut after = self.undo.split_off(from);
        let Some(first) = after.pop_front() else { return Some(false) };
        // The merged step keeps the newest number: a mark taken at the end of the change still names the top.
        let seq = after.back().map_or(first.seq, |last| last.seq);
        self.undo.push_back(Entry { seq, label: label.to_string(), project: first.project });
        Some(true)
    }

    /// One step back: returns the state to restore.
    fn undo(&mut self, current: Project) -> Option<Project> {
        let entry = self.undo.pop_back()?;
        self.redo.push(Entry { seq: entry.seq, label: entry.label, project: current });
        Some(entry.project)
    }

    fn redo(&mut self, current: Project) -> Option<Project> {
        let entry = self.redo.pop()?;
        self.undo.push_back(Entry { seq: entry.seq, label: entry.label, project: current });
        Some(entry.project)
    }
}

impl ProjectService {
    fn travel(&self, step: impl FnOnce(&mut History, Project) -> Option<Project>) -> bool {
        let mut inner = self.lock();
        // A history step in the middle of a group would break it: the group remembers a state that is gone.
        if inner.group.is_some() {
            return false;
        }
        let current = inner.project.clone();
        let next_id = current.next_id;
        let Some(mut restored) = step(&mut inner.history, current) else {
            return false;
        };
        // Ids are never reused, even when the creation of an entity is undone.
        restored.next_id = restored.next_id.max(next_id);
        inner.project = restored;
        inner.dirty = true;
        inner.last_edit = None;
        self.sync(&mut inner);
        true
    }
}

impl HistoryControl for ProjectService {
    fn undo(&self) -> bool {
        self.travel(History::undo)
    }

    fn redo(&self) -> bool {
        self.travel(History::redo)
    }

    fn can_undo(&self) -> bool {
        !self.lock().history.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.lock().history.redo.is_empty()
    }

    fn undo_label(&self) -> Option<String> {
        self.lock().history.undo.back().map(|entry| entry.label.clone())
    }

    fn steps(&self) -> HistorySteps {
        let inner = self.lock();
        let label = |entry: &Entry| entry.label.clone();
        HistorySteps {
            done: inner.history.undo.iter().map(label).collect(),
            // Redo pops from the end of its stack: the next redo is the last entry.
            undone: inner.history.redo.iter().rev().map(label).collect(),
        }
    }

    fn begin_group(&self, label: &str) {
        let mut inner = self.lock();
        match &mut inner.group {
            Some(group) => group.depth += 1,
            None => inner.group = Some(Group { label: label.to_string(), before: inner.project.clone(), depth: 1 }),
        }
    }

    fn end_group(&self) -> bool {
        let mut inner = self.lock();
        let Some(group) = &mut inner.group else { return false };
        group.depth -= 1;
        if group.depth > 0 {
            return false;
        }
        let Some(Group { label, before, .. }) = inner.group.take() else { return false };
        if inner.project == before {
            return false;
        }
        inner.history.record(&label, before);
        // The next knob edit must not merge with the group into one step.
        inner.last_edit = None;
        true
    }

    fn cancel_group(&self) {
        let mut inner = self.lock();
        let Some(group) = inner.group.take() else { return };
        if inner.project != group.before {
            let next_id = inner.project.next_id;
            inner.project = group.before;
            // As with undo: ids given out inside the group are never given out again.
            inner.project.next_id = inner.project.next_id.max(next_id);
            inner.last_edit = None;
            self.sync(&mut inner);
        }
    }

    fn mark_baseline(&self) {
        let mut inner = self.lock();
        inner.history.clear();
        inner.group = None;
        inner.dirty = false;
        inner.last_edit = None;
    }

    fn checkpoint(&self) -> HistoryMark {
        let mut inner = self.lock();
        // A knob edit right after the mark must start a new step, not continue one made before it.
        inner.last_edit = None;
        inner.history.mark()
    }

    fn squash(&self, mark: HistoryMark, label: &str) -> Result<bool, ContractError> {
        let label = label.trim();
        if label.is_empty() {
            return Err(history::STEP_LABEL_EMPTY.err());
        }
        let mut inner = self.lock();
        // Edits inside an open group are not steps yet: the merged step would miss them.
        if inner.group.is_some() {
            return Err(history::GROUP_OPEN.err());
        }
        let squashed = inner.history.squash(mark, label).ok_or_else(|| history::CHECKPOINT_GONE.with("mark", mark))?;
        if squashed {
            inner.last_edit = None;
            // The model is the same, but the undo label changed: a new revision makes the interface re-read it.
            self.publish_snapshot(&mut inner);
        }
        Ok(squashed)
    }
}
