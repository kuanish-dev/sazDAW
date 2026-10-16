//! `LiveMeter` (`MasterMeter`): "measure the master as it plays". While watched it keeps the engine's `MasterTap` open
//! and a thread (`daw-master-meter`) moves the new frames into the measurement (`measure.rs`) about 20 times a second, so
//! every frame is measured however rarely the interface reads; a reading takes the newest frames first. Playback that
//! starts from the song start (the tap's restart mark) starts the integration over at that very frame.

mod measure;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;

use daw_contracts::sync::lock;
use daw_contracts::{AppLog, LogEntry, MasterFrames, MasterMeter, MasterReading, MasterTap};

use measure::{Measurement, nothing_yet};

/// How often the meter's thread reads the tap: far within what the tap holds (`MASTER_TAP_FRAMES`).
const FEED_INTERVAL: Duration = Duration::from_millis(50);

pub struct LiveMeter {
    inner: Arc<Inner>,
}

struct Inner {
    tap: Arc<dyn MasterTap>,
    log: Arc<dyn AppLog>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The current watch, `None` while off. A thread of an older watch ends when it sees another one.
    watch: Option<u64>,
    next_watch: u64,
    /// The end of the frames read from the tap in this watch.
    end: u64,
    /// The tap's restart mark already applied.
    restart: Option<u64>,
    /// `None` until the first frames tell the sample rate.
    measurement: Option<Measurement>,
}

impl LiveMeter {
    pub fn new(tap: Arc<dyn MasterTap>, log: Arc<dyn AppLog>) -> Self {
        Self { inner: Arc::new(Inner { tap, log, state: Mutex::new(State::default()) }) }
    }

    fn spawn_feeder(&self, watch: u64) {
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        let spawned = thread::Builder::new().name("daw-master-meter".into()).spawn(move || {
            // Only a weak reference between reads: the meter going away ends the thread.
            while let Some(inner) = weak.upgrade() {
                let mut state = lock(&inner.state);
                if state.watch != Some(watch) {
                    break;
                }
                state.feed(&*inner.tap, &*inner.log);
                drop(state);
                drop(inner);
                thread::sleep(FEED_INTERVAL);
            }
        });
        if let Err(error) = spawned {
            let message = format!("could not start the master meter's thread: it measures only while it is read ({error})");
            self.inner.log.record(LogEntry::error("meter", message));
        }
    }
}

impl MasterMeter for LiveMeter {
    fn watch(&self, on: bool) {
        let mut state = lock(&self.inner.state);
        if on == state.watch.is_some() {
            return;
        }
        let watch = state.next_watch;
        *state = State { watch: on.then_some(watch), next_watch: watch + 1, ..State::default() };
        // Under the state lock: a reading never sees the new watch before the tap is open (or after it is closed).
        self.inner.tap.open_tap(on);
        drop(state);
        if on {
            self.spawn_feeder(watch);
        }
    }

    fn reset(&self) {
        let mut state = lock(&self.inner.state);
        if state.watch.is_none() {
            return;
        }
        // What played before the click belongs before the reset.
        state.feed(&*self.inner.tap, &*self.inner.log);
        if let Some(measurement) = &mut state.measurement {
            measurement.restart();
        }
    }

    fn reading(&self) -> Option<MasterReading> {
        let mut state = lock(&self.inner.state);
        state.watch?;
        state.feed(&*self.inner.tap, &*self.inner.log);
        Some(state.measurement.as_ref().map_or_else(|| nothing_yet(0), Measurement::reading))
    }
}

impl State {
    /// Measures what the tap captured since the previous call.
    fn feed(&mut self, tap: &dyn MasterTap, log: &dyn AppLog) {
        let frames = tap.frames_after(self.end);
        if frames.sample_rate == 0 {
            return;
        }
        if frames.start > self.end {
            let seconds = (frames.start - self.end) as f64 / f64::from(frames.sample_rate);
            let message = format!(
                "the live master meter missed {seconds:.1} s of the master (it was not read for too long): the integrated loudness and the loudness range leave it out"
            );
            log.record(LogEntry::warn("meter", message));
        }
        // A new stream rate needs new filters: the measurement starts over.
        if self.measurement.as_ref().is_none_or(|m| m.sample_rate() != frames.sample_rate) {
            self.measurement = Some(Measurement::new(frames.sample_rate));
        }
        self.measure(&frames);
    }

    fn measure(&mut self, frames: &MasterFrames) {
        let Some(measurement) = &mut self.measurement else { return };
        let count = (frames.samples.len() / 2) as u64;
        let end = frames.start + count;
        // A restart mark not applied yet and already reached: the frames before it, the restart, the frames from it. A
        // mark ahead (the song start still on its way through the latency compensation) waits for its frame.
        if let Some(at) = frames.restart.filter(|&at| self.restart != Some(at) && at <= end) {
            let split = 2 * (at.max(frames.start) - frames.start) as usize;
            measurement.push(&frames.samples[..split]);
            measurement.restart();
            measurement.push(&frames.samples[split..]);
            self.restart = Some(at);
        } else {
            measurement.push(&frames.samples);
        }
        self.end = end;
    }
}

impl Drop for Inner {
    /// A meter going away while watched closes the tap, so the engine stops copying for nobody.
    fn drop(&mut self) {
        if lock(&self.state).watch.is_some() {
            self.tap.open_tap(false);
        }
    }
}
