//! The window's copy of the project (`ProjectFeed`): after an edit the window gets only what changed, without asking.
//! The feed remembers the project as it sent it last and, when the core has a new revision, sends the difference
//! (`ProjectChanges::between`) with the status. Why the feed keeps that copy and not the core: it is what one reader
//! has, and the core owns only the model. Why the whole project also comes through the feed (`WholeRequest`) and not as
//! a call's reply: one sender sends everything in order, so a change can never arrive after a newer whole project.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use daw_contracts::{Controls, Project, ProjectChanges};
use serde::Serialize;

use crate::project::{Status, status, without_plugin_states};

/// One update for the window. JSON: `{ whole, changes, status }`. `Clone` because a Tauri event payload must be.
#[derive(Clone, Serialize)]
pub struct ProjectUpdate {
    /// `changes` is the whole project: the window takes it as its copy. Otherwise they go on top of the update before.
    whole: bool,
    changes: ProjectChanges,
    status: Status,
}

/// Asks the feed for the whole project with its next update; a clone goes to whoever may ask (the window's command).
#[derive(Clone, Default)]
pub struct WholeRequest(Arc<AtomicBool>);

impl WholeRequest {
    pub fn ask(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

/// Keeps the window's copy of the project current. Owned by the thread that sends the window its events.
#[derive(Default)]
pub struct ProjectFeed {
    /// The project as the window got it last, without plugin states; `None` until the first update.
    sent: Option<Project>,
    /// The revision `sent` was read at: while it is the same, there is nothing to compare.
    revision: Option<u64>,
    whole: WholeRequest,
}

impl ProjectFeed {
    pub fn new() -> Self {
        Self::default()
    }

    /// Where the window asks for the whole project: when it starts and after a refused call it already showed.
    pub fn whole_request(&self) -> WholeRequest {
        self.whole.clone()
    }

    /// What to send the window now: `None` when the project has no new revision and nobody asked for it whole. A new
    /// revision with the same model (only the undo label changed) is an update with no changes and the new status.
    pub fn poll(&mut self, controls: &Controls) -> Option<ProjectUpdate> {
        let asked = self.whole.take();
        // The revision is read before the copy: an edit in between shows as a new revision in the next poll.
        let revision = controls.view.revision();
        if !asked && self.revision == Some(revision) {
            return None;
        }
        let project = without_plugin_states(controls.view.snapshot());
        let (whole, changes) = match self.sent.as_ref().filter(|_| !asked) {
            Some(sent) => (false, ProjectChanges::between(sent, &project)),
            None => (true, ProjectChanges::whole(&project)),
        };
        self.revision = Some(revision);
        self.sent = Some(project);
        Some(ProjectUpdate { whole, changes, status: status(controls) })
    }
}
