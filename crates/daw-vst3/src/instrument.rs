//! `Vst3Instrument`: a VST3 plugin as an engine instrument (`daw_contracts::Instrument`).
//!
//! `note_on`/`note_off`/`all_notes_off` and `set_param` only write to fixed-capacity queues
//! (`NoteQueue`, `PendingChanges`); they reach the plugin at the start of the next `render` (`IEventList`,
//! `IParameterChanges`). Plugin parameters are named `p<id>` and come in real units; they are converted to normalized
//! ones by the `ParamScales` table built at creation. Failures on the audio thread
//! (plugin refusal, bad numbers, queue overflow) are counted by `BlockFaults` and logged off the audio thread.
//! The plugin gets the transport time (`set_timing`) in the `ProcessContext` of each call (`PluginClock`).
//! MIDI controllers (pitch bend, CC, pressure) in VST3 are parameter changes the plugin named through
//! `IMidiMapping` (`MidiAssignments`): they go through the same `PendingChanges` queue as params, at offset 0 —
//! the engine plays the instrument in pieces between events, so the start of a piece is the controller's exact frame.
//! A controller the plugin does not listen to is dropped and counted in `BlockFaults`.

use std::sync::Arc;

use daw_contracts::{AppLog, AudioBlock, BlockTiming, ControllerKind, Instrument, Params, StreamConfig};

use crate::context::PluginClock;
use crate::faults::BlockFaults;
use crate::instance::PluginInstance;
use crate::midi_map::MidiAssignments;
use crate::notes::NoteQueue;
use crate::pending::PendingChanges;
use crate::runner::ProcessRunner;
use crate::scale::ParamScales;

pub struct Vst3Instrument {
    instance: PluginInstance,
    runner: ProcessRunner,
    pending: PendingChanges,
    /// Converts the project's real values to the plugin's normalized values (built here, on the control side).
    scales: ParamScales,
    notes: NoteQueue,
    /// Which plugin parameters listen to MIDI controllers (read at creation).
    midi: MidiAssignments,
    faults: BlockFaults,
    clock: PluginClock,
    /// `true` if `prepare` succeeded; otherwise `render` adds nothing.
    ready: bool,
    /// Plugin latency (`getLatencySamples`) read in `prepare`: the audio thread asks for it without calling the plugin.
    latency: u32,
}

// SAFETY: the COM pointers inside the instrument are not thread-safe by themselves, but the instrument is used by exactly
// one thread at a time (it is handed to the audio thread whole, like any `Box<dyn Instrument>`), and the plugin gets no
// references to the host that it could use from other threads: the host objects belong to this same instrument.
unsafe impl Send for Vst3Instrument {}

impl Vst3Instrument {
    /// An instrument with initial parameter values from the project (sent to the plugin in the first block).
    pub fn new(mut instance: PluginInstance, name: String, log: Arc<dyn AppLog>, params: &Params) -> Result<Self, String> {
        // Check the format right at creation: an instrument without a stereo f32 output cannot be used.
        instance.configure_instrument()?;
        let runner = ProcessRunner::new_instrument(instance.out_buses());
        let scales = instance.parameters().scales;
        let midi = instance.midi_assignments();
        let mut instrument = Self {
            instance,
            runner,
            pending: PendingChanges::default(),
            scales,
            notes: NoteQueue::default(),
            midi,
            faults: BlockFaults::new(name, log),
            clock: PluginClock::default(),
            ready: false,
            latency: 0,
        };
        for (name, value) in params {
            instrument.set_param(name, *value);
        }
        Ok(instrument)
    }
}

impl Instrument for Vst3Instrument {
    fn prepare(&mut self, config: StreamConfig) {
        // A repeated start restarts the plugin: it forgets the sounding notes, and so does the host.
        self.notes.reset();
        self.ready = self.instance.guarded(|instance| self.faults.start(instance, config.sample_rate));
        self.clock.start(f64::from(config.sample_rate));
        self.latency = if self.ready { self.instance.latency_samples() } else { 0 };
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        let queued = self.notes.note_on(note, velocity);
        self.faults.note(queued);
    }

    fn note_off(&mut self, note: u8) {
        self.notes.note_off(note);
    }

    fn all_notes_off(&mut self) {
        self.notes.all_notes_off();
    }

    fn controller(&mut self, kind: ControllerKind, value: f32) {
        match self.midi.change_for(kind, value) {
            Some((id, normalized)) => {
                let queued = self.pending.set(id, normalized);
                self.faults.param(queued);
            }
            None => self.faults.unmapped_controller(),
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let queued = self.pending.set_named(name, value, &self.scales);
        self.faults.param(queued);
    }

    fn render(&mut self, block: &mut AudioBlock) {
        if !self.ready {
            // The plugin is not started: nobody needs what accumulated, the queues must not pile up.
            self.notes.clear_events();
            self.pending.clear();
            return;
        }
        // A plugin refusal or bad numbers in its sound give silence; `BlockFaults` reports them to the log.
        let channels = block.channels();
        // "Plugin is processing" mark, for the crash report on the next start.
        let ok = self.instance.guarded(|instance| {
            self.runner.render_add(
                instance.processor(),
                &mut self.pending,
                &mut self.notes,
                &mut self.clock,
                block.as_mut_slice(),
                channels,
            )
        });
        self.faults.block(ok);
    }

    fn latency_samples(&self) -> u32 {
        self.latency
    }

    fn set_timing(&mut self, timing: &BlockTiming) {
        self.clock.set_timing(timing);
    }
}
