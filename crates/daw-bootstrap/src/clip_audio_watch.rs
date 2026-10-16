//! Watching the audio of tempo-fitted clips (the `daw-clip-audio-watch` thread, ~20 times a second): processed audio
//! is computed in the background, and a finished job does not publish the engine snapshot by itself. The thread notices
//! `ProcessedAudio::finished` growing and asks the core to publish the snapshot again. A tempo change is a regular edit
//! of the project tempo map: it publishes the snapshot (and queues a new job) itself. Lives as long as the core: holds
//! it through a weak reference.

use std::sync::{Arc, Weak};
use std::thread;
use std::time::Duration;

use daw_contracts::{AppLog, LogEntry, ProcessedAudio};
use daw_project::ProjectService;

const INTERVAL: Duration = Duration::from_millis(50);

pub(crate) fn spawn(service: Weak<ProjectService>, processed: Arc<dyn ProcessedAudio>, log: &dyn AppLog) {
    let spawned = thread::Builder::new().name("daw-clip-audio-watch".into()).spawn(move || {
        let mut seen = processed.finished();
        while let Some(service) = service.upgrade() {
            let finished = processed.finished();
            if finished != seen {
                seen = finished;
                service.refresh_clip_audio();
            }
            drop(service);
            thread::sleep(INTERVAL);
        }
    });
    if let Err(error) = spawned {
        let message = format!("could not start the clip audio watch: tempo fitting will not update ({error})");
        log.record(LogEntry::error("stretch", message));
    }
}
