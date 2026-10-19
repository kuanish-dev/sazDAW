//! The audio engine: project snapshots, handing DSP objects to the audio thread, live notes, meters, the master window
//! for the visualizer, a node's window for its editor and event recording.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    ClipAudio, ClipId, ContractError, ControllerKind, Effect, InputChannels, Instrument, LiveInput, NodeId, NoteEffect, NoteEffectId,
    Project, SampleData, SampleId, TrackId,
};

/// An immutable project snapshot for the audio thread.
/// The control side publishes a new snapshot after every edit.
pub struct EngineSnapshot {
    pub version: u64,
    pub project: Project,
    /// Processing order of graph nodes (`Graph::order`), computed on the control side so the engine walks it without
    /// sorting or allocating.
    pub order: Vec<NodeId>,
    pub samples: HashMap<SampleId, Arc<SampleData>>,
    /// Processed audio of clip takes with warp or pitch shift (`Clip::has_sound_processing`), keyed by (clip, take):
    /// in a comp each take has its own sound. Such a clip plays only from here; until the sound is computed there is no
    /// entry and that part of the clip is silent.
    pub clip_audio: HashMap<(ClipId, SampleId), ClipAudio>,
}

/// Where snapshots are published (implemented by the audio engine).
pub trait SnapshotSink: Send + Sync {
    fn publish(&self, snapshot: Arc<EngineSnapshot>);
}

/// A separate engine instance with its own transport (for example, for offline rendering).
pub struct EngineInstance {
    pub renderer: Box<dyn crate::AudioRenderer>,
    pub dsp: Arc<dyn DspSink>,
    pub snapshots: Arc<dyn SnapshotSink>,
    pub transport: Arc<dyn crate::TransportControl>,
}

/// Creates independent engine instances. Only `daw-bootstrap` provides the implementation.
pub trait EngineFactory: Send + Sync {
    fn create(&self) -> EngineInstance;
}

/// Commands that hand ready DSP objects to the audio thread.
/// Objects are created on the control thread so the realtime side never allocates.
pub enum DspCommand {
    AddInstrument {
        track: TrackId,
        instrument: Box<dyn Instrument>,
    },
    RemoveInstrument {
        track: TrackId,
    },
    /// The effect for an effect node of the graph.
    AddEffect {
        node: NodeId,
        effect: Box<dyn Effect>,
    },
    RemoveEffect {
        node: NodeId,
    },
    /// A track's whole MIDI effect chain in order, with effect ids from `Track::note_effects` (the engine uses them to
    /// find parameters and bypass in the snapshot); empty means no chain. The previous chain is replaced and the engine
    /// releases its notes.
    SetNoteEffects {
        track: TrackId,
        chain: Vec<(NoteEffectId, Box<dyn NoteEffect>)>,
    },
    /// Previews the sample region `[from, to)` (in frames) on a separate voice.
    PreviewPlay {
        data: Arc<SampleData>,
        from: usize,
        to: usize,
        looping: bool,
    },
    PreviewStop,
    /// The input for monitoring (`AudioCapture::take_live_input`); the previous one is sent back to be freed.
    SetLiveInput {
        input: Box<dyn LiveInput>,
    },
}

/// The audio thread's DSP object inbox. Job: deliver ready objects to the audio thread and take back what it returns
/// (replaced instruments and effects, finished sounds) so they are freed outside it.
pub trait DspSink: Send + Sync {
    fn send(&self, command: DspCommand) -> Result<(), ContractError>;
    /// Frees everything the audio thread returned. The audio thread never frees anything itself and the return queue
    /// is bounded, so the owner calls this regularly (telemetry ~30 times a second, export once per chunk): a full
    /// queue means leaked memory. A sink with nothing to return does nothing.
    fn collect_garbage(&self);
}

/// Live notes (MIDI keyboard, audition): played on a track on top of its clips.
pub trait LiveNoteSink: Send + Sync {
    fn note_on(&self, track: TrackId, pitch: u8, velocity: f32);
    fn note_off(&self, track: TrackId, pitch: u8);
    /// A live controller (pitch bend wheel, CC knob, pressure): goes to the track's instrument like
    /// `Instrument::controller`; the value is clamped to `ControllerKind::range`.
    fn controller(&self, track: TrackId, kind: ControllerKind, value: f32);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    pub node: NodeId,
    pub peak_left: f32,
    pub peak_right: f32,
}

/// The deepest gain reduction of an effect node since the previous read (`Effect::gain_reduction_db`), in dB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GainReduction {
    pub node: NodeId,
    pub db: f32,
}

/// The dynamic gain of each band of an effect node since the previous read (`Effect::band_gains_db`), in dB, lowest
/// band first: per band the value farthest from 0 dB, so a short cut or boost between two reads is not lost.
#[derive(Debug, Clone, PartialEq)]
pub struct BandGains {
    pub node: NodeId,
    pub db: Vec<f32>,
}

/// What the audio engine dropped to avoid blocking the audio thread. Counters only grow from engine creation; the
/// control side logs every increase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineLosses {
    /// Live notes (presses and releases) and controllers that did not fit into the engine queue.
    pub live_notes: u64,
    /// Notes and controllers played while recording that did not fit into the recording queue.
    pub recorded_events: u64,
    /// Objects that had to be left in memory for good because the free queue was full.
    pub leaked_garbage: u64,
    /// How many times plugin delay compensation (PDC) had to be cut short when applying a snapshot: a path needed more
    /// delay than the engine allows, or delay lines ran out. Such paths are then out of alignment.
    pub latency_clamped: u64,
    /// How many times input monitoring ran out of frames (the input fell behind the output): the track block had
    /// silence in it.
    pub monitor_underruns: u64,
}

/// Total engine output delay caused by plugin delay compensation (PDC): all timeline sound at the master is late
/// against the transport position by exactly this much (every path is aligned to the longest one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineLatency {
    pub frames: u32,
    /// Stream sample rate the frames are counted in (0 means the stream has not been prepared yet).
    pub sample_rate: u32,
}

impl EngineLatency {
    pub fn seconds(self) -> f64 {
        if self.sample_rate == 0 { 0.0 } else { f64::from(self.frames) / f64::from(self.sample_rate) }
    }
}

/// How long a read keeps a live capture going, seconds: the master window (`EngineStats::master_window`), a node's
/// window (`NodeScope::node_windows`) and the live master meter (`MasterMeter::reading`). There is no on/off switch:
/// reading is the request to keep capturing. So any number of readers (the interface, an AI client) share a capture
/// without turning it off under each other, and a reader that goes away without a word (a closed panel, a reloaded
/// window, a client that crashed) stops the copying by itself this long after its last read.
pub const CAPTURE_LEASE_SECONDS: f32 = 10.0;

/// How many frames of the master output `EngineStats::master_window` keeps: enough for a 2048-point spectrum and about
/// 46 ms of waveform at 44.1 kHz.
pub const MASTER_WINDOW_FRAMES: usize = 2048;

/// The latest stretch of the master output, for the visualizer in the interface (oscilloscope and spectrum).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterWindow {
    /// Mono samples (the middle of the two channels), oldest first: `MASTER_WINDOW_FRAMES`, or none when this read
    /// started the capture.
    pub samples: Vec<f32>,
    /// Stream sample rate the samples are in, Hz.
    pub sample_rate: u32,
}

/// Audio engine telemetry.
pub trait EngineStats: Send + Sync {
    /// Fraction of the block time spent rendering (0.0..1.0+).
    fn cpu_load(&self) -> f32;
    /// Bus and master peaks since the previous read (reset afterwards).
    fn meters(&self) -> Vec<Meter>;
    /// Gain reduction of the effect nodes that report it (compressor, gate, limiter) and were processed since the
    /// previous read (a bypassed effect is not); reading resets it.
    fn gain_reductions(&self) -> Vec<GainReduction>;
    /// Band gains of the effect nodes that report them (multiband compressor, dynamic EQ) and were processed since the
    /// previous read (a bypassed effect is not); reading resets them.
    fn band_gains(&self) -> Vec<BandGains>;
    /// Position of the preview voice in file frames; `None` when it is silent.
    fn preview_position(&self) -> Option<usize>;
    /// Losses since engine creation (see `EngineLosses`).
    fn losses(&self) -> EngineLosses;
    /// Total compensation delay (PDC) for the last applied snapshot; 0 frames when nothing adds latency.
    fn latency(&self) -> EngineLatency;
    /// The latest `MASTER_WINDOW_FRAMES` frames of the master output: after the master fader and the preview voice,
    /// before the ±1 clamp and the metronome (the visualizer shows the music, not the clicks). Reading keeps the
    /// capture going for `CAPTURE_LEASE_SECONDS` of the stream; while nobody reads, the audio thread copies nothing. A
    /// read that finds the capture stopped starts it and gets no samples; the frames before it started are silence.
    fn master_window(&self) -> MasterWindow;
}

/// How many frames of a node's sound `NodeScope` keeps per captured node: a 4096-point spectrum, whose bins (about 12 Hz
/// at 48 kHz) still tell apart the low end of the EQ graph, from about 85 ms of sound.
pub const NODE_WINDOW_FRAMES: usize = 4096;

/// How many nodes `NodeScope` captures at once: every effect along a long signal path plus an open editor. The engine
/// keeps this many pairs of windows preallocated, so capturing another node never allocates on the audio thread.
pub const MAX_WATCHED_NODES: usize = 16;

/// How far a reader has read a captured node's sound: the watch the frames came from and the end of the last frame it
/// has (`NodeWindow::watch` and `NodeWindow::end` of its previous reading).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeCursor {
    pub node: NodeId,
    pub watch: u32,
    pub end: u64,
}

/// The newest stretch of one captured node's sound, for the live picture in its editor or along a signal path: what
/// came into the node and what it sent on, frame by frame (the live visuals of the built-in effects).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeWindow {
    pub node: NodeId,
    /// Which watch of the node the frames belong to: a node whose capture starts (again: its lease ran out, or its slot
    /// went to another node) gets a new number, and its sound starts from silence. A reader's cursor counts only within
    /// the same watch.
    pub watch: u32,
    /// Frames captured since the watch began: the last sample of `input` and `output` is frame `end − 1` of it.
    pub end: u64,
    /// What came into the node, before it changed the sound: before the effect for an effect node (the same as
    /// `output` while it is bypassed), before the fader for sources, buses and the master. Mono samples (the middle of
    /// the two channels), oldest first: the frames after the reader's cursor, or the last `NODE_WINDOW_FRAMES` (silence
    /// before the watch began) for a reader that has none in this watch or fell further behind.
    pub input: Vec<f32>,
    /// What the node sent on, over the same frames as `input` (sample `i` of both is the same moment): after the effect
    /// (its input while bypassed), after the fader for sources, buses and the master (the master without the preview
    /// voice). Mono, oldest first, as many samples as `input`.
    pub output: Vec<f32>,
    /// Stream sample rate the samples are in, Hz.
    pub sample_rate: u32,
}

/// The live sound of graph nodes for the pictures that show them (implemented by the audio engine). Job: capture the
/// sound of the nodes the interface is showing.
///
/// Why the input as well as the output: most effect pictures compare the two. A distortion draws its output against
/// its input, a compressor, limiter or gate measures the input against its threshold (a closed gate's output is
/// silence), a delay lights its echo taps by the input that feeds them, and a reverb finds its tail as the output minus
/// the dry part of the input; none of this can be recovered from the output of a nonlinear or dynamic effect.
///
/// Why many nodes in one read and cursors: a signal path shows every effect along it live at once, and one batched read
/// per picture frame with only the new frames keeps the traffic near the audio rate itself rather than a whole window per
/// node per read.
pub trait NodeScope: Send + Sync {
    /// One window per node of `nodes` that is captured (see `NodeWindow`), in the order of `nodes`; a repeated id counts
    /// once. Reading a node keeps its capture going for `CAPTURE_LEASE_SECONDS` of the stream; while nobody reads a
    /// node, the audio thread copies nothing of it. A node not captured yet gets a slot and a new watch whose sound
    /// starts from silence: a free slot, else the one read longest ago, unless all `MAX_WATCHED_NODES` were read in this
    /// same call (then the node gets no window). For a node with a cursor in `seen` of its current watch, only the
    /// frames after the cursor's end (none when nothing new came); otherwise the last `NODE_WINDOW_FRAMES`. A node that
    /// is not in the graph is captured like any other and adds nothing to its window.
    fn node_windows(&self, nodes: &[NodeId], seen: &[NodeCursor]) -> Vec<NodeWindow>;
}

/// An event played live while recording. Its time is the transport tick at the start of the audio segment in which
/// the engine received it (accurate to about one segment, ~10 ms), minus the total compensation delay
/// (`EngineStats::latency`): the musician plays along with what they hear, and they hear the master that late.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordedEvent {
    pub track: TrackId,
    pub message: LiveMessage,
    pub tick: f64,
}

/// What was played live.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiveMessage {
    /// `velocity` 0 means the note was released.
    Note { pitch: u8, velocity: f32 },
    /// A controller (value within `ControllerKind::range`).
    Controller { kind: ControllerKind, value: f32 },
}

/// Source of recorded events (implemented by the audio engine).
pub trait RecordSource: Send + Sync {
    /// While recording is not armed, the engine does not collect events.
    fn set_armed(&self, armed: bool);
    fn is_armed(&self) -> bool;
    /// Takes the collected events in order. Events collected while armed wait here until drained, also after
    /// `set_armed(false)`: the last notes before a disarm are not lost.
    fn drain(&self) -> Vec<RecordedEvent>;
}

/// An audio track's input as the engine sees it: whether recording is armed and which input channels it listens to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackInput {
    pub track: TrackId,
    pub armed: bool,
    pub channels: InputChannels,
}

/// Current state of input monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonitorStatus {
    /// How many tracks should hear the input (mode `On`, or `Auto` with recording armed).
    pub listening: u32,
    /// Input sample rate (0 means the input is closed).
    pub input_rate: u32,
    /// Engine output sample rate.
    pub output_rate: u32,
}

impl MonitorStatus {
    /// Why tracks that should hear the input do not; `None` when they do (or no track is listening).
    pub fn problem(&self) -> Option<String> {
        if self.listening == 0 {
            return None;
        }
        if self.input_rate == 0 {
            return Some(crate::tr!(
                "input closed: monitoring is silent (no input device or no microphone access; see the log)",
                "вход закрыт: прослушивание молчит (нет устройства ввода или доступа к микрофону — см. журнал)"
            ));
        }
        (self.input_rate != self.output_rate).then(|| {
            crate::tr!(
                "input rate {} Hz differs from output rate {} Hz: monitoring is off (pick one device or one rate)",
                "частота входа {} Гц не равна частоте вывода {} Гц: прослушивание выключено (выберите одно устройство или одну частоту)",
                self.input_rate,
                self.output_rate
            )
        })
    }
}

/// Input monitoring (implemented by the engine). Job: know which audio tracks are armed and which input channels they
/// listen to, and report whether they can hear it. A track's monitoring mode (`MonitorMode`) lives in the project.
pub trait InputMonitor: Send + Sync {
    /// Replaces the whole input table; tracks not in it are unarmed and listen to `Stereo`.
    /// Entries beyond [`crate::MAX_TRACKS`] are ignored.
    fn set_track_inputs(&self, inputs: &[TrackInput]);
    fn monitor_status(&self) -> MonitorStatus;
}
