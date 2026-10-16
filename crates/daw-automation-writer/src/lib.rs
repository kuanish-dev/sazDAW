//! Writing automation live (`AutomationWriter`): param moves made while the song plays become points of the lane's
//! clips.
//!
//! The UI (through RPC) and MIDI knobs (`ProjectMidiMappings`) report every move with `touch`; this crate keeps one
//! [`Stretch`](stretch::Stretch) per target being written, follows the play position with a background poll
//! (thread `daw-automation-write`), writes the clips through `AutomationControl` and tells the engine which lanes not
//! to play (`AutomationHold`). Everything a play pass changes is one undo step (an edit group, as in the note
//! recorder), opened on the first written move and closed when the transport stops or the mode becomes `Off`.
//!
//! Where the points go: into the clip under the play position, at its pattern position (in a looped clip that changes
//! the pattern of every repeat; crossing the loop seam starts a new stretch at the pattern start). Outside clips a new
//! clip is created from the start of the bar of the first write (not before the end of the previous clip) and grows,
//! loop = length, as writing goes on — up to the next clip or the longest loop, where the next clip takes over.

mod stretch;
#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use daw_contracts::codes::automation;
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AutoPoint, AutomationClipEdit, AutomationClipId, AutomationControl, AutomationHold, AutomationWriteMode, AutomationWriter,
    ContractError, ErrorKind, HistoryControl, LaneId, LogEntry, MAX_CLIP_LOOP_TICKS, PPQ, ParamTarget, TOUCH_IDLE_MS, TempoControl,
    TouchEnd, TransportControl, TransportState, tr,
};

use stretch::Stretch;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// Lanes being written are re-sent to the project at most this often (the engine does not play them meanwhile, so
/// only the drawn lane lags).
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// What the writer works with. Contracts only.
pub struct WriterDeps {
    pub automation: Arc<dyn AutomationControl>,
    pub history: Arc<dyn HistoryControl>,
    /// The play position and state: where the points go and when a pass ends.
    pub transport: Arc<dyn TransportControl>,
    /// Bar starts: where a clip created by writing begins.
    pub tempo: Arc<dyn TempoControl>,
    pub hold: Arc<dyn AutomationHold>,
    pub log: Arc<dyn AppLog>,
}

/// Writing of one target into one clip in this pass.
struct Write {
    target: ParamTarget,
    lane: LaneId,
    clip: AutomationClipId,
    /// Song tick of the clip start.
    origin: u32,
    /// The pattern length of an existing clip; `None` for a clip this pass created, which grows with the writing.
    loop_ticks: Option<u32>,
    /// Song tick where writing into this clip ends: the clip end, or for a growing clip the next clip or longest loop.
    limit: u32,
    /// The clip length the project has (a growing clip is lengthened on flush).
    length: u32,
    /// Pattern ticks.
    stretch: Stretch,
    /// Still writing; `false` after release or idle timeout (the stretch stays in the clip).
    active: bool,
    end: TouchEnd,
    last_touch: Instant,
    /// Has points the project has not got yet.
    dirty: bool,
}

impl Write {
    /// The pattern tick of song `tick`, `None` outside the clip's writable span.
    fn local(&self, tick: u32) -> Option<u32> {
        if tick < self.origin || tick >= self.limit {
            return None;
        }
        let offset = tick - self.origin;
        Some(self.loop_ticks.map_or(offset, |l| offset % l.max(1)))
    }

    /// The length a growing clip needs for what is written: whole beats, within its limit.
    fn grown_length(&self) -> u32 {
        (self.stretch.end() + 1).div_ceil(PPQ).saturating_mul(PPQ).min(self.limit - self.origin).max(1)
    }
}

/// A target to start writing again at the current position (it left its clip or jumped back).
struct Resume {
    target: ParamTarget,
    value: f32,
    end: TouchEnd,
    last_touch: Instant,
}

#[derive(Default)]
struct State {
    mode: AutomationWriteMode,
    writes: Vec<Write>,
    group_open: bool,
    /// The last seen play position: after a stop the transport is back at the start.
    last_tick: u32,
    last_flush: Option<Instant>,
    /// A failure of this pass is already logged (polling must not flood the log).
    failure_logged: bool,
}

struct Inner {
    deps: WriterDeps,
    /// Held for the whole poll or touch, project edits included. Project contracts never call back into the writer,
    /// so there is no deadlock.
    state: Mutex<State>,
}

fn position(transport: &dyn TransportControl) -> u32 {
    u32::try_from(transport.position_ticks()).unwrap_or(u32::MAX)
}

impl Inner {
    fn report(&self, state: &mut State, error: &ContractError) {
        if !state.failure_logged {
            state.failure_logged = true;
            self.deps.log.record(LogEntry::error("automation", format!("live automation was not written: {error}")));
        }
    }

    fn open_group(&self, state: &mut State) {
        if !state.group_open {
            self.deps.history.begin_group(&tr!("Write automation", "Запись автоматизации"));
            state.group_open = true;
        }
    }

    fn update_holds(&self, state: &State) {
        let lanes: Vec<LaneId> = state.writes.iter().filter(|w| w.active).map(|w| w.lane).collect();
        self.deps.hold.hold(&lanes);
    }

    /// A write of `target` at song `tick`: into the clip there, or into a new clip from the bar start (the lane is
    /// created if missing).
    fn open_write(&self, resume: Resume, tick: u32) -> Result<Write, ContractError> {
        let Resume { target, value, end, last_touch } = resume;
        let lanes = self.deps.automation.lanes();
        let (lane, clips) = match lanes.into_iter().find(|lane| lane.target == target) {
            Some(lane) => (lane.id, lane.clips),
            None => (self.deps.automation.add_lane(target.clone())?, Vec::new()),
        };
        let write = |clip, origin, loop_ticks, limit, length, stretch| Write {
            target: target.clone(),
            lane,
            clip,
            origin,
            loop_ticks,
            limit,
            length,
            stretch,
            active: true,
            end,
            last_touch,
            dirty: true,
        };
        let under = clips.iter().find(|c| c.start_tick <= tick && tick - c.start_tick < c.length_ticks);
        if let Some(clip) = under {
            let local = (tick - clip.start_tick) % clip.loop_ticks.max(1);
            let stretch = Stretch::begin(clip.points.clone(), local, value, true);
            let limit = clip.start_tick.saturating_add(clip.length_ticks);
            return Ok(write(clip.id, clip.start_tick, Some(clip.loop_ticks), limit, clip.length_ticks, stretch));
        }
        let previous_end = clips.iter().map(|c| c.start_tick.saturating_add(c.length_ticks)).filter(|&e| e <= tick).max().unwrap_or(0);
        let bar = self.deps.tempo.tempo_map().bar_start(f64::from(tick)).max(0.0) as u32;
        let origin = bar.max(previous_end).min(tick);
        let next = clips.iter().map(|c| c.start_tick).filter(|&s| s > tick).min().unwrap_or(u32::MAX);
        let limit = next.min(origin.saturating_add(MAX_CLIP_LOOP_TICKS));
        let length = tick - origin + 1;
        let clip = self.deps.automation.add_clip(lane, origin, length)?;
        let original = self.clip_points(lane, clip);
        Ok(write(clip, origin, None, limit, length, Stretch::begin(original, tick - origin, value, false)))
    }

    fn clip_points(&self, lane: LaneId, clip: AutomationClipId) -> Vec<AutoPoint> {
        let lanes = self.deps.automation.lanes();
        let clip = lanes.iter().filter(|l| l.id == lane).flat_map(|l| &l.clips).find(|c| c.id == clip);
        clip.map(|c| c.points.clone()).unwrap_or_default()
    }

    /// Starts writing `resume.target` anew at `tick`; what its previous write wrote goes to the project first.
    fn begin(&self, state: &mut State, resume: Resume, tick: u32, now: Instant) -> Result<(), ContractError> {
        if state.writes.iter().any(|w| w.target == resume.target) {
            self.flush(state, now);
            state.writes.retain(|w| w.target != resume.target);
        }
        let write = self.open_write(resume, tick)?;
        state.writes.push(write);
        Ok(())
    }

    fn send(&self, write: &mut Write) -> Result<(), ContractError> {
        if write.loop_ticks.is_none() {
            let length = write.grown_length();
            if length != write.length {
                let edit = AutomationClipEdit { length_ticks: Some(length), loop_ticks: Some(length), ..AutomationClipEdit::default() };
                self.deps.automation.edit_clip(write.lane, write.clip, edit)?;
                write.length = length;
            }
        }
        self.deps.automation.set_points(write.lane, write.clip, write.stretch.lane())
    }

    /// Sends dirty clips to the project. A vanished lane or clip (deleted while writing) just drops its writing.
    fn flush(&self, state: &mut State, now: Instant) {
        state.last_flush = Some(now);
        let mut gone = Vec::new();
        let mut failure = None;
        for write in state.writes.iter_mut().filter(|w| w.dirty) {
            write.dirty = false;
            if let Err(error) = self.send(write) {
                if error.kind() == ErrorKind::NotFound {
                    gone.push(write.clip);
                } else {
                    // A full clip or a clash: writing this target stops, what was written before stays.
                    write.active = false;
                    failure = Some(error);
                }
            }
        }
        state.writes.retain(|w| !gone.contains(&w.clip));
        if let Some(error) = failure {
            self.report(state, &error);
        }
    }

    fn touch(&self, target: ParamTarget, value: f32, end: TouchEnd, now: Instant) -> Result<(), ContractError> {
        if !value.is_finite() {
            return Err(automation::POINT_NOT_A_NUMBER.with("value", value));
        }
        let mut state = lock(&self.state);
        if state.mode == AutomationWriteMode::Off || self.deps.transport.state() != TransportState::Playing {
            return Ok(());
        }
        let tick = position(&*self.deps.transport);
        self.open_group(&mut state);
        let forward = state.writes.iter_mut().find(|w| w.target == target && w.active).and_then(|w| {
            let local = w.local(tick).filter(|&l| l >= w.stretch.end())?;
            Some((w, local))
        });
        if let Some((write, local)) = forward {
            write.stretch.write(local, value);
            write.end = end;
            write.last_touch = now;
            write.dirty = true;
        } else {
            self.begin(&mut state, Resume { target, value, end, last_touch: now }, tick, now)?;
        }
        self.update_holds(&state);
        Ok(())
    }

    fn release(&self, matches: impl Fn(&Write) -> bool) {
        let mut state = lock(&self.state);
        if state.mode != AutomationWriteMode::Touch {
            return;
        }
        let mut released = false;
        for write in state.writes.iter_mut().filter(|w| w.active && matches(w)) {
            write.active = false;
            write.dirty = true;
            released = true;
        }
        if released {
            self.flush(&mut state, Instant::now());
            self.update_holds(&state);
        }
    }

    /// The pass ended: the last stretches reach the last seen position, the undo step closes.
    fn finish(&self, state: &mut State, now: Instant) {
        let last = state.last_tick;
        for write in state.writes.iter_mut().filter(|w| w.active) {
            if let Some(local) = write.local(last) {
                write.dirty |= write.stretch.extend(local);
            }
            write.active = false;
        }
        self.flush(state, now);
        state.writes.clear();
        self.deps.hold.hold(&[]);
        if std::mem::take(&mut state.group_open) {
            self.deps.history.end_group();
        }
        state.failure_logged = false;
        state.last_flush = None;
    }

    fn poll(&self, now: Instant) {
        let mut state = lock(&self.state);
        if !state.group_open && state.writes.is_empty() {
            return;
        }
        let playing = self.deps.transport.state() == TransportState::Playing;
        if !playing || state.mode == AutomationWriteMode::Off {
            self.finish(&mut state, now);
            return;
        }
        let tick = position(&*self.deps.transport);
        let touch_mode = state.mode == AutomationWriteMode::Touch;
        let idle = Duration::from_millis(TOUCH_IDLE_MS);
        let mut resume = Vec::new();
        for write in state.writes.iter_mut().filter(|w| w.active) {
            let local = write.local(tick);
            if touch_mode && write.end == TouchEnd::Idle && now.duration_since(write.last_touch) > idle {
                write.active = false;
                write.dirty = true;
            } else if let Some(local) = local.filter(|&l| l >= write.stretch.end()) {
                write.dirty |= write.stretch.extend(local);
            } else {
                // Left the clip, crossed the loop seam or the transport jumped back: a new stretch starts here with
                // the same value.
                let value = write.stretch.value();
                resume.push(Resume { target: write.target.clone(), value, end: write.end, last_touch: write.last_touch });
            }
        }
        for item in resume {
            if let Err(error) = self.begin(&mut state, item, tick, now) {
                self.report(&mut state, &error);
            }
        }
        let due = state.last_flush.is_none_or(|at| now.duration_since(at) >= FLUSH_INTERVAL);
        if due || state.writes.iter().any(|w| !w.active && w.dirty) {
            self.flush(&mut state, now);
        }
        self.update_holds(&state);
        state.last_tick = tick;
    }

    fn set_mode(&self, mode: AutomationWriteMode) {
        let mut state = lock(&self.state);
        state.mode = mode;
        if mode == AutomationWriteMode::Off {
            self.finish(&mut state, Instant::now());
        }
    }
}

/// The live automation writer with its background poll.
pub struct LiveAutomationWriter {
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl LiveAutomationWriter {
    /// Creates the writer and starts the poll thread. If the thread fails to start, that is logged; a pass then ends
    /// only when the mode is switched off.
    pub fn start(deps: WriterDeps) -> Self {
        let inner = Arc::new(Inner { deps, state: Mutex::default() });
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (inner, stop) = (Arc::clone(&inner), Arc::clone(&stop));
            thread::Builder::new().name("daw-automation-write".into()).spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    inner.poll(Instant::now());
                    thread::sleep(POLL_INTERVAL);
                }
            })
        };
        let worker = match worker {
            Ok(handle) => Some(handle),
            Err(error) => {
                inner.deps.log.record(LogEntry::error("automation", format!("could not start the automation writing thread: {error}")));
                None
            }
        };
        Self { inner, stop, worker: Mutex::new(worker) }
    }

    /// One poll step at `now` by hand (for tests and exact synchronization).
    pub fn poll_at(&self, now: Instant) {
        self.inner.poll(now);
    }
}

impl Drop for LiveAutomationWriter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = lock(&self.worker).take() {
            // A thread panic is already logged by the panic hook; here we only wait for the thread to end.
            let _ = worker.join();
        }
    }
}

impl AutomationWriter for LiveAutomationWriter {
    fn mode(&self) -> AutomationWriteMode {
        lock(&self.inner.state).mode
    }

    fn set_mode(&self, mode: AutomationWriteMode) {
        self.inner.set_mode(mode);
    }

    fn touch(&self, target: ParamTarget, value: f32, end: TouchEnd) -> Result<(), ContractError> {
        self.inner.touch(target, value, end, Instant::now())
    }

    fn release(&self, target: &ParamTarget) {
        self.inner.release(|w| w.target == *target);
    }

    fn release_all(&self) {
        self.inner.release(|w| w.end == TouchEnd::Release);
    }
}
