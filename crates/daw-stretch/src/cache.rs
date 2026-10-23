//! `StretchCache` (`ProcessedAudio`): "keep the processed audio of clips ready". Finished audio is stored per job
//! (`ProcessJob`); missing audio is computed one job at a time by the `daw-stretch` background thread. What the project
//! no longer needs is dropped in `retain`; a job that stopped being needed while queued is not computed at all (so
//! quick tempo changes do not pile up work).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use daw_contracts::codes::{clips, samples};
use daw_contracts::sync::lock;
use daw_contracts::{AppLog, ContractError, LogEntry, ProcessJob, ProcessedAudio, SampleData, SampleStore};

#[derive(Default)]
struct State {
    ready: HashMap<ProcessJob, Arc<SampleData>>,
    /// Waiting for the background thread.
    queued: HashSet<ProcessJob>,
    /// Failed: not retried while the job is wanted (otherwise the log would fill up with one error).
    failed: HashSet<ProcessJob>,
    /// What the project needs, by the last `retain` and the requests since.
    wanted: HashSet<ProcessJob>,
}

struct Shared {
    state: Mutex<State>,
    samples: Arc<dyn SampleStore>,
    log: Arc<dyn AppLog>,
    finished: AtomicU64,
}

impl Shared {
    fn compute_now(&self, job: &ProcessJob) -> Result<Arc<SampleData>, ContractError> {
        let data = self.samples.get(job.sample).ok_or_else(|| samples::NOT_FOUND.with("id", job.sample))?;
        crate::process_job(&data, job).map(Arc::new)
    }

    fn fail(&self, job: &ProcessJob, error: &ContractError) {
        let curve = if job.curve().is_some() { " on average, along the tempo curve" } else { "" };
        let message = format!(
            "audio of sample {} was not processed (stretch {:.4}{curve}, pitch {:+.2}): {error}",
            job.sample,
            job.stretch(),
            job.semitones()
        );
        self.log.record(LogEntry::error("stretch", message));
    }

    /// The background thread: computes jobs one by one while `StretchCache` (its `Sender`) is alive.
    fn work(&self, jobs: &Receiver<ProcessJob>) {
        while let Ok(job) = jobs.recv() {
            {
                let mut state = lock(&self.state);
                if !state.wanted.contains(&job) {
                    state.queued.remove(&job);
                    continue;
                }
            }
            let result = self.compute_now(&job);
            let mut state = lock(&self.state);
            state.queued.remove(&job);
            match result {
                Ok(data) => {
                    if state.wanted.contains(&job) {
                        state.ready.insert(job, data);
                    }
                }
                Err(error) => {
                    state.failed.insert(job.clone());
                    drop(state);
                    self.fail(&job, &error);
                }
            }
            self.finished.fetch_add(1, Ordering::Release);
        }
    }
}

pub struct StretchCache {
    shared: Arc<Shared>,
    jobs: Sender<ProcessJob>,
}

impl StretchCache {
    /// Starts the background thread. If it fails to start, that is logged; clips with processing are then silent (their
    /// jobs fail at once), while the export (`compute`) still works.
    pub fn new(samples: Arc<dyn SampleStore>, log: Arc<dyn AppLog>) -> Self {
        let shared = Arc::new(Shared { state: Mutex::new(State::default()), samples, log, finished: AtomicU64::new(0) });
        let (jobs, receiver) = mpsc::channel();
        let worker = Arc::clone(&shared);
        if let Err(error) = thread::Builder::new().name("daw-stretch".into()).spawn(move || worker.work(&receiver)) {
            shared.log.record(LogEntry::error("stretch", format!("could not start the audio processing thread: {error}")));
        }
        Self { shared, jobs }
    }
}

impl ProcessedAudio for StretchCache {
    fn ready(&self, job: &ProcessJob) -> Option<Arc<SampleData>> {
        let mut state = lock(&self.shared.state);
        state.wanted.insert(job.clone());
        if let Some(data) = state.ready.get(job) {
            return Some(Arc::clone(data));
        }
        if state.queued.contains(job) || state.failed.contains(job) {
            return None;
        }
        if self.jobs.send(job.clone()).is_ok() {
            state.queued.insert(job.clone());
        } else {
            state.failed.insert(job.clone());
            drop(state);
            self.shared.fail(job, &clips::STRETCH_WORKER_DOWN.err());
        }
        None
    }

    fn compute(&self, job: &ProcessJob) -> Result<Arc<SampleData>, ContractError> {
        if let Some(data) = lock(&self.shared.state).ready.get(job) {
            return Ok(Arc::clone(data));
        }
        let data = self.shared.compute_now(job)?;
        lock(&self.shared.state).ready.insert(job.clone(), Arc::clone(&data));
        Ok(data)
    }

    fn finished(&self) -> u64 {
        self.shared.finished.load(Ordering::Acquire)
    }

    fn retain(&self, keep: &[ProcessJob]) {
        let mut state = lock(&self.shared.state);
        state.wanted = keep.iter().cloned().collect();
        let State { ready, failed, wanted, .. } = &mut *state;
        ready.retain(|job, _| wanted.contains(job));
        failed.retain(|job| wanted.contains(job));
    }
}
