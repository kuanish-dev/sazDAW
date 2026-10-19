//! The engine on the audio thread side: the segment conductor. Takes commands and live notes, picks up the snapshot,
//! hands out the work (notes to instruments, automation to the player, sound to the mixer) and mixes the master into
//! the device buffer. Everything to be freed goes to the control side through the release queue (`GarbageBin`).

use std::sync::Arc;
use std::time::Instant;

use daw_contracts::{
    AudioBlock, AudioProcessor, AudioRenderer, BlockTiming, DspCommand, EngineSnapshot, LiveMessage, NoteEvent, RecordedEvent,
    StreamConfig, TempoMap, TransportClock,
};
use rtrb::{Consumer, Producer};

use crate::SEGMENT_FRAMES;
use crate::automation::AutomationPlayer;
use crate::controllers::ControllerEvent;
use crate::instruments::InstrumentRack;
use crate::link::{AudioEnd, EngineLink, Garbage, GarbageBin, LiveEvent, Loss};
use crate::mixer::GraphMixer;
use crate::monitor::InputListener;
use crate::note_effects::{NoteEffectRack, SegmentNotes};
use crate::playback::Playback;
use crate::preview::PreviewVoice;

/// Load smoothing: the share of the new block in the reading (the rest is the previous value), so the number does not jump.
const LOAD_SMOOTHING: f32 = 0.1;

pub struct Engine {
    link: Arc<EngineLink>,
    commands: Consumer<DspCommand>,
    bin: GarbageBin,
    live: Consumer<LiveEvent>,
    recorded: Producer<RecordedEvent>,
    clock: Box<dyn TransportClock>,
    /// The tempo map while there is no snapshot yet (preallocated: it cannot be created in the audio thread).
    no_snapshot_map: TempoMap,
    post_processors: Vec<Box<dyn AudioProcessor>>,
    config: StreamConfig,

    snapshot: Option<Arc<EngineSnapshot>>,
    /// The snapshot the mixer tables were built from and the params were sent by; `None` means send them all again.
    applied: Option<Arc<EngineSnapshot>>,
    /// DSP objects came or went: they must be bound to nodes (new ones get their params).
    rebind: bool,

    instruments: InstrumentRack,
    /// Track MIDI effects: the segment notes pass through them before the instruments.
    note_effects: NoteEffectRack,
    /// The segment notes: played ones (live and from clips) and the ones the instruments will get.
    notes: SegmentNotes,
    mixer: GraphMixer,
    playback: Playback,
    preview: PreviewVoice,
    listener: InputListener,
    automation: AutomationPlayer,
    /// The total compensation latency (frames): the master lags the transport position by this much.
    latency: u32,
    load: f32,
}

impl Engine {
    pub fn new(end: AudioEnd, clock: Box<dyn TransportClock>, post_processors: Vec<Box<dyn AudioProcessor>>) -> Self {
        Self {
            link: end.link,
            commands: end.commands,
            bin: end.bin,
            live: end.live,
            recorded: end.recorded,
            clock,
            no_snapshot_map: TempoMap::default(),
            post_processors,
            config: StreamConfig::DEFAULT,
            snapshot: None,
            applied: None,
            rebind: false,
            instruments: InstrumentRack::new(),
            note_effects: NoteEffectRack::new(),
            notes: SegmentNotes::new(),
            mixer: GraphMixer::new(),
            playback: Playback::new(),
            preview: PreviewVoice::default(),
            listener: InputListener::new(),
            automation: AutomationPlayer::new(),
            latency: 0,
            load: 0.0,
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                DspCommand::AddInstrument { track, instrument } => {
                    if let Some(old) = self.instruments.add(track, instrument) {
                        self.bin.throw(Garbage::Instrument(old));
                    }
                    self.rebind = true;
                }
                DspCommand::RemoveInstrument { track } => {
                    if let Some(old) = self.instruments.remove(track) {
                        self.bin.throw(Garbage::Instrument(old));
                    }
                    // The latency of a removed instrument no longer takes part in compensation.
                    self.rebind = true;
                }
                DspCommand::SetNoteEffects { track, chain } => {
                    if let Some(old) = self.note_effects.set(track, chain) {
                        self.bin.throw(Garbage::NoteEffects(old));
                    }
                    // The bypass and params of the new chain come from the snapshot.
                    self.rebind = true;
                }
                DspCommand::AddEffect { node, effect } => {
                    if let Some(old) = self.mixer.add_effect(node, effect) {
                        self.bin.throw(Garbage::Effect(old));
                    }
                    self.rebind = true;
                }
                DspCommand::RemoveEffect { node } => {
                    if let Some(old) = self.mixer.remove_effect(node) {
                        self.bin.throw(Garbage::Effect(old));
                    }
                    self.rebind = true;
                }
                DspCommand::PreviewPlay { data, from, to, looping } => {
                    let old = self.preview.start(data, from, to, looping);
                    self.bin.throw_sample(old);
                }
                DspCommand::SetLiveInput { input } => {
                    if let Some(old) = self.listener.replace(input) {
                        self.bin.throw(Garbage::LiveInput(old));
                    }
                }
                DspCommand::PreviewStop => {
                    let old = self.preview.stop();
                    self.bin.throw_sample(old);
                }
            }
        }
    }

    /// Live notes go into the segment notes at frame 0 (then through the track MIDI effects), controllers go to the
    /// instruments right away; with recording armed and the transport playing they are also handed to the control side
    /// with the current tick: what is played gets recorded, not the MIDI effects output.
    fn drain_live(&mut self, timing: &BlockTiming) {
        while let Ok(event) = self.live.pop() {
            match event.message {
                LiveMessage::Note { pitch, velocity } => {
                    self.notes.push(event.track, NoteEvent { frame: 0, pitch, velocity });
                }
                LiveMessage::Controller { kind, value } => {
                    self.instruments.controller(event.track, kind, value);
                }
            }
            if timing.playing && self.link.is_recording_armed() {
                // The musician plays along with what they hear, and they hear the master delayed by compensation.
                let tick = self.heard(timing).start_tick.max(0.0);
                let recorded = RecordedEvent { track: event.track, message: event.message, tick };
                // The recording queue is full: the event is lost, but the audio thread does not block; the loss shows in the log.
                if self.recorded.push(recorded).is_err() {
                    self.link.telemetry.count(Loss::RecordedEvent, 1);
                }
            }
        }
    }

    fn refresh_snapshot(&mut self) {
        let latest = {
            let guard = self.link.current_snapshot();
            guard.as_ref().filter(|s| Some(s.version) != self.snapshot.as_ref().map(|c| c.version)).cloned()
        };
        if let Some(latest) = latest {
            self.snapshot = Some(latest);
        }
        let changed = match (&self.snapshot, &self.applied) {
            (Some(current), Some(applied)) => !Arc::ptr_eq(current, applied),
            (current, _) => current.is_some(),
        };
        if changed || self.rebind {
            self.apply_snapshot();
        }
    }

    /// Builds the tables from the snapshot and sends the objects the params changed since the last applied snapshot.
    /// The previous snapshot is never freed here as the last owner: the control side's "graveyard" holds it.
    fn apply_snapshot(&mut self) {
        self.rebind = false;
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        let previous = self.applied.take();
        self.mixer.apply_snapshot(&snapshot, previous.as_deref(), &self.link.telemetry.meters);
        self.instruments.apply_snapshot(&snapshot, previous.as_deref(), &self.mixer);
        self.note_effects.apply_snapshot(&snapshot, previous.as_deref());
        let instruments = &self.instruments;
        let alignment = self.mixer.align_latency(|track| instruments.latency(track));
        self.latency = alignment.total;
        self.link.telemetry.set_latency(alignment.total);
        self.link.telemetry.count(Loss::LatencyClamped, u64::from(alignment.clamped));
        self.automation.forget_sent(&snapshot);
        self.applied = Some(snapshot);
    }

    /// The next snapshot sends all params to the objects: their previous values may have changed
    /// (automation, `prepare`).
    fn resend_all_params(&mut self) {
        self.applied = None;
    }

    /// The segment clip notes go into the segment notes by frame (sounding ones are remembered to release them on time).
    fn collect_clip_notes(&mut self, timing: &BlockTiming, frames: usize) {
        for index in 0..self.playback.events().len() {
            let event = self.playback.events()[index];
            if event.on {
                self.playback.note_started(&event);
            } else {
                self.playback.note_released(&event);
            }
            let velocity = if event.on { event.velocity } else { 0.0 };
            let frame = timing.frame_at(event.tick as f64).min(frames);
            self.notes.push(event.track, NoteEvent { frame, pitch: event.pitch, velocity });
        }
    }

    /// Plays the segment instruments in pieces between events: notes (after MIDI effects) and controllers in time order,
    /// a controller at the same frame goes before a note (a bend or the pedal must already apply to it).
    fn play_events(&mut self, timing: &BlockTiming, frames: usize) {
        self.collect_clip_notes(timing, frames);
        self.note_effects.process(&mut self.notes, timing);
        let mut cursor = 0;
        let (mut note, mut control) = (0, 0);
        loop {
            let next_note = self.notes.outgoing().get(note).copied();
            let next_control = self.playback.controller_events().get(control).copied().map(|c| (timing.frame_at(c.tick as f64), c));
            let (frame, take_control) = match (next_note, next_control) {
                (None, None) => break,
                (Some(n), Some((f, _))) => (f.min(n.event.frame), f <= n.event.frame),
                (Some(n), None) => (n.event.frame, false),
                (None, Some((f, _))) => (f, true),
            };
            let offset = frame.clamp(cursor, frames);
            self.instruments.render(&mut self.mixer, cursor, offset);
            cursor = offset;
            if let Some((_, ControllerEvent { track, kind, value, .. })) = next_control.filter(|_| take_control) {
                self.instruments.controller(track, kind, value);
                control += 1;
            } else if let Some(n) = next_note {
                self.instruments.note(n.track, n.event.pitch, n.event.velocity);
                note += 1;
            }
        }
        self.instruments.render(&mut self.mixer, cursor, frames);
    }

    fn process_segment(&mut self, timing: &BlockTiming, frames: usize) {
        self.mixer.clear(frames);
        self.notes.clear();
        self.drain_live(timing);
        let snapshot = self.snapshot.take();
        let sample_rate = self.config.sample_rate;

        let plan = self.playback.plan(snapshot.as_deref(), timing, sample_rate, &mut self.bin);
        if plan.release_all {
            self.instruments.all_notes_off();
            // The chase for the new position (if playing) comes as segment events, after the reset.
            self.instruments.reset_controllers();
            self.note_effects.release_all();
        }
        if self.automation.apply(snapshot.as_deref(), timing, &self.link.held, &mut self.mixer, &mut self.instruments) {
            self.resend_all_params();
        }

        // Starting playback stops the preview: a long sound from the browser must not drag on over it.
        if plan.started {
            let old = self.preview.stop();
            self.bin.throw_sample(old);
        }

        // Plugins that follow the beat get the transport position (not shifted by PDC, like the source notes).
        self.instruments.set_timing(timing);
        self.mixer.set_timing(timing);
        self.play_events(timing, frames);

        self.playback.render_audio(&mut self.mixer, timing, frames, sample_rate, &mut self.bin);
        // The input goes into the buffers of the listening audio tracks before the mixer: from there it goes through their fader and effects.
        let underrun = self.listener.render(&mut self.mixer, &self.link.monitor, frames, sample_rate);
        self.link.telemetry.count(Loss::MonitorUnderrun, u64::from(underrun));
        self.mixer.process(frames, &self.link.telemetry.meters, &self.link.telemetry.node_tap);
        // The live meter gets the mix as it is exported: before the preview voice, the mono check, the clamp and the
        // metronome. Playback from the song start starts its integration over where the start reaches the master.
        if plan.started && timing.start_tick < 1.0 {
            self.link.telemetry.meter_tap.mark_restart(self.latency);
        }
        self.link.telemetry.meter_tap.write(self.mixer.master(frames));
        if self.preview.is_playing() {
            let finished = self.preview.render(self.mixer.master_mut(frames), sample_rate);
            self.bin.throw_sample(finished);
        }
        // The visualizer gets the music with the preview, before the clamp and the metronome (`EngineStats::master_window`).
        self.link.telemetry.scope.write(self.mixer.master(frames));
        self.link.telemetry.set_preview_position(self.preview.position());
        // After the segment's captures: a lease that runs out with this segment still had it copied.
        self.link.telemetry.clock.advance(frames);
        self.snapshot = snapshot;
    }

    /// The segment time as it is heard in the master: earlier than the transport position by the compensation latency
    /// (the shift uses the segment tempo: the latency is a fraction of a second, a tempo change inside it barely matters).
    /// Post-processors play by it (the metronome stays in time with the delayed music), and recorded notes are placed by it.
    fn heard(&self, timing: &BlockTiming) -> BlockTiming {
        BlockTiming { start_tick: timing.start_tick - f64::from(self.latency) * timing.ticks_per_frame, ..*timing }
    }

    /// The segment master into the device frames: clamped to ±1, mono gets the middle of the channels. The mono check
    /// (`MonoCheck`) sums before the clamp, so what cancels in mono is heard cancelled rather than clipped first.
    fn write_master(&self, segment: &mut [f32], channels: usize) {
        let master = self.mixer.master(segment.len() / channels);
        let mono = self.link.is_mono();
        for (frame, target) in master.chunks_exact(2).zip(segment.chunks_exact_mut(channels)) {
            let (left, right) = if mono {
                let middle = f32::midpoint(frame[0], frame[1]);
                (middle, middle)
            } else {
                (frame[0], frame[1])
            };
            let (left, right) = (left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0));
            if channels == 1 {
                target[0] = f32::midpoint(left, right);
            } else {
                target[0] = left;
                target[1] = right;
            }
        }
    }
}

impl AudioRenderer for Engine {
    fn prepare(&mut self, config: StreamConfig) {
        self.config = config;
        self.link.telemetry.set_stream_config(config);
        self.mixer.prepare(config);
        self.instruments.prepare(config);
        for processor in &mut self.post_processors {
            processor.prepare(config);
        }
        // `prepare` may reset object params to defaults, so the project values are restored.
        self.resend_all_params();
    }

    fn render(&mut self, out: &mut [f32]) {
        let started = Instant::now();
        out.fill(0.0);
        self.drain_commands();
        self.refresh_snapshot();

        let channels = usize::from(self.config.channels.max(1));
        let total = out.len() / channels;
        let mut done = 0;
        while done < total {
            let wanted = (total - done).min(SEGMENT_FRAMES);
            // The tempo comes from the project map in the snapshot: the clock cuts segments at its points.
            let map = self.snapshot.as_ref().map_or(&self.no_snapshot_map, |snapshot| &snapshot.project.tempo_map);
            let timing = self.clock.advance(wanted, self.config.sample_rate, map);
            let frames = timing.frames.clamp(1, wanted);
            let timing = BlockTiming { frames, ..timing };
            self.process_segment(&timing, frames);

            let segment = &mut out[done * channels..(done + frames) * channels];
            self.write_master(segment, channels);
            let heard = self.heard(&timing);
            for processor in &mut self.post_processors {
                processor.process(&mut AudioBlock::new(&mut *segment, channels as u16), &heard);
            }
            done += frames;
        }

        let budget = total as f32 / self.config.sample_rate as f32;
        if budget > 0.0 {
            let used = started.elapsed().as_secs_f32() / budget;
            self.load = self.load * (1.0 - LOAD_SMOOTHING) + used * LOAD_SMOOTHING;
            self.link.telemetry.set_cpu_load(self.load);
        }
    }
}
