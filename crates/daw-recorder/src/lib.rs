//! Recording notes played live into a track clip.
//!
//! The engine sends events with the transport tick and the track; the recorder pairs presses and releases
//! (`pairing`), optionally snaps notes to the grid (`shape`) and adds them to the track note clip under the cursor at
//! the take start (`take`; without a clip a one-bar clip is created). Recording loops over the clip content length at
//! the start: the clip does not grow, and a note played on a new pass replaces what the take recorded on earlier
//! passes. The whole take is one undo step. Note times are from the clip start, including its offset.
//! Controllers (pitch bend, CC knobs, pressure) go to the controller lanes of the same clip (`controllers`).
//! While recording is armed the song loop waits (`TransportControl::hold_song_loop`): a take may run past the song end
//! and lengthen the song instead of jumping back to its start.

mod controllers;
mod pairing;
mod shape;
mod take;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use daw_contracts::codes::record;
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, BAR_TICKS, ClipControl, ContractError, ControllerKind, ErrorKind, HistoryControl, LiveMessage, LogEntry, LogLevel, NewNote,
    NewNoteClip, NoteControl, RecordControl, RecordSource, RecordStatus, RecordedEvent, TempoControl, TempoMap, TrackId, TransportControl,
    TransportState, tr,
};

use controllers::{ControllerTake, Played};
use pairing::{Finished, Pairing};
use shape::pass_of;
use take::TrackTakes;

const POLL_INTERVAL: Duration = Duration::from_millis(15);

#[derive(Default)]
struct State {
    /// A take is running: from the first event or the start of playback until stop.
    active: bool,
    /// Transport tick at the take start: the clip under the cursor is looked up by it.
    start_tick: f64,
    takes: TrackTakes,
    pairing: Pairing,
    controllers: ControllerTake,
    group_open: bool,
    /// Grid quantize; 0 records notes as played (the default).
    quantize: u32,
    /// A failure of this take is already logged: polling runs every 15 ms, one cause must not flood the log.
    failure_logged: bool,
}

struct Inner {
    source: Arc<dyn RecordSource>,
    clips: Arc<dyn ClipControl>,
    notes: Arc<dyn NoteControl>,
    history: Arc<dyn HistoryControl>,
    transport: Arc<dyn TransportControl>,
    /// The tempo map: a new clip starts at the bar start by the time signature at that point of the song.
    tempo: Arc<dyn TempoControl>,
    log: Arc<dyn AppLog>,
    /// The lock is held for the whole poll, project edits included: "drain events, handle them" must not be split by a
    /// second poll (worker thread, `set_armed`, tests), and edit results (clip and note ids) are needed right away for
    /// the next events. Project contracts never call back into the recorder, so there is no deadlock; `status` waits
    /// at most one poll.
    state: Mutex<State>,
}

impl Inner {
    /// Recorded data did not reach the project: log the cause once per take. "The clip vanished while recording" is an
    /// expected case and is not logged.
    fn report_failure(&self, state: &mut State, what: &str, error: &ContractError) {
        if error.kind() == ErrorKind::NotFound || state.failure_logged {
            return;
        }
        state.failure_logged = true;
        self.log.record(LogEntry::new(LogLevel::Error, "record", format!("{what}: {error}")));
    }

    fn capturing_now(&self) -> bool {
        self.source.is_armed() && self.transport.state() == TransportState::Playing
    }

    fn open_group(&self, state: &mut State) {
        if !state.group_open {
            self.history.begin_group(&tr!("Recording", "Запись"));
            state.group_open = true;
        }
    }

    /// Finds the track clip under the cursor at the take start or creates a one-bar clip.
    /// `false` means there is nowhere to write (track without an instrument, spot taken): the track notes are skipped.
    fn bind(&self, state: &mut State, track: TrackId) -> bool {
        let at = state.start_tick.max(0.0);
        if let Some(clip) = take::clip_under(self.clips.clips(), track, at) {
            return state.takes.bind(track, &clip);
        }
        self.open_group(state);
        let bar_start = self.tempo.tempo_map().bar_start(at) as u32;
        match self.clips.add_note_clip(NewNoteClip::empty(track, bar_start, None)) {
            Ok(clip) => state.takes.bind(track, &clip),
            Err(error) => {
                state.takes.drop_track(track);
                self.report_failure(state, "could not create a clip for recorded notes", &error);
                false
            }
        }
    }

    /// Adds finished notes (each tagged with its pass) to the take clips.
    fn commit(&self, state: &mut State, notes: Finished) {
        if notes.is_empty() {
            return;
        }
        self.open_group(state);
        for (track, list) in notes {
            let Some(clip) = state.takes.get(track).map(|take| take.clip) else {
                continue;
            };
            let (passes, new_notes): (Vec<u64>, Vec<NewNote>) = list.into_iter().unzip();
            match self.notes.add_notes(clip, new_notes) {
                Ok(created) => state.takes.record(track, passes, created.into_iter().map(|n| n.id)),
                // The clip may have vanished while recording (not found): expected; anything else means the notes are lost.
                Err(error) => self.report_failure(state, "recorded notes were not added to the clip", &error),
            }
        }
    }

    /// A note on pass `pass`: the first note of a new pass saves what has accumulated and replaces notes of earlier passes.
    fn enter_pass(&self, state: &mut State, track: TrackId, pass: u64, finished: &mut Finished) {
        if state.takes.starts_new_pass(track, pass) {
            self.commit(state, std::mem::take(finished));
        }
        let Some((clip, replaced)) = state.takes.enter_pass(track, pass) else {
            return;
        };
        if replaced.is_empty() {
            return;
        }
        self.open_group(state);
        // The user may have deleted the notes by hand (not found): expected; anything else leaves both passes in the clip.
        if let Err(error) = self.notes.remove_notes(clip, replaced) {
            self.report_failure(state, "notes of the previous pass were not removed from the clip", &error);
        }
    }

    /// Recorded controllers go to the take clip lanes. `flush` means the take ended (see `controllers.rs`).
    fn commit_controllers(&self, state: &mut State, flush: bool) {
        let changes = state.controllers.take_changes(flush);
        if changes.is_empty() {
            return;
        }
        self.open_group(state);
        for (clip, kind, points) in changes {
            // The clip may have vanished while recording (not found): expected; anything else means the controllers are lost.
            if let Err(error) = self.notes.set_controller_lane(clip, kind, points) {
                self.report_failure(state, "recorded controllers were not written to the clip", &error);
            }
        }
    }

    /// The track writes to the take clip: its first event binds the clip (accumulated notes are saved first so that
    /// creating the clip does not overtake them). `false` means there is nowhere to write.
    fn ensure_bound(&self, state: &mut State, track: TrackId, finished: &mut Finished) -> bool {
        if state.takes.is_dropped(track) {
            return false;
        }
        if state.takes.is_bound(track) {
            return true;
        }
        self.commit(state, std::mem::take(finished));
        self.bind(state, track)
    }

    fn handle_controller(&self, state: &mut State, event: (TrackId, ControllerKind, f32, f64), map: &TempoMap, finished: &mut Finished) {
        let (track, kind, value, tick) = event;
        if !self.ensure_bound(state, track, finished) {
            return;
        }
        let Some(take) = state.takes.get(track) else {
            return;
        };
        let relative = (tick + take.shift).max(0.0);
        let pass = pass_of(relative, take.loop_ticks);
        let position = relative - pass as f64 * f64::from(take.loop_ticks);
        let played = Played { kind, value, tick: (position.floor() as u32).min(take.loop_ticks - 1), pass, seconds: map.seconds_at(tick) };
        state.controllers.observe(track, take, &played);
    }

    fn begin_take(&self, state: &mut State, capturing: bool, events: &[RecordedEvent]) {
        state.active = true;
        state.takes.clear();
        state.controllers.clear();
        state.failure_logged = false;
        // The take starts no later than the first event: polling may lag, and the transport position may have moved on.
        let earliest = events.iter().map(|e| e.tick).fold(f64::MAX, f64::min);
        state.start_tick = if capturing { (self.transport.position_ticks() as f64).min(earliest) } else { earliest };
        state.pairing.restart(state.start_tick);
    }

    fn handle_events(&self, state: &mut State, events: Vec<RecordedEvent>) {
        let mut finished = Finished::new();
        // Song seconds are needed to thin controllers by time.
        let map = self.tempo.tempo_map();
        for RecordedEvent { track, message, tick } in events {
            state.pairing.observe(tick);
            let (pitch, velocity) = match message {
                LiveMessage::Note { pitch, velocity } => (pitch, velocity),
                LiveMessage::Controller { kind, value } => {
                    self.handle_controller(state, (track, kind, value, tick), &map, &mut finished);
                    continue;
                }
            };
            state.pairing.release(track, pitch, tick, &state.takes, state.quantize, &mut finished);
            if velocity <= 0.0 || !self.ensure_bound(state, track, &mut finished) {
                continue;
            }
            let Some(take) = state.takes.get(track) else {
                continue;
            };
            let relative = tick + take.shift;
            let pass = pass_of(relative, take.loop_ticks);
            self.enter_pass(state, track, pass, &mut finished);
            state.pairing.press(track, pitch, relative, velocity);
        }
        self.commit(state, finished);
        self.commit_controllers(state, false);
    }

    /// The take ended (stop, pause, disarm): finish the held notes and close the undo step.
    fn finish_take(&self, state: &mut State) {
        // On pause the position is still valid; after stop it is reset, so the last seen tick is used.
        let last = state.pairing.last_tick();
        let end = if self.transport.state() == TransportState::Paused { last.max(self.transport.position_ticks() as f64) } else { last };
        let tail = state.pairing.close_all(end, &state.takes, state.quantize);
        self.commit(state, tail);
        self.commit_controllers(state, true);
        if state.group_open {
            self.history.end_group();
            state.group_open = false;
        }
        state.active = false;
        // The take result stays in `takes` until the next take: the note counter is computed from it.
    }

    fn poll(&self) {
        let mut state = lock(&self.state);
        let events = self.source.drain();
        let capturing = self.source.is_armed() && self.transport.state() == TransportState::Playing;

        // A take starts with playback while armed. The source collects events only while armed, so whatever is drained
        // belongs to a take, even when it arrives after stop or disarm (the last notes before the poll).
        if (capturing || !events.is_empty()) && !state.active {
            self.begin_take(&mut state, capturing, &events);
        }
        if capturing {
            state.pairing.observe(self.transport.position_ticks() as f64);
        }
        if state.active {
            self.handle_events(&mut state, events);
        }
        if state.active && !capturing {
            self.finish_take(&mut state);
        }
    }
}

pub struct Recorder {
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// This recorder holds the transport's song loop (armed): the swap pairs every hold with one release.
    holds_song_loop: AtomicBool,
}

impl Recorder {
    /// Creates the recorder and starts background polling (thread `daw-note-record`). If the thread fails to start,
    /// that is logged, and recorded notes are picked up by the next poll from `set_armed(false)`.
    pub fn start(
        source: Arc<dyn RecordSource>,
        clips: Arc<dyn ClipControl>,
        notes: Arc<dyn NoteControl>,
        history: Arc<dyn HistoryControl>,
        transport: Arc<dyn TransportControl>,
        tempo: Arc<dyn TempoControl>,
        log: Arc<dyn AppLog>,
    ) -> Self {
        let inner = Arc::new(Inner { source, clips, notes, history, transport, tempo, log, state: Mutex::default() });
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (inner, stop) = (Arc::clone(&inner), Arc::clone(&stop));
            thread::Builder::new().name("daw-note-record".into()).spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    inner.poll();
                    thread::sleep(POLL_INTERVAL);
                }
            })
        };
        let worker = match worker {
            Ok(handle) => Some(handle),
            Err(error) => {
                let message = format!("could not start the note recording thread: {error}");
                inner.log.record(LogEntry::new(LogLevel::Error, "record", message));
                None
            }
        };
        Self { inner, stop, worker: Mutex::new(worker), holds_song_loop: AtomicBool::new(false) }
    }

    /// One poll step by hand (for tests and exact synchronization).
    pub fn poll_once(&self) {
        self.inner.poll();
    }

    fn hold_song_loop(&self, hold: bool) {
        if self.holds_song_loop.swap(hold, Ordering::AcqRel) != hold {
            self.inner.transport.hold_song_loop(hold);
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.hold_song_loop(false);
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = lock(&self.worker).take() {
            // A thread panic is already logged by the panic hook; here we only wait for the thread to end.
            let _ = worker.join();
        }
    }
}

impl RecordControl for Recorder {
    fn set_armed(&self, armed: bool) {
        // The hold comes before the first captured block and goes after the take is finished.
        if armed {
            self.hold_song_loop(true);
        }
        self.inner.source.set_armed(armed);
        if !armed {
            self.inner.poll();
            self.hold_song_loop(false);
        }
    }

    fn set_quantize(&self, ticks: u32) -> Result<(), ContractError> {
        if ticks > BAR_TICKS {
            return Err(record::QUANTIZE_OUT_OF_RANGE.with("ticks", ticks).with("max", BAR_TICKS));
        }
        lock(&self.inner.state).quantize = ticks;
        Ok(())
    }

    fn status(&self) -> RecordStatus {
        let state = lock(&self.inner.state);
        RecordStatus {
            armed: self.inner.source.is_armed(),
            capturing: self.inner.capturing_now(),
            take_notes: state.takes.note_count(),
            quantize_ticks: state.quantize,
        }
    }
}

#[cfg(test)]
mod tests;
