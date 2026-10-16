//! Live automation writer tests with a fake transport, tempo (4/4), automation lanes, history, engine hold and log.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use daw_contracts::codes::automation;
use daw_contracts::{
    AppLog, AutoPoint, AutomationClip, AutomationClipEdit, AutomationClipId, AutomationControl, AutomationHold, AutomationLane,
    AutomationWriteMode, AutomationWriter, Bpm, ClipDrawing, ContractError, DrawnClip, HistoryControl, HistoryMark, LaneId, LogEntry,
    ParamTarget, ShapeRequest, SignatureChange, TempoChange, TempoControl, TempoMap, TimeSignature, TouchEnd, TransportControl,
    TransportState,
};

use crate::{LiveAutomationWriter, WriterDeps};

struct FakeTransport {
    state: Mutex<TransportState>,
    position: AtomicU64,
}

impl TransportControl for FakeTransport {
    fn play(&self) {
        *self.state.lock().unwrap() = TransportState::Playing;
    }
    fn pause(&self) {
        *self.state.lock().unwrap() = TransportState::Paused;
    }
    fn stop(&self) {
        *self.state.lock().unwrap() = TransportState::Stopped;
        self.position.store(0, Ordering::SeqCst);
    }
    fn seek(&self, tick: u64) {
        self.position.store(tick, Ordering::SeqCst);
    }
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
    fn hold_song_loop(&self, _: bool) {}
    fn wrap_region(&self) -> Option<(u64, u64)> {
        None
    }
    fn state(&self) -> TransportState {
        *self.state.lock().unwrap()
    }
    fn position_ticks(&self) -> u64 {
        self.position.load(Ordering::SeqCst)
    }
}

struct FakeTempo;

impl TempoControl for FakeTempo {
    fn tempo_map(&self) -> TempoMap {
        TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR)
    }
    fn set_tempo_changes(&self, _: Vec<TempoChange>) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_signature_changes(&self, _: Vec<SignatureChange>) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_tempo_map(&self, _: TempoMap) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_tempo(&self, _: Bpm) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_time_signature(&self, _: TimeSignature) -> Result<(), ContractError> {
        Ok(())
    }
}

/// Lanes and clips without checks; a new clip has no points.
#[derive(Default)]
struct FakeLanes {
    lanes: Mutex<Vec<AutomationLane>>,
}

impl FakeLanes {
    fn with_clip<R>(&self, lane: LaneId, clip: AutomationClipId, f: impl FnOnce(&mut AutomationClip) -> R) -> Result<R, ContractError> {
        let mut lanes = self.lanes.lock().unwrap();
        let lane = lanes.iter_mut().find(|l| l.id == lane).ok_or_else(|| automation::LANE_NOT_FOUND.with("id", lane))?;
        let found = lane.clips.iter_mut().find(|c| c.id == clip);
        found.map(f).ok_or_else(|| automation::CLIP_NOT_FOUND.with("id", clip).with("lane", lane.id))
    }
}

impl AutomationControl for FakeLanes {
    fn lanes(&self) -> Vec<AutomationLane> {
        self.lanes.lock().unwrap().clone()
    }
    fn add_lane(&self, target: ParamTarget) -> Result<LaneId, ContractError> {
        let mut lanes = self.lanes.lock().unwrap();
        let id = 10 + lanes.len() as LaneId;
        lanes.push(AutomationLane { id, target, clips: Vec::new() });
        Ok(id)
    }
    fn remove_lane(&self, id: LaneId) -> Result<(), ContractError> {
        self.lanes.lock().unwrap().retain(|l| l.id != id);
        Ok(())
    }
    fn add_clip(&self, lane: LaneId, start_tick: u32, length_ticks: u32) -> Result<AutomationClipId, ContractError> {
        let mut lanes = self.lanes.lock().unwrap();
        let id = 100 + lanes.iter().map(|l| l.clips.len() as u32).sum::<u32>();
        let lane = lanes.iter_mut().find(|l| l.id == lane).ok_or_else(|| automation::LANE_NOT_FOUND.with("id", lane))?;
        lane.clips.push(AutomationClip::once(id, start_tick, length_ticks, Vec::new()));
        lane.clips.sort_by_key(|c| c.start_tick);
        Ok(id)
    }
    fn edit_clip(&self, lane: LaneId, clip: AutomationClipId, edit: AutomationClipEdit) -> Result<AutomationClip, ContractError> {
        self.with_clip(lane, clip, |c| {
            c.start_tick = edit.start_tick.unwrap_or(c.start_tick);
            c.length_ticks = edit.length_ticks.unwrap_or(c.length_ticks);
            c.loop_ticks = edit.loop_ticks.unwrap_or(c.loop_ticks);
            c.clone()
        })
    }
    fn duplicate_clip(&self, _: LaneId, _: AutomationClipId) -> Result<AutomationClipId, ContractError> {
        Ok(0)
    }
    fn remove_clip(&self, _: LaneId, _: AutomationClipId) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_points(&self, lane: LaneId, clip: AutomationClipId, points: Vec<AutoPoint>) -> Result<(), ContractError> {
        self.with_clip(lane, clip, |c| c.points = points)
    }
    fn draw_shape(&self, _: LaneId, _: AutomationClipId, _: ShapeRequest) -> Result<(), ContractError> {
        Ok(())
    }
    fn add_drawn_clip(&self, _: ParamTarget, _: u32, _: u32, _: ClipDrawing) -> Result<DrawnClip, ContractError> {
        Ok(DrawnClip { lane: 0, clip: 0 })
    }
}

#[derive(Default)]
struct FakeHistory {
    begun: AtomicU64,
    ended: AtomicU64,
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
        self.begun.fetch_add(1, Ordering::SeqCst);
    }
    fn end_group(&self) -> bool {
        self.ended.fetch_add(1, Ordering::SeqCst);
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

#[derive(Default)]
struct FakeHold(Mutex<Vec<LaneId>>);

impl AutomationHold for FakeHold {
    fn hold(&self, lanes: &[LaneId]) {
        *self.0.lock().unwrap() = lanes.to_vec();
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
    transport: Arc<FakeTransport>,
    lanes: Arc<FakeLanes>,
    history: Arc<FakeHistory>,
    hold: Arc<FakeHold>,
    writer: LiveAutomationWriter,
}

const VOLUME: ParamTarget = ParamTarget::NodeVolume { node: 1 };
const PAN: ParamTarget = ParamTarget::NodePan { node: 1 };

impl Rig {
    fn new(mode: AutomationWriteMode) -> Self {
        let transport = Arc::new(FakeTransport { state: Mutex::new(TransportState::Stopped), position: AtomicU64::new(0) });
        let lanes = Arc::new(FakeLanes::default());
        let history = Arc::new(FakeHistory::default());
        let hold = Arc::new(FakeHold::default());
        let writer = LiveAutomationWriter::start(WriterDeps {
            automation: lanes.clone(),
            history: history.clone(),
            transport: transport.clone(),
            tempo: Arc::new(FakeTempo),
            hold: hold.clone(),
            log: Arc::new(MemoryLog::default()),
        });
        writer.set_mode(mode);
        Self { transport, lanes, history, hold, writer }
    }

    /// Moves the play position and polls (the background thread also polls; this makes the step exact).
    fn at(&self, tick: u64) {
        self.transport.seek(tick);
        self.writer.poll_at(Instant::now() + Duration::from_secs(1));
    }

    fn clips(&self, target: &ParamTarget) -> Vec<AutomationClip> {
        let lanes = self.lanes.lanes();
        lanes.into_iter().find(|l| l.target == *target).expect("a lane for the target").clips
    }

    /// The first clip's pattern points.
    fn points(&self, target: &ParamTarget) -> Vec<(u32, f32)> {
        self.clips(target)[0].points.iter().map(|p| (p.tick, p.value)).collect()
    }

    /// A lane with one clip.
    fn clip(&self, target: ParamTarget, start: u32, length: u32, loop_ticks: u32, points: Vec<AutoPoint>) {
        let lane = self.lanes.add_lane(target).unwrap();
        let clip = self.lanes.add_clip(lane, start, length).unwrap();
        self.lanes.edit_clip(lane, clip, AutomationClipEdit { loop_ticks: Some(loop_ticks), ..AutomationClipEdit::default() }).unwrap();
        self.lanes.set_points(lane, clip, points).unwrap();
    }

    fn held(&self) -> Vec<LaneId> {
        self.hold.0.lock().unwrap().clone()
    }

    fn stop(&self) {
        self.transport.stop();
        self.writer.poll_at(Instant::now());
    }
}

#[test]
fn nothing_is_written_when_off_or_not_playing() {
    let rig = Rig::new(AutomationWriteMode::Off);
    rig.transport.play();
    rig.writer.touch(VOLUME, 0.5, TouchEnd::Release).unwrap();
    rig.writer.set_mode(AutomationWriteMode::Latch);
    rig.transport.stop();
    rig.writer.touch(VOLUME, 0.5, TouchEnd::Release).unwrap();
    assert!(rig.lanes.lanes().is_empty());
    assert_eq!(rig.history.begun.load(Ordering::SeqCst), 0);
    assert!(matches!(rig.writer.touch(VOLUME, f32::NAN, TouchEnd::Release), Err(e) if e.is(automation::POINT_NOT_A_NUMBER)));
}

#[test]
fn touch_writes_while_held_and_stops_on_release() {
    let rig = Rig::new(AutomationWriteMode::Touch);
    rig.transport.play();
    rig.transport.seek(1_000);
    rig.writer.touch(VOLUME, 0.2, TouchEnd::Release).unwrap();
    assert_eq!(rig.held().len(), 1, "the new lane is held");
    rig.at(1_500);
    rig.writer.touch(VOLUME, 0.8, TouchEnd::Release).unwrap();
    rig.at(2_000);
    rig.writer.release(&VOLUME);
    assert!(rig.held().is_empty(), "released: the lane plays again");
    assert_eq!(rig.points(&VOLUME), [(1_000, 0.2), (1_500, 0.8), (2_000, 0.8)]);
    let clip = &rig.clips(&VOLUME)[0];
    assert_eq!((clip.start_tick, clip.length_ticks, clip.loop_ticks), (0, 2_880, 2_880), "from the bar start, whole beats");

    rig.at(3_000);
    assert_eq!(rig.points(&VOLUME).last(), Some(&(2_000, 0.8)), "nothing is written after the release");
    rig.stop();
    assert_eq!((rig.history.begun.load(Ordering::SeqCst), rig.history.ended.load(Ordering::SeqCst)), (1, 1), "one undo step");
}

#[test]
fn latch_keeps_writing_the_last_value_until_stop() {
    let rig = Rig::new(AutomationWriteMode::Latch);
    rig.transport.play();
    rig.writer.touch(PAN, -0.5, TouchEnd::Release).unwrap();
    rig.writer.release_all();
    rig.at(4_000);
    assert_eq!(rig.held().len(), 1, "latch ignores release");
    rig.stop();
    assert_eq!(rig.points(&PAN), [(0, -0.5), (4_000, -0.5)], "written up to the last position before stop");
    assert_eq!(rig.clips(&PAN)[0].length_ticks, 4_800, "the clip grew with the writing");
    assert!(rig.held().is_empty());
    assert_eq!(rig.history.ended.load(Ordering::SeqCst), 1);
}

#[test]
fn writing_overwrites_old_points_over_the_written_span_only() {
    let rig = Rig::new(AutomationWriteMode::Touch);
    rig.clip(VOLUME, 0, 10_000, 10_000, vec![AutoPoint::new(0, 1.0), AutoPoint::new(1_500, 0.0), AutoPoint::new(10_000, 1.0)]);
    rig.transport.play();
    rig.transport.seek(1_000);
    rig.writer.touch(VOLUME, 0.5, TouchEnd::Release).unwrap();
    rig.at(2_000);
    rig.writer.release(&VOLUME);
    let ticks: Vec<u32> = rig.points(&VOLUME).iter().map(|p| p.0).collect();
    assert_eq!(ticks, [0, 999, 1_000, 2_000, 2_001, 10_000], "the 1500 point is gone, the joins keep the old curve");
}

#[test]
fn a_knob_without_release_stops_writing_after_the_idle_timeout_in_touch_mode() {
    let rig = Rig::new(AutomationWriteMode::Touch);
    rig.transport.play();
    rig.writer.touch(VOLUME, 0.3, TouchEnd::Idle).unwrap();
    rig.writer.release_all();
    assert_eq!(rig.held().len(), 1, "release_all is for released controls only");
    rig.at(500);
    assert!(rig.held().is_empty(), "idle for longer than the timeout");
    rig.stop();
}

#[test]
fn a_loop_jump_starts_a_new_stretch_and_mode_off_closes_the_pass() {
    let rig = Rig::new(AutomationWriteMode::Latch);
    rig.transport.play();
    rig.transport.seek(2_000);
    rig.writer.touch(VOLUME, 0.4, TouchEnd::Release).unwrap();
    rig.at(3_000);
    rig.at(500); // the loop jumped back
    rig.at(800);
    rig.writer.set_mode(AutomationWriteMode::Off);
    let points = rig.points(&VOLUME);
    assert!(points.contains(&(500, 0.4)) && points.contains(&(800, 0.4)) && points.contains(&(3_000, 0.4)), "{points:?}");
    assert_eq!(rig.history.ended.load(Ordering::SeqCst), 1, "switching off ends the pass");
    assert!(rig.held().is_empty());
}

#[test]
fn writing_into_a_looped_clip_changes_its_pattern_and_the_seam_starts_a_new_stretch() {
    let rig = Rig::new(AutomationWriteMode::Latch);
    rig.clip(VOLUME, 0, 4_000, 1_000, vec![AutoPoint::new(0, 0.0)]);
    rig.transport.play();
    rig.transport.seek(1_200);
    rig.writer.touch(VOLUME, 0.7, TouchEnd::Release).unwrap();
    rig.at(1_800);
    rig.at(2_100); // the next repeat: pattern tick 100
    rig.at(2_300);
    rig.writer.set_mode(AutomationWriteMode::Off);
    let ticks: Vec<u32> = rig.points(&VOLUME).iter().map(|p| p.0).collect();
    assert_eq!(ticks, [0, 99, 100, 300, 301, 800, 801], "pattern ticks of both stretches");
    let clips = rig.clips(&VOLUME);
    assert_eq!((clips.len(), clips[0].length_ticks, clips[0].loop_ticks), (1, 4_000, 1_000), "the clip keeps its shape");
}

#[test]
fn a_growing_clip_stops_at_the_next_clip_which_takes_over() {
    let rig = Rig::new(AutomationWriteMode::Latch);
    rig.clip(VOLUME, 7_680, 320, 320, Vec::new());
    rig.transport.play();
    rig.transport.seek(5_000);
    rig.writer.touch(VOLUME, 0.6, TouchEnd::Release).unwrap();
    rig.at(7_000);
    rig.at(7_700);
    rig.stop();
    let clips = rig.clips(&VOLUME);
    assert_eq!(clips.len(), 2);
    assert_eq!((clips[0].start_tick, clips[0].length_ticks), (3_840, 3_840), "from the bar start up to the next clip");
    assert!(clips[0].points.contains(&AutoPoint::new(1_160, 0.6)), "{:?}", clips[0].points);
    assert!(clips[1].points.contains(&AutoPoint::new(20, 0.6)), "the latched value goes on in the next clip: {:?}", clips[1].points);
}
