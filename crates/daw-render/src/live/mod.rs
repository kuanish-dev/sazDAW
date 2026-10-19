//! `LiveMeter` (`MasterMeter`): "measure the master as it plays". A reading starts the measuring and keeps it going for
//! `CAPTURE_LEASE_SECONDS`: meanwhile the engine's `MasterTap` stays open and a thread (`daw-master-meter`) moves the new
//! frames into the measurement (`measure.rs`) about 20 times a second, so every frame is measured however rarely the
//! interface reads; a reading takes the newest frames first. Once nobody has read for a whole lease, the thread closes
//! the tap and ends. Playback that starts from the song start (the tap's restart mark) starts the integration over at
//! that very frame.

mod measure;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use daw_contracts::sync::lock;
use daw_contracts::{AppLog, CAPTURE_LEASE_SECONDS, LogEntry, MasterFrames, MasterMeter, MasterReading, MasterTap};

use measure::{Measurement, nothing_yet};

/// How often the meter's thread reads the tap: far within what the tap holds (`MASTER_TAP_FRAMES`).
const FEED_INTERVAL: Duration = Duration::from_millis(50);

pub struct LiveMeter {
    inner: Arc<Inner>,
}

struct Inner {
    tap: Arc<dyn MasterTap>,
    log: Arc<dyn AppLog>,
    /// How long a reading keeps the measuring going (`CAPTURE_LEASE_SECONDS`; shorter in tests).
    lease: Duration,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The current measurement, `None` while stopped. A thread of an older one ends when it sees another one.
    watch: Option<u64>,
    next_watch: u64,
    /// When the measuring stops unless read again: a lease after the last reading.
    until: Option<Instant>,
    /// The end of the frames read from the tap in this measurement.
    end: u64,
    /// The tap's restart mark already applied.
    restart: Option<u64>,
    /// `None` until the first frames tell the sample rate.
    measurement: Option<Measurement>,
}

impl LiveMeter {
    pub fn new(tap: Arc<dyn MasterTap>, log: Arc<dyn AppLog>) -> Self {
        Self::with_lease(tap, log, Duration::from_secs_f32(CAPTURE_LEASE_SECONDS))
    }

    fn with_lease(tap: Arc<dyn MasterTap>, log: Arc<dyn AppLog>, lease: Duration) -> Self {
        Self { inner: Arc::new(Inner { tap, log, lease, state: Mutex::new(State::default()) }) }
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
                if state.until.is_some_and(|until| Instant::now() >= until) {
                    state.stop(&*inner.tap);
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

    fn reading(&self) -> MasterReading {
        let mut state = lock(&self.inner.state);
        let started = state.watch.is_none().then(|| state.start(&*self.inner.tap));
        state.until = Some(Instant::now() + self.inner.lease);
        state.feed(&*self.inner.tap, &*self.inner.log);
        let reading = state.measurement.as_ref().map_or_else(|| nothing_yet(0), Measurement::reading);
        drop(state);
        if let Some(watch) = started {
            self.spawn_feeder(watch);
        }
        reading
    }
}

impl State {
    /// A fresh measurement with the tap open; returns its number for its thread.
    fn start(&mut self, tap: &dyn MasterTap) -> u64 {
        let watch = self.next_watch;
        *self = State { watch: Some(watch), next_watch: watch + 1, ..State::default() };
        // Under the state lock: a reading never sees the measurement before the tap is open (or after it is closed).
        tap.open_tap(true);
        watch
    }

    /// Nobody read for a whole lease: the measurement is dropped and the engine stops copying.
    fn stop(&mut self, tap: &dyn MasterTap) {
        *self = State { next_watch: self.next_watch, ..State::default() };
        tap.open_tap(false);
    }

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
    /// A meter going away while measuring closes the tap, so the engine stops copying for nobody.
    fn drop(&mut self) {
        if lock(&self.state).watch.is_some() {
            self.tap.open_tap(false);
        }
    }
}
