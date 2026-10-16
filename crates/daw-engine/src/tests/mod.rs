//! Engine tests through its contracts: the `Rig` bench (engine + transport + a project the test edits and publishes as
//! a snapshot) and fake instruments and effects that log what was done to them.

mod audio_clips;
mod automation;
mod comp;
mod controllers;
mod graph;
mod latency;
mod live;
mod monitor;
mod note_effects;
mod params;
mod preview;
mod processors;
mod scope;
mod sends;
mod stage;
mod timeline;
mod timing;
mod utility;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use daw_contracts::{
    AudioBlock, AudioProcessor, AudioRenderer, AutoPoint, AutomationLane, BAR_TICKS, Bpm, Clip, ClipAudio, ClipContent, ClipId,
    ControllerKind, ControllerLane, ControllerPoint, DspCommand, DspSink, Edge, Effect, EngineLosses, EngineSnapshot, EngineStats,
    Instrument, InstrumentSpec, LiveMessage, LiveNoteSink, MASTER, MAX_EDGES, MAX_NODES, MonitorMode, Node, NodeId, NodeKind, Note,
    NoteContent, ParamTarget, Params, Project, RecordSource, SampleData, SampleId, SampleRef, SnapshotSink, StreamConfig, Strip, TempoMap,
    TimeSignature, Track, TrackId, TransportControl,
};

use crate::{EngineLink, EngineParts, create_engine};

const SR: u32 = 48_000;
/// 120 BPM at 48 kHz: 0.04 ticks per frame, i.e. 25 frames per tick.
const FRAMES_PER_TICK: usize = 25;

type Log = Arc<Mutex<Vec<String>>>;

/// An instrument: while a note sounds, it adds a constant level of `0.5 * velocity`.
struct Dc {
    log: Log,
    rendered: usize,
    sounding: bool,
    velocity: f32,
}

impl Instrument for Dc {
    fn prepare(&mut self, _: StreamConfig) {}
    fn note_on(&mut self, note: u8, velocity: f32) {
        self.sounding = true;
        self.velocity = velocity;
        self.log.lock().unwrap().push(format!("on:{note}@{}", self.rendered));
    }
    fn note_off(&mut self, note: u8) {
        self.sounding = false;
        self.log.lock().unwrap().push(format!("off:{note}@{}", self.rendered));
    }
    fn all_notes_off(&mut self) {
        self.sounding = false;
        self.log.lock().unwrap().push("all_off".into());
    }
    fn controller(&mut self, kind: ControllerKind, value: f32) {
        let name = match kind {
            ControllerKind::PitchBend => "bend".to_string(),
            ControllerKind::Aftertouch => "at".to_string(),
            ControllerKind::Cc { number } => format!("cc{number}"),
        };
        self.log.lock().unwrap().push(format!("{name}={value}@{}", self.rendered));
    }
    fn set_param(&mut self, name: &str, value: f32) {
        self.log.lock().unwrap().push(format!("param:{name}={value}"));
    }
    fn render(&mut self, block: &mut AudioBlock) {
        for frame in 0..block.frames() {
            if self.sounding {
                block.add_to_frame(frame, 0.5 * self.velocity);
            }
        }
        self.rendered += block.frames();
    }
}

/// An effect: zeroes the signal if a sidechain is fed; otherwise doubles it.
struct DuckOrDouble;

impl Effect for DuckOrDouble {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, buffer: &mut [f32], sidechain: Option<&[f32]>) {
        let key_loud = sidechain.is_some_and(|k| k.iter().any(|v| v.abs() > 0.01));
        for sample in buffer {
            *sample *= if key_loud { 0.0 } else { 2.0 };
        }
    }
}

/// An effect: doubles the signal.
struct Double;

impl Effect for Double {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, buffer: &mut [f32], _: Option<&[f32]>) {
        for sample in buffer {
            *sample *= 2.0;
        }
    }
}

/// A silent effect: logs the params it receives (`fx:name=value`).
struct Knobs {
    log: Log,
}

impl Effect for Knobs {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, name: &str, value: f32) {
        self.log.lock().unwrap().push(format!("fx:{name}={value}"));
    }
    fn reset(&mut self) {}
    fn process(&mut self, _: &mut [f32], _: Option<&[f32]>) {}
}

struct Rig {
    renderer: Box<dyn AudioRenderer>,
    link: Arc<EngineLink>,
    transport: Arc<dyn TransportControl>,
    project: Project,
    version: u64,
    log: Log,
    samples: HashMap<SampleId, Arc<SampleData>>,
    clip_audio: HashMap<(ClipId, SampleId), ClipAudio>,
    /// If set, goes into the snapshot instead of the honest `Graph::order`.
    order: Option<Vec<NodeId>>,
}

impl Rig {
    fn new() -> Self {
        Self::with_post_processors(Vec::new())
    }

    fn with_post_processors(post_processors: Vec<Box<dyn AudioProcessor>>) -> Self {
        let (transport, clock) = daw_transport::create_transport();
        let EngineParts { mut renderer, link } = create_engine(clock, post_processors);
        renderer.prepare(StreamConfig { sample_rate: SR, channels: 2 });
        let mut project = Project::empty();
        project.tempo_map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        // Master volume 1.0 so the numbers in tests are round.
        project.graph.node_mut(MASTER).unwrap().kind = NodeKind::Master { volume: 1.0 };
        Self {
            renderer,
            link,
            transport,
            project,
            version: 0,
            log: Arc::new(Mutex::new(Vec::new())),
            samples: HashMap::new(),
            clip_audio: HashMap::new(),
            order: None,
        }
    }

    fn node_kind(&mut self, id: NodeId) -> &mut NodeKind {
        &mut self.project.graph.node_mut(id).unwrap().kind
    }

    fn add_node(&mut self, kind: NodeKind) -> NodeId {
        let id = self.project.alloc_id();
        self.project.graph.nodes.push(Node { id, name: format!("n{id}"), kind, x: 0.0, y: 0.0 });
        id
    }

    fn connect(&mut self, from: NodeId, to: NodeId, level: f32, sidechain: bool) {
        self.project.graph.check_edge(from, to, sidechain).unwrap();
        self.project.graph.edges.push(Edge { from, to, level, sidechain, pre_fader: false });
    }

    /// The fader of a bus or source: volume, pan, mute, solo.
    fn set_strip(&mut self, id: NodeId, new_volume: f32, new_pan: f32, new_mute: bool, new_solo: bool) {
        let strip = self.node_kind(id).strip_mut().expect("the node has a fader");
        *strip = Strip { volume: new_volume, pan: new_pan, mute: new_mute, solo: new_solo, ..Strip::default() };
    }

    /// A bus with volume `volume` whose output goes to the master.
    fn add_bus(&mut self, volume: f32) -> NodeId {
        let id = self.add_node(NodeKind::Bus { strip: Strip { volume, pan: 0.0, mute: false, solo: false, ..Strip::default() } });
        self.connect(id, MASTER, 1.0, false);
        id
    }

    /// An effect without connections: the test routes sound to and from it.
    fn add_fx(&mut self, bypass: bool, effect: Box<dyn Effect>) -> NodeId {
        let id = self.add_node(NodeKind::Effect { effect: "fx".into(), bypass, params: Params::new(), state: None });
        self.link.send(DspCommand::AddEffect { node: id, effect }).unwrap();
        id
    }

    /// An instrument track with an `Instrument` node whose output goes to `to`. Returns the node.
    fn add_track(&mut self, id: TrackId, to: NodeId) -> NodeId {
        let node = self.add_node(NodeKind::Instrument {
            track: id,
            strip: Strip { volume: 1.0, pan: 0.0, mute: false, solo: false, ..Strip::default() },
        });
        self.connect(node, to, 1.0, false);
        self.add_track_only(id);
        node
    }

    /// A track and its instrument, but without a node in the graph.
    fn add_track_only(&mut self, id: TrackId) {
        self.project.tracks.push(Track::new(id, format!("tr{id}"), Some(InstrumentSpec::new("dc", Params::new())), 60));
        let instrument = Box::new(Dc { log: Arc::clone(&self.log), rendered: 0, sounding: false, velocity: 1.0 });
        self.link.send(DspCommand::AddInstrument { track: id, instrument }).unwrap();
    }

    /// An audio track with an `Audio` node whose output goes to `to`.
    fn add_audio_track(&mut self, id: TrackId, to: NodeId) -> NodeId {
        self.project.tracks.push(Track::new(id, format!("au{id}"), None, 60));
        let node = self.add_node(NodeKind::Audio {
            track: id,
            strip: Strip { volume: 1.0, pan: 0.0, mute: false, solo: false, ..Strip::default() },
            monitor: MonitorMode::Off,
        });
        self.connect(node, to, 1.0, false);
        node
    }

    /// Sound `data` in the project under id `id`.
    fn add_sample(&mut self, id: SampleId, data: Arc<SampleData>) {
        self.samples.insert(id, data);
        self.project.samples.push(SampleRef { id, name: "s".into(), path: "x".into(), markers: Vec::new() });
    }

    fn add_clip(&mut self, track: TrackId, start: u32, length: u32, content: ClipContent) -> ClipId {
        let id = self.project.alloc_id();
        self.project.clips.push(Clip {
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
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        });
        id
    }

    /// A note clip; the content repeats every `loop_ticks`.
    fn note_clip(&mut self, track: TrackId, start: u32, length: u32, loop_ticks: u32) -> ClipId {
        let content = NoteContent { notes: Vec::new(), next_note_id: 1, loop_ticks, auto_length: false, controllers: Vec::new() };
        self.add_clip(track, start, length, ClipContent::Notes(content))
    }

    fn clip_mut(&mut self, clip: ClipId) -> &mut Clip {
        self.project.clips.iter_mut().find(|c| c.id == clip).unwrap()
    }

    fn note_in(&mut self, clip: ClipId, pitch: u8, start: u32, length: u32) {
        let ClipContent::Notes(content) = &mut self.clip_mut(clip).content else {
            panic!("not a note clip");
        };
        let id = content.next_note_id;
        content.next_note_id += 1;
        content.notes.push(Note { id, pitch, start_tick: start, length_ticks: length, velocity: 127 });
    }

    /// A note of the track's "main" clip: it is created on the first call, starts at zero,
    /// is very long and repeats one bar of content (like the former pattern).
    fn note(&mut self, track: TrackId, pitch: u8, start: u32, length: u32) {
        let existing = self.project.clips.iter().find(|c| c.track == track).map(|c| c.id);
        let clip = existing.unwrap_or_else(|| self.note_clip(track, 0, 10_000_000, BAR_TICKS));
        self.note_in(clip, pitch, start, length);
    }

    /// A controller lane in a note clip: `(tick, value)`.
    fn controller_in(&mut self, clip: ClipId, kind: ControllerKind, points: &[(u32, f32)]) {
        let ClipContent::Notes(content) = &mut self.clip_mut(clip).content else {
            panic!("not a note clip");
        };
        let points = points.iter().map(|&(tick, value)| ControllerPoint { tick, value }).collect();
        content.controllers.push(ControllerLane { kind, points });
    }

    fn clear_notes(&mut self) {
        for clip in &mut self.project.clips {
            if let ClipContent::Notes(content) = &mut clip.content {
                content.notes.clear();
            }
        }
    }

    fn publish(&mut self) {
        self.version += 1;
        self.link.publish(Arc::new(EngineSnapshot {
            version: self.version,
            project: self.project.clone(),
            order: self.order.clone().unwrap_or_else(|| self.project.graph.order()),
            samples: self.samples.clone(),
            clip_audio: self.clip_audio.clone(),
        }));
    }

    fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut buffer = vec![0.0; frames * 2];
        self.renderer.render(&mut buffer);
        buffer
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn clear_log(&self) {
        self.log.lock().unwrap().clear();
    }

    fn count(&self, prefix: &str) -> usize {
        self.log().iter().filter(|e| e.starts_with(prefix)).count()
    }

    fn left(buffer: &[f32], frame: usize) -> f32 {
        buffer[frame * 2]
    }

    fn right(buffer: &[f32], frame: usize) -> f32 {
        buffer[frame * 2 + 1]
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// Mono sound of one value.
fn constant_sample(frames: usize, value: f32) -> Arc<SampleData> {
    Arc::new(SampleData { sample_rate: SR, channels: 1, frames: vec![value; frames] })
}

/// Mono sound rising evenly from 0 to 1.
fn ramp_sample(frames: usize) -> Arc<SampleData> {
    Arc::new(SampleData { sample_rate: SR, channels: 1, frames: (0..frames).map(|i| i as f32 / frames as f32).collect() })
}
