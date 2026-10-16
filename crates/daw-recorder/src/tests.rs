//! Note recorder tests with a fake event source, clips, notes, history, transport and log.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use daw_contracts::codes::{clips, general, record};
use daw_contracts::{
    AppLog, BAR_TICKS, Bpm, Clip, ClipContent, ClipControl, ClipEdit, ClipId, ContractError, ControllerKind, ControllerLane,
    ControllerPoint, HistoryControl, HistoryMark, LiveMessage, LogEntry, LogLevel, NewNote, NewNoteClip, Note, NoteContent, NoteControl,
    NoteId, RecordControl, RecordSource, RecordedEvent, SampleId, SignatureChange, TempoChange, TempoControl, TempoMap, TimeSignature,
    TrackId, TransportControl, TransportState,
};

use crate::Recorder;

#[derive(Default)]
struct FakeSource {
    armed: AtomicBool,
    queue: Mutex<Vec<RecordedEvent>>,
}

impl RecordSource for FakeSource {
    fn set_armed(&self, armed: bool) {
        self.armed.store(armed, Ordering::SeqCst);
    }
    fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }
    /// Like the engine: what was collected while armed is drained even after a disarm.
    fn drain(&self) -> Vec<RecordedEvent> {
        std::mem::take(&mut *self.queue.lock().unwrap())
    }
}

#[derive(Default)]
struct FakeClips {
    clips: Mutex<Vec<Clip>>,
    /// Tracks without an instrument: no note clip can be created on them.
    audio_tracks: Mutex<Vec<TrackId>>,
    /// No note clip can be created at all (the spot is taken).
    refuse_new: AtomicBool,
}

impl FakeClips {
    fn with_clip(&self, track: TrackId, start: u32, length: u32, offset: u32, loop_ticks: u32) -> ClipId {
        let mut clips = self.clips.lock().unwrap();
        let id = 100 + clips.len() as u32;
        let mut content = NoteContent::empty();
        content.loop_ticks = loop_ticks;
        clips.push(clip(id, track, start, length, offset, ClipContent::Notes(content)));
        id
    }
}

fn clip(id: ClipId, track: TrackId, start: u32, length: u32, offset: u32, content: ClipContent) -> Clip {
    Clip {
        id,
        track,
        start_tick: start,
        length_ticks: length,
        content,
        gain: 1.0,
        fade_in_ticks: 0,
        fade_out_ticks: 0,
        fade_in_shape: Default::default(),
        fade_out_shape: Default::default(),
        offset_ticks: offset,
        offset_frames: 0,
        warp: None,
        pitch_semitones: 0.0,
    }
}

impl ClipControl for FakeClips {
    fn clips(&self) -> Vec<Clip> {
        self.clips.lock().unwrap().clone()
    }
    fn loop_region(&self) -> Option<(u32, u32)> {
        None
    }
    fn add_note_clip(&self, new: NewNoteClip) -> Result<Clip, ContractError> {
        let NewNoteClip { track, start_tick: start, length_ticks: length, .. } = new;
        if self.refuse_new.load(Ordering::SeqCst) || self.audio_tracks.lock().unwrap().contains(&track) {
            return Err(general::INTERNAL.with("details", "cannot create a clip"));
        }
        let mut clips = self.clips.lock().unwrap();
        let id = 100 + clips.len() as u32;
        let created = clip(id, track, start, length.unwrap_or(BAR_TICKS), 0, ClipContent::Notes(NoteContent::empty()));
        clips.push(created.clone());
        Ok(created)
    }
    fn add_note_clips(&self, _: Vec<NewNoteClip>) -> Result<Vec<Clip>, ContractError> {
        Err(unused())
    }
    fn add_audio_clip(&self, _: TrackId, _: u32, _: SampleId) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn edit_clip(&self, _: ClipId, _: ClipEdit) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn split_clip(&self, _: ClipId, _: u32) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn trim_start(&self, _: ClipId, _: u32) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn duplicate_clip(&self, _: ClipId) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn remove_clip(&self, _: ClipId) -> Result<(), ContractError> {
        Err(unused())
    }
    fn edit_clips(&self, _: Vec<(ClipId, ClipEdit)>) -> Result<Vec<Clip>, ContractError> {
        Err(unused())
    }
    fn remove_clips(&self, _: Vec<ClipId>) -> Result<(), ContractError> {
        Err(unused())
    }
    fn copy_clips(&self, _: Vec<ClipId>, _: u32) -> Result<Vec<Clip>, ContractError> {
        Err(unused())
    }
    fn set_loop_region(&self, _: Option<(u32, u32)>) -> Result<(), ContractError> {
        Err(unused())
    }
}

#[derive(Default)]
struct FakeNotes {
    notes: Mutex<BTreeMap<ClipId, Vec<Note>>>,
    next_id: Mutex<NoteId>,
    /// Clips that are "gone": writing into them fails with not found.
    gone: Mutex<BTreeSet<ClipId>>,
    /// Note removal breaks (not a not-found failure).
    refuse_remove: AtomicBool,
    /// Controller lanes per clip: the last write of each kind.
    lanes: Mutex<BTreeMap<ClipId, Vec<ControllerLane>>>,
}

impl FakeNotes {
    fn fail_with_not_found(&self, clip: ClipId) {
        self.gone.lock().unwrap().insert(clip);
    }

    fn of(&self, clip: ClipId) -> Vec<(u8, u32, u32)> {
        let notes = self.notes.lock().unwrap();
        notes.get(&clip).map_or_else(Vec::new, |list| list.iter().map(|n| (n.pitch, n.start_tick, n.length_ticks)).collect())
    }
}

impl NoteControl for FakeNotes {
    fn notes(&self, clip: ClipId) -> Result<Vec<Note>, ContractError> {
        Ok(self.notes.lock().unwrap().get(&clip).cloned().unwrap_or_default())
    }
    fn add_note(&self, _: ClipId, _: NewNote) -> Result<Note, ContractError> {
        Err(unused())
    }
    fn update_note(&self, _: ClipId, _: NoteId, _: NewNote) -> Result<Note, ContractError> {
        Err(unused())
    }
    fn remove_note(&self, _: ClipId, _: NoteId) -> Result<(), ContractError> {
        Err(unused())
    }
    fn clear_notes(&self, _: ClipId) -> Result<(), ContractError> {
        Err(unused())
    }
    fn add_notes(&self, clip: ClipId, notes: Vec<NewNote>) -> Result<Vec<Note>, ContractError> {
        if self.gone.lock().unwrap().contains(&clip) {
            return Err(clips::NOT_FOUND.with("id", clip));
        }
        let mut next = self.next_id.lock().unwrap();
        let created: Vec<Note> = notes
            .into_iter()
            .map(|n| {
                *next += 1;
                Note::from_new(*next, n)
            })
            .collect();
        self.notes.lock().unwrap().entry(clip).or_default().extend(created.iter().copied());
        Ok(created)
    }
    fn update_notes(&self, _: ClipId, _: Vec<(NoteId, NewNote)>) -> Result<Vec<Note>, ContractError> {
        Err(unused())
    }
    fn remove_notes(&self, clip: ClipId, ids: Vec<NoteId>) -> Result<(), ContractError> {
        if self.refuse_remove.load(Ordering::SeqCst) {
            return Err(general::INTERNAL.with("details", "disk unavailable"));
        }
        if let Some(list) = self.notes.lock().unwrap().get_mut(&clip) {
            list.retain(|n| !ids.contains(&n.id));
        }
        Ok(())
    }
    fn replace_notes(&self, _: ClipId, _: Vec<NewNote>) -> Result<usize, ContractError> {
        Err(unused())
    }
    fn set_loop_ticks(&self, _: ClipId, _: u32) -> Result<(), ContractError> {
        Err(unused())
    }
    fn transform_notes(&self, _: ClipId, _: Option<Vec<NoteId>>, _: daw_contracts::NoteTransform) -> Result<Vec<Note>, ContractError> {
        Err(unused())
    }
    fn vary_to_new_clip(
        &self,
        _: ClipId,
        _: Option<Vec<NoteId>>,
        _: daw_contracts::Variation,
    ) -> Result<daw_contracts::Clip, ContractError> {
        Err(unused())
    }
    fn set_controller_lane(&self, clip: ClipId, kind: ControllerKind, points: Vec<ControllerPoint>) -> Result<(), ContractError> {
        let mut lanes = self.lanes.lock().unwrap();
        let list = lanes.entry(clip).or_default();
        list.retain(|lane| lane.kind != kind);
        list.push(ControllerLane { kind, points });
        Ok(())
    }
}

#[derive(Default)]
struct FakeHistory {
    begun: Mutex<u32>,
    ended: Mutex<u32>,
}

impl HistoryControl for FakeHistory {
    fn undo(&self) -> bool {
        false
    }
    fn redo(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo_label(&self) -> Option<String> {
        None
    }
    fn begin_group(&self, _: &str) {
        *self.begun.lock().unwrap() += 1;
    }
    fn end_group(&self) -> bool {
        *self.ended.lock().unwrap() += 1;
        true
    }
    fn cancel_group(&self) {}
    fn mark_baseline(&self) {}
    fn checkpoint(&self) -> HistoryMark {
        0
    }
    fn squash(&self, _: HistoryMark, _: &str) -> Result<bool, ContractError> {
        Ok(false)
    }
}

struct FakeTransport {
    state: Mutex<TransportState>,
    position: Mutex<u64>,
    /// Song loop holds minus releases: below zero would mean an unpaired release.
    holds: Mutex<i32>,
}

/// The default tempo and a test-set time signature for the whole song.
struct FakeTempo {
    signature: Mutex<TimeSignature>,
}

impl TempoControl for FakeTempo {
    fn tempo_map(&self) -> TempoMap {
        TempoMap::constant(Bpm::DEFAULT, *self.signature.lock().unwrap())
    }
    fn set_tempo_changes(&self, _: Vec<TempoChange>) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_signature_changes(&self, _: Vec<SignatureChange>) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_tempo_map(&self, _: TempoMap) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_tempo(&self, _: Bpm) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_time_signature(&self, signature: TimeSignature) -> Result<(), ContractError> {
        *self.signature.lock().unwrap() = signature;
        Ok(())
    }
}

impl TransportControl for FakeTransport {
    fn play(&self) {}
    fn pause(&self) {}
    fn stop(&self) {}
    fn seek(&self, _: u64) {}
    fn set_cue(&self, _: Option<u64>) {}
    fn cue(&self) -> Option<u64> {
        None
    }
    fn set_loop(&self, _: Option<(u64, u64)>) {}
    fn loop_region(&self) -> Option<(u64, u64)> {
        None
    }
    fn set_song_loop(&self, _: bool) {}
    fn song_loop(&self) -> bool {
        true
    }
    fn set_song_end(&self, _: Option<u64>) {}
    fn hold_song_loop(&self, hold: bool) {
        *self.holds.lock().unwrap() += if hold { 1 } else { -1 };
    }
    fn wrap_region(&self) -> Option<(u64, u64)> {
        None
    }
    fn state(&self) -> TransportState {
        *self.state.lock().unwrap()
    }
    fn position_ticks(&self) -> u64 {
        *self.position.lock().unwrap()
    }
}

#[derive(Default)]
struct MemoryLog(Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn recent(&self, _: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }
    fn file(&self) -> Option<std::path::PathBuf> {
        None
    }
}

struct Rig {
    log: Arc<MemoryLog>,
    source: Arc<FakeSource>,
    clips: Arc<FakeClips>,
    notes: Arc<FakeNotes>,
    history: Arc<FakeHistory>,
    transport: Arc<FakeTransport>,
    tempo: Arc<FakeTempo>,
    recorder: Recorder,
}

fn rig() -> Rig {
    let source = Arc::new(FakeSource::default());
    let clips = Arc::new(FakeClips::default());
    let notes = Arc::new(FakeNotes::default());
    let history = Arc::new(FakeHistory::default());
    let transport = Arc::new(FakeTransport { state: Mutex::new(TransportState::Stopped), position: Mutex::new(0), holds: Mutex::new(0) });
    let tempo = Arc::new(FakeTempo { signature: Mutex::new(TimeSignature::FOUR_FOUR) });
    let log = Arc::new(MemoryLog::default());
    let recorder = Recorder::start(
        Arc::clone(&source) as _,
        Arc::clone(&clips) as _,
        Arc::clone(&notes) as _,
        Arc::clone(&history) as _,
        Arc::clone(&transport) as _,
        Arc::clone(&tempo) as _,
        Arc::clone(&log) as _,
    );
    Rig { log, source, clips, notes, history, transport, tempo, recorder }
}

impl Rig {
    fn press(&self, track: TrackId, pitch: u8, from: f64, to: Option<f64>) {
        let mut queue = self.source.queue.lock().unwrap();
        queue.push(RecordedEvent { track, message: LiveMessage::Note { pitch, velocity: 0.8 }, tick: from });
        if let Some(to) = to {
            queue.push(RecordedEvent { track, message: LiveMessage::Note { pitch, velocity: 0.0 }, tick: to });
        }
    }

    fn turn(&self, track: TrackId, kind: ControllerKind, value: f32, tick: f64) {
        self.source.queue.lock().unwrap().push(RecordedEvent { track, message: LiveMessage::Controller { kind, value }, tick });
    }

    fn play_from(&self, position: u64) {
        *self.transport.position.lock().unwrap() = position;
        *self.transport.state.lock().unwrap() = TransportState::Playing;
        self.recorder.set_armed(true);
        self.recorder.poll_once();
    }

    fn stop(&self) {
        *self.transport.state.lock().unwrap() = TransportState::Stopped;
        self.recorder.poll_once();
    }
}

#[test]
fn take_creates_a_one_bar_clip_when_none_exists() {
    let r = rig();
    r.play_from(5_000);
    r.press(1, 60, 5_000.0, Some(5_200.0));
    r.recorder.poll_once();
    r.stop();
    let clips = r.clips.clips();
    assert_eq!(clips.len(), 1);
    assert_eq!((clips[0].start_tick, clips[0].length_ticks), (BAR_TICKS, BAR_TICKS), "the bar under the cursor");
    assert_eq!(r.notes.of(clips[0].id), [(60, 1_160, 200)], "time from the clip start");
    assert_eq!((*r.history.begun.lock().unwrap(), *r.history.ended.lock().unwrap()), (1, 1), "one undo step");
    assert_eq!(r.recorder.status().take_notes, 1);
}

#[test]
fn armed_recording_holds_the_song_loop_once_until_disarmed() {
    let r = rig();
    r.play_from(0);
    assert_eq!(*r.transport.holds.lock().unwrap(), 1, "armed: a take may run past the song end");
    r.recorder.set_armed(true);
    assert_eq!(*r.transport.holds.lock().unwrap(), 1, "arming twice holds once");
    r.recorder.set_armed(false);
    r.recorder.set_armed(false);
    assert_eq!(*r.transport.holds.lock().unwrap(), 0, "disarming releases the hold once");
}

#[test]
fn disarming_while_playing_keeps_the_notes_played_since_the_last_poll() {
    let r = rig();
    r.play_from(0);
    r.press(1, 60, 100.0, Some(300.0)); // played after the last poll: still in the engine's queue
    r.recorder.set_armed(false);
    let clips = r.clips.clips();
    assert_eq!(clips.len(), 1);
    assert_eq!(r.notes.of(clips[0].id), [(60, 100, 200)]);
    assert_eq!((*r.history.begun.lock().unwrap(), *r.history.ended.lock().unwrap()), (1, 1), "one undo step");
}

#[test]
fn a_late_poll_still_binds_the_clip_under_the_first_note() {
    // Polling lagged: the transport moved two bars ahead, while the note was played in the first bar.
    let r = rig();
    let id = r.clips.with_clip(1, 0, BAR_TICKS, 0, BAR_TICKS);
    *r.transport.position.lock().unwrap() = 2 * u64::from(BAR_TICKS);
    *r.transport.state.lock().unwrap() = TransportState::Playing;
    r.recorder.set_armed(true);
    r.press(1, 62, 100.0, Some(400.0)); // the events are queued before the first poll
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.clips.clips().len(), 1, "the first bar clip is found, no second one is created");
    assert_eq!(r.notes.of(id), [(62, 100, 300)]);
}

#[test]
fn take_reuses_the_clip_under_the_playhead_and_its_offset_and_start_shift_note_times() {
    let r = rig();
    let id = r.clips.with_clip(1, 3_840, 3_840, 960, 3_840);
    r.play_from(3_840);
    r.press(1, 60, 3_840.0 + 480.0, Some(3_840.0 + 780.0));
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.clips.clips().len(), 1, "no new clip is needed");
    assert_eq!(r.notes.of(id), [(60, 1_440, 300)]);
}

#[test]
fn later_pass_replaces_earlier_pass_of_the_same_take_and_keeps_foreign_notes() {
    let r = rig();
    let id = r.clips.with_clip(1, 0, 4 * BAR_TICKS, 0, BAR_TICKS);
    r.notes.add_notes(id, vec![NewNote { pitch: 40, start_tick: 0, length_ticks: 100, velocity: 90 }]).unwrap();
    r.play_from(0);
    r.press(1, 60, 100.0, Some(300.0));
    r.recorder.poll_once();
    r.press(1, 64, f64::from(BAR_TICKS) + 500.0, Some(f64::from(BAR_TICKS) + 700.0));
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.notes.of(id), [(40, 0, 100), (64, 500, 200)]);
    assert_eq!(r.recorder.status().take_notes, 1);
}

#[test]
fn held_note_is_closed_at_stop_and_cut_at_loop_end() {
    let r = rig();
    let id = r.clips.with_clip(1, 0, BAR_TICKS, 0, BAR_TICKS);
    r.play_from(0);
    r.press(1, 60, 3_000.0, None);
    r.recorder.poll_once();
    *r.transport.position.lock().unwrap() = 9_000;
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.notes.of(id), [(60, 3_000, 840)], "cut at the loop end");
}

#[test]
fn occupied_place_drops_the_take_and_logs_the_reason_once() {
    let r = rig();
    r.clips.refuse_new.store(true, Ordering::SeqCst);
    r.play_from(0);
    r.press(1, 60, 100.0, Some(300.0));
    r.press(1, 62, 400.0, Some(600.0));
    r.recorder.poll_once();
    r.stop();
    assert!(r.clips.clips().is_empty());
    assert_eq!(r.recorder.status().take_notes, 0);
    assert_eq!(*r.history.begun.lock().unwrap(), *r.history.ended.lock().unwrap(), "the group is closed");
    let entries = r.log.recent(10);
    assert_eq!(entries.len(), 1, "one failure is one entry, not one per note");
    assert_eq!((entries[0].source.as_str(), entries[0].level), ("record", LogLevel::Error));
    assert!(entries[0].message.contains("could not create a clip"));
}

#[test]
fn a_clip_deleted_during_recording_is_normal_and_not_logged() {
    let r = rig();
    let id = r.clips.with_clip(1, 0, BAR_TICKS, 0, BAR_TICKS);
    r.play_from(0);
    r.press(1, 60, 100.0, Some(300.0));
    r.recorder.poll_once();
    r.clips.clips.lock().unwrap().clear();
    r.notes.fail_with_not_found(id);
    r.press(1, 62, 400.0, Some(600.0));
    r.recorder.poll_once();
    r.stop();
    assert!(r.log.recent(10).is_empty(), "not found is an expected case");
}

#[test]
fn track_without_instrument_is_ignored() {
    let r = rig();
    r.clips.audio_tracks.lock().unwrap().push(2);
    let audio = clip(7, 2, 0, BAR_TICKS, 0, ClipContent::audio(1));
    r.clips.clips.lock().unwrap().push(audio);
    let music = r.clips.with_clip(1, 0, BAR_TICKS, 0, BAR_TICKS);
    r.play_from(0);
    r.press(2, 60, 100.0, Some(300.0));
    r.press(1, 62, 100.0, Some(300.0));
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.clips.clips().len(), 2, "no clip is created on an audio track");
    assert_eq!(r.notes.of(music), [(62, 100, 200)]);
    assert_eq!(r.recorder.status().take_notes, 1);
}

#[test]
fn quantize_is_applied_and_validated() {
    let r = rig();
    r.recorder.set_quantize(240).unwrap();
    assert!(matches!(r.recorder.set_quantize(BAR_TICKS + 1), Err(e) if e.is(record::QUANTIZE_OUT_OF_RANGE)));
    let id = r.clips.with_clip(1, 0, BAR_TICKS, 0, BAR_TICKS);
    r.play_from(0);
    r.press(1, 60, 250.0, Some(700.0));
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.notes.of(id), [(60, 240, 480)]);
    assert_eq!(r.recorder.status().quantize_ticks, 240);
}

/// Regression: the clip used to be created by a 4/4 bar even when the project had another signature.
#[test]
fn a_new_clip_starts_on_the_bar_of_the_current_time_signature() {
    let r = rig();
    r.tempo.set_time_signature(TimeSignature::new(3, 4).unwrap()).unwrap();
    r.play_from(3_000); // a 3/4 bar is 2880 ticks: the cursor is in the second bar
    r.press(1, 60, 3_000.0, Some(3_100.0));
    r.recorder.poll_once();
    r.stop();
    assert_eq!(r.clips.clips()[0].start_tick, 2_880);
}

/// Regression: a failure to remove notes of the previous pass used to be lost silently, leaving both versions.
#[test]
fn a_failed_replacement_of_an_earlier_pass_is_logged() {
    let r = rig();
    r.clips.with_clip(1, 0, 4 * BAR_TICKS, 0, BAR_TICKS);
    r.notes.refuse_remove.store(true, Ordering::SeqCst);
    r.play_from(0);
    r.press(1, 60, 100.0, Some(300.0));
    r.recorder.poll_once();
    r.press(1, 64, f64::from(BAR_TICKS) + 500.0, Some(f64::from(BAR_TICKS) + 700.0));
    r.recorder.poll_once();
    r.stop();
    let entries = r.log.recent(10);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].message.contains("previous pass"), "{}", entries[0].message);
}

#[test]
fn controllers_go_into_the_take_clip_lane_in_one_undo_step() {
    let r = rig();
    let id = r.clips.with_clip(1, 0, 4 * BAR_TICKS, 0, BAR_TICKS);
    r.play_from(0);
    let bend = ControllerKind::PitchBend;
    r.turn(1, bend, 0.5, 100.0);
    r.turn(1, bend, 0.5, 150.0); // the same value: no point needed
    r.turn(1, bend, -0.25, f64::from(BAR_TICKS) + 300.0); // the second pass replaces the first
    r.recorder.poll_once();
    r.stop();
    let lanes = r.notes.lanes.lock().unwrap().get(&id).cloned().unwrap();
    assert_eq!(lanes.len(), 1);
    assert_eq!(lanes[0].kind, bend);
    assert_eq!(lanes[0].points, [ControllerPoint { tick: 300, value: -0.25 }]);
    assert_eq!((*r.history.begun.lock().unwrap(), *r.history.ended.lock().unwrap()), (1, 1));
}

#[test]
fn a_controller_alone_creates_the_take_clip() {
    let r = rig();
    r.play_from(0);
    r.turn(2, ControllerKind::Cc { number: 64 }, 1.0, 10.0);
    r.stop();
    let clips = r.clips.clips();
    assert_eq!(clips.len(), 1);
    let lanes = r.notes.lanes.lock().unwrap().get(&clips[0].id).cloned().unwrap();
    assert_eq!(lanes[0].points, [ControllerPoint { tick: 10, value: 1.0 }]);
}

/// Error of a fake method these tests never call.
fn unused() -> ContractError {
    general::INTERNAL.with("details", "not used by these tests")
}
