//! Runs blocks through the plugin without allocations.
//!
//! The entity's job is to take the interleaved stereo buffer, run it through `IAudioProcessor::process`
//! and return the result. Everything needed (planar buffers, pointer arrays, `ProcessData`, parameter change
//! lists, transport context) is created once in `new`; `run` only rewrites numbers.

use vst3::ComPtr;
use vst3::ComWrapper;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, IAudioProcessor, IAudioProcessorTrait, IEventList, IParameterChanges, ProcessContext,
    ProcessData,
};
use vst3::Steinberg::kResultOk;

use crate::context::{PluginClock, empty_context};
use crate::convert::{add_planar, deinterleave, interleave};
use crate::host_changes::ParamChanges;
use crate::host_events::EventList;
use crate::instance::{PROCESS_MODE, SAMPLE_SIZE};
use crate::notes::NoteQueue;
use crate::pending::PendingChanges;

/// Maximum frames per plugin call; a larger block is processed in pieces.
pub const MAX_FRAMES: usize = 8192;

pub struct ProcessRunner {
    in_left: Vec<f32>,
    in_right: Vec<f32>,
    out_left: Vec<f32>,
    out_right: Vec<f32>,
    _in_pointers: Box<[*mut f32; 2]>,
    _out_pointers: Box<[*mut f32; 2]>,
    _inputs: Vec<AudioBusBuffers>,
    _outputs: Vec<AudioBusBuffers>,
    /// Transport time for the plugin (`ProcessData::processContext` points here).
    context: Box<ProcessContext>,
    data: ProcessData,
    changes_in: ComWrapper<ParamChanges>,
    _changes_out: ComWrapper<ParamChanges>,
    /// Notes for the plugin; only an instrument has them (an effect's `inputEvents` is `null`).
    events: Option<ComWrapper<EventList>>,
}

fn bus(channels: i32, pointers: *mut *mut f32) -> AudioBusBuffers {
    AudioBusBuffers { numChannels: channels, silenceFlags: 0, __field0: AudioBusBuffers__type0 { channelBuffers32: pointers } }
}

/// Plugin buses: the main one (index 0) with stereo buffers, the others off and without buffers.
fn bus_list(count: usize, main_pointers: *mut *mut f32) -> Vec<AudioBusBuffers> {
    (0..count).map(|index| if index == 0 { bus(2, main_pointers) } else { bus(0, std::ptr::null_mut()) }).collect()
}

impl ProcessRunner {
    /// Effect runner: stereo in and out, no events.
    pub fn new(in_buses: usize, out_buses: usize) -> Self {
        Self::build(in_buses, out_buses, None)
    }

    /// Instrument runner: no input (`numInputs = 0`), stereo output, notes come as events.
    pub fn new_instrument(out_buses: usize) -> Self {
        Self::build(0, out_buses, Some(EventList::new()))
    }

    fn build(in_buses: usize, out_buses: usize, events: Option<ComWrapper<EventList>>) -> Self {
        let mut in_left = vec![0.0; MAX_FRAMES];
        let mut in_right = vec![0.0; MAX_FRAMES];
        let mut out_left = vec![0.0; MAX_FRAMES];
        let mut out_right = vec![0.0; MAX_FRAMES];
        let mut in_pointers = Box::new([in_left.as_mut_ptr(), in_right.as_mut_ptr()]);
        let mut out_pointers = Box::new([out_left.as_mut_ptr(), out_right.as_mut_ptr()]);
        let mut inputs = bus_list(in_buses, in_pointers.as_mut_ptr());
        let mut outputs = bus_list(out_buses, out_pointers.as_mut_ptr());
        let changes_in = ParamChanges::new();
        let changes_out = ParamChanges::new();
        let mut context = Box::new(empty_context());
        let pointer_of = |changes: &ComWrapper<ParamChanges>| {
            changes.as_com_ref::<IParameterChanges>().map_or(std::ptr::null_mut(), |interface| interface.as_ptr())
        };
        let data = ProcessData {
            processMode: PROCESS_MODE,
            symbolicSampleSize: SAMPLE_SIZE,
            numSamples: 0,
            numInputs: in_buses as i32,
            numOutputs: out_buses as i32,
            inputs: inputs.as_mut_ptr(),
            outputs: outputs.as_mut_ptr(),
            inputParameterChanges: pointer_of(&changes_in),
            outputParameterChanges: pointer_of(&changes_out),
            inputEvents: events
                .as_ref()
                .and_then(ComWrapper::as_com_ref::<IEventList>)
                .map_or(std::ptr::null_mut(), |interface| interface.as_ptr()),
            outputEvents: std::ptr::null_mut(),
            processContext: &raw mut *context,
        };
        Self {
            in_left,
            in_right,
            out_left,
            out_right,
            _in_pointers: in_pointers,
            _out_pointers: out_pointers,
            _inputs: inputs,
            _outputs: outputs,
            context,
            data,
            changes_in,
            _changes_out: changes_out,
            events,
        }
    }

    /// Processes `interleaved` in place. Accumulated parameter changes go to the plugin in the first piece,
    /// each piece's time comes from `clock`. `false` when the plugin refused to process some piece (it stays unchanged).
    pub fn run(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        pending: &mut PendingChanges,
        clock: &mut PluginClock,
        interleaved: &mut [f32],
    ) -> bool {
        let mut all_ok = true;
        for chunk in interleaved.chunks_mut(2 * MAX_FRAMES) {
            if chunk.len() >= 2 {
                all_ok &= self.run_chunk(processor, pending, clock, chunk);
            }
        }
        all_ok
    }

    fn run_chunk(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        pending: &mut PendingChanges,
        clock: &mut PluginClock,
        chunk: &mut [f32],
    ) -> bool {
        let frames = chunk.len() / 2;
        deinterleave(chunk, &mut self.in_left[..frames], &mut self.in_right[..frames]);
        if !self.call_process(processor, pending, clock, frames) {
            return false;
        }
        interleave(&self.out_left[..frames], &self.out_right[..frames], chunk);
        true
    }

    /// Instrument: the plugin **adds** sound into `block` (with `channels` channels per frame). Notes and parameter changes
    /// go to the plugin in the first piece. A block where the plugin produced `NaN`/`inf` is skipped whole (silence):
    /// a bad number must not reach the speakers. `false` when the plugin refused or produced bad sound.
    pub fn render_add(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        pending: &mut PendingChanges,
        notes: &mut NoteQueue,
        clock: &mut PluginClock,
        block: &mut [f32],
        channels: usize,
    ) -> bool {
        if channels == 0 {
            return true;
        }
        let mut all_ok = true;
        for chunk in block.chunks_mut(channels * MAX_FRAMES) {
            all_ok &= self.render_chunk(processor, pending, notes, clock, chunk, channels);
        }
        all_ok
    }

    fn render_chunk(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        pending: &mut PendingChanges,
        notes: &mut NoteQueue,
        clock: &mut PluginClock,
        chunk: &mut [f32],
        channels: usize,
    ) -> bool {
        let frames = chunk.len() / channels;
        if frames == 0 {
            return true;
        }
        self.out_left[..frames].fill(0.0);
        self.out_right[..frames].fill(0.0);
        if let Some(events) = &self.events {
            events.load(notes.pending());
        }
        notes.clear_events();
        let ok = self.call_process(processor, pending, clock, frames);
        if let Some(events) = &self.events {
            events.clear();
        }
        let (left, right) = (&self.out_left[..frames], &self.out_right[..frames]);
        if !ok || !left.iter().chain(right).all(|v| v.is_finite()) {
            return false;
        }
        add_planar(left, right, chunk, channels);
        true
    }

    /// One `process` call for `frames` frames: accumulated parameter changes go to the plugin and are forgotten,
    /// transport time comes from `clock`. `false` when the plugin refused.
    fn call_process(
        &mut self,
        processor: &ComPtr<IAudioProcessor>,
        pending: &mut PendingChanges,
        clock: &mut PluginClock,
        frames: usize,
    ) -> bool {
        self.data.numSamples = frames as i32;
        clock.fill(&mut self.context, frames);
        self.changes_in.load(pending);
        pending.clear();
        // SAFETY: `data` points to buffers, context, parameter and event lists owned by `self`; they are alive, do not
        // move and are sized for `MAX_FRAMES` frames (`frames` is not larger). The call comes from one thread at a time.
        let result = unsafe { processor.process(&raw mut self.data) };
        self.changes_in.clear();
        result == kResultOk
    }
}
