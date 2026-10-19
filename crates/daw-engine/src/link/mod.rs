//! The link between the control thread and the audio thread — how the app sees the engine: snapshots
//! (`SnapshotSink`), DSP commands (`DspSink`), live notes and recording (`LiveNoteSink`, `RecordSource`), input
//! monitoring (`InputMonitor`, the table is in `monitor.rs`), lanes held by live automation writing (`AutomationHold`,
//! `held.rs`), listening to the master in mono (`MonoCheck`) and telemetry (`EngineStats`, `NodeScope` and the live
//! meter's `MasterTap`, `telemetry.rs`).
//! The audio thread side has no locks: `rtrb` ring queues, `ArcSwap` and atomics; only the control ends of the queues
//! hold locks.

mod held;
mod monitor;
mod telemetry;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::{ArcSwapOption, Guard};
use daw_contracts::codes;
use daw_contracts::sync::lock;
use daw_contracts::{
    AutomationHold, BandGains, ContractError, ControllerKind, DspCommand, DspSink, Effect, EngineLatency, EngineLosses, EngineSnapshot,
    EngineStats, GainReduction, InputMonitor, Instrument, LaneId, LiveInput, LiveMessage, LiveNoteSink, MasterFrames, MasterTap,
    MasterWindow, Meter, MonitorStatus, MonoCheck, NodeCursor, NodeId, NodeScope, NodeWindow, RecordSource, RecordedEvent, SampleData,
    SnapshotSink, TrackId, TrackInput,
};

use crate::note_effects::Units;
use rtrb::{Consumer, Producer, RingBuffer};

pub use held::HeldLanes;
pub use monitor::MonitorTable;
pub use telemetry::{Loss, Telemetry};

const COMMAND_QUEUE: usize = 1024;
const GARBAGE_QUEUE: usize = 1024;
const LIVE_QUEUE: usize = 512;
const RECORD_QUEUE: usize = 4096;

/// What the audio thread hands back to the control thread to be freed.
pub enum Garbage {
    Instrument(Box<dyn Instrument>),
    Effect(Box<dyn Effect>),
    /// The previous MIDI effect chain of a track.
    NoteEffects(Units),
    /// A reference to a sound (preview, an audio clip voice): the last ownership must not end in the audio thread.
    Sample(Arc<SampleData>),
    /// The previous monitoring input (its queue is freed here).
    LiveInput(Box<dyn LiveInput>),
}

/// A live note or controller from the keyboard.
#[derive(Clone, Copy)]
pub struct LiveEvent {
    pub track: TrackId,
    pub message: LiveMessage,
}

pub struct EngineLink {
    snapshot: ArcSwapOption<EngineSnapshot>,
    /// Old snapshots wait here until the audio thread lets them go: the last reference is never freed in the audio thread.
    graveyard: Mutex<Vec<Arc<EngineSnapshot>>>,
    commands: Mutex<Producer<DspCommand>>,
    garbage: Mutex<Consumer<Garbage>>,
    live: Mutex<Producer<LiveEvent>>,
    /// Notes played while recording: the audio thread hands them to the recorder.
    recorded: Mutex<Consumer<RecordedEvent>>,
    armed: AtomicBool,
    /// The mono check (`MonoCheck`): the audio thread sums the master it sends to the device.
    mono: AtomicBool,
    pub(crate) monitor: MonitorTable,
    /// Lanes whose params are being written live: the automation player skips them.
    pub(crate) held: HeldLanes,
    pub(crate) telemetry: Telemetry,
}

/// The queue ends for the audio thread.
pub struct AudioEnd {
    pub link: Arc<EngineLink>,
    pub commands: Consumer<DspCommand>,
    pub bin: GarbageBin,
    pub live: Consumer<LiveEvent>,
    pub recorded: Producer<RecordedEvent>,
}

/// The audio end of the release queue: everything the audio thread lets go (replaced objects, finished sounds) is
/// freed on the control side.
pub struct GarbageBin {
    queue: Producer<Garbage>,
    link: Arc<EngineLink>,
}

impl GarbageBin {
    /// When the queue is full, losing the object is better than freeing it in the audio thread; the loss shows in the log.
    pub fn throw(&mut self, garbage: Garbage) {
        if let Err(rtrb::PushError::Full(item)) = self.queue.push(garbage) {
            std::mem::forget(item);
            self.link.telemetry.count(Loss::LeakedGarbage, 1);
        }
    }

    pub fn throw_sample(&mut self, data: Option<Arc<SampleData>>) {
        if let Some(data) = data {
            self.throw(Garbage::Sample(data));
        }
    }
}

impl EngineLink {
    pub fn create() -> AudioEnd {
        let (commands, command_rx) = RingBuffer::new(COMMAND_QUEUE);
        let (garbage_tx, garbage) = RingBuffer::new(GARBAGE_QUEUE);
        let (live, live_rx) = RingBuffer::new(LIVE_QUEUE);
        let (recorded_tx, recorded) = RingBuffer::new(RECORD_QUEUE);
        let link = Arc::new(EngineLink {
            snapshot: ArcSwapOption::empty(),
            graveyard: Mutex::new(Vec::new()),
            commands: Mutex::new(commands),
            garbage: Mutex::new(garbage),
            live: Mutex::new(live),
            recorded: Mutex::new(recorded),
            armed: AtomicBool::new(false),
            mono: AtomicBool::new(false),
            monitor: MonitorTable::new(),
            held: HeldLanes::new(),
            telemetry: Telemetry::new(),
        });
        let bin = GarbageBin { queue: garbage_tx, link: Arc::clone(&link) };
        AudioEnd { link, commands: command_rx, bin, live: live_rx, recorded: recorded_tx }
    }

    /// A fresh snapshot for the audio thread.
    pub(crate) fn current_snapshot(&self) -> Guard<Option<Arc<EngineSnapshot>>> {
        self.snapshot.load()
    }

    pub(crate) fn is_recording_armed(&self) -> bool {
        self.armed.load(Ordering::Relaxed)
    }

    pub(crate) fn is_mono(&self) -> bool {
        self.mono.load(Ordering::Relaxed)
    }

    fn push_live(&self, event: LiveEvent) {
        // The audio thread cannot keep up with the notes: the note is lost, but the caller does not wait; the loss shows in the log.
        if lock(&self.live).push(event).is_err() {
            self.telemetry.count(Loss::LiveNote, 1);
        }
    }
}

impl SnapshotSink for EngineLink {
    fn publish(&self, snapshot: Arc<EngineSnapshot>) {
        {
            let mut graveyard = lock(&self.graveyard);
            graveyard.retain(|old| Arc::strong_count(old) > 1);
            graveyard.push(Arc::clone(&snapshot));
        }
        self.snapshot.store(Some(snapshot));
    }
}

impl DspSink for EngineLink {
    /// Prepares the object for the current stream (`prepare` allocates memory or restarts the plugin, which has no
    /// place in the audio thread) and queues the command. A full queue is `audio::ENGINE_QUEUE_FULL`.
    fn send(&self, mut command: DspCommand) -> Result<(), ContractError> {
        self.collect_garbage();
        let config = self.telemetry.stream_config();
        match &mut command {
            DspCommand::AddInstrument { instrument, .. } => instrument.prepare(config),
            DspCommand::AddEffect { effect, .. } => effect.prepare(config),
            DspCommand::RemoveInstrument { .. }
            | DspCommand::RemoveEffect { .. }
            | DspCommand::SetNoteEffects { .. }
            | DspCommand::PreviewPlay { .. }
            | DspCommand::PreviewStop
            | DspCommand::SetLiveInput { .. } => {}
        }
        lock(&self.commands).push(command).map_err(|_| codes::audio::ENGINE_QUEUE_FULL.err())
    }

    fn collect_garbage(&self) {
        let mut garbage = lock(&self.garbage);
        while garbage.pop().is_ok() {}
    }
}

impl LiveNoteSink for EngineLink {
    fn note_on(&self, track: TrackId, pitch: u8, velocity: f32) {
        self.push_live(LiveEvent { track, message: LiveMessage::Note { pitch, velocity: velocity.clamp(0.01, 1.0) } });
    }

    fn note_off(&self, track: TrackId, pitch: u8) {
        self.push_live(LiveEvent { track, message: LiveMessage::Note { pitch, velocity: 0.0 } });
    }

    fn controller(&self, track: TrackId, kind: ControllerKind, value: f32) {
        let (min, max) = kind.range();
        let value = if value.is_finite() { value.clamp(min, max) } else { 0.0 };
        self.push_live(LiveEvent { track, message: LiveMessage::Controller { kind, value } });
    }
}

impl EngineStats for EngineLink {
    fn cpu_load(&self) -> f32 {
        self.telemetry.cpu_load()
    }

    fn meters(&self) -> Vec<Meter> {
        self.telemetry.meters.take()
    }

    fn gain_reductions(&self) -> Vec<GainReduction> {
        self.telemetry.meters.take_reductions()
    }

    fn band_gains(&self) -> Vec<BandGains> {
        self.telemetry.meters.take_bands()
    }

    fn preview_position(&self) -> Option<usize> {
        self.telemetry.preview_position()
    }

    fn losses(&self) -> EngineLosses {
        self.telemetry.losses()
    }

    fn latency(&self) -> EngineLatency {
        self.telemetry.latency()
    }

    fn master_window(&self) -> MasterWindow {
        let sample_rate = self.telemetry.stream_config().sample_rate;
        MasterWindow { samples: self.telemetry.scope.read(sample_rate), sample_rate }
    }
}

impl NodeScope for EngineLink {
    fn node_windows(&self, nodes: &[NodeId], seen: &[NodeCursor]) -> Vec<NodeWindow> {
        self.telemetry.node_tap.windows(nodes, seen, self.telemetry.stream_config().sample_rate)
    }
}

impl MasterTap for EngineLink {
    fn open_tap(&self, on: bool) {
        self.telemetry.meter_tap.open(on);
    }

    fn frames_after(&self, end: u64) -> MasterFrames {
        self.telemetry.meter_tap.frames_after(end, self.telemetry.stream_config().sample_rate)
    }
}

impl MonoCheck for EngineLink {
    fn set_mono(&self, on: bool) {
        self.mono.store(on, Ordering::Relaxed);
    }

    fn is_mono(&self) -> bool {
        EngineLink::is_mono(self)
    }
}

impl InputMonitor for EngineLink {
    fn set_track_inputs(&self, inputs: &[TrackInput]) {
        self.monitor.set(inputs);
    }

    fn monitor_status(&self) -> MonitorStatus {
        MonitorStatus {
            listening: self.monitor.listening(),
            input_rate: self.monitor.input_rate(),
            output_rate: self.telemetry.stream_config().sample_rate,
        }
    }
}

impl AutomationHold for EngineLink {
    fn hold(&self, lanes: &[LaneId]) {
        self.held.set(lanes);
    }
}

impl RecordSource for EngineLink {
    fn set_armed(&self, armed: bool) {
        self.armed.store(armed, Ordering::Relaxed);
    }

    fn is_armed(&self) -> bool {
        self.is_recording_armed()
    }

    fn drain(&self) -> Vec<RecordedEvent> {
        let mut recorded = lock(&self.recorded);
        let mut events = Vec::new();
        while let Ok(event) = recorded.pop() {
            events.push(event);
        }
        events
    }
}
