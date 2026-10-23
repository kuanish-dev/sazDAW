//! Bus layout of an instance: an effect (stereo in and out) or an instrument (no input, stereo out), 32-bit samples.
//!
//! The layout is remembered when the effect or instrument is created and applied again on every start (`start`):
//! the plugin may forget it after `setActive(false)`.

use vst3::Steinberg::Vst::{BusDirections_, IAudioProcessorTrait, IComponentTrait, MediaTypes_, SpeakerArr, SpeakerArrangement};

use super::{PluginInstance, SAMPLE_SIZE, check};

/// How the plugin is connected to the host: this decides the bus layout on every start.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Effect,
    Instrument,
}

impl PluginInstance {
    /// Effect: checks that the plugin takes stereo and 32-bit samples, and turns on the main buses.
    pub fn configure_effect(&mut self) -> Result<(), String> {
        self.layout = Layout::Effect;
        self.apply_layout()
    }

    /// Instrument: no input, stereo out. Only the first output bus is active; the others (extra
    /// stereo pairs of multi-output synths) and all inputs are off.
    pub fn configure_instrument(&mut self) -> Result<(), String> {
        self.layout = Layout::Instrument;
        self.apply_layout()
    }

    pub(super) fn apply_layout(&self) -> Result<(), String> {
        match self.layout {
            Layout::Effect => self.apply_effect_layout(),
            Layout::Instrument => self.apply_instrument_layout(),
        }
    }

    fn apply_effect_layout(&self) -> Result<(), String> {
        if self.in_buses == 0 || self.out_buses == 0 {
            return Err("the plugin has no audio input or output (it is not an effect)".into());
        }
        self.require_f32()?;
        let mut inputs = [SpeakerArr::kStereo];
        let mut outputs = [SpeakerArr::kStereo];
        // SAFETY: the arrays are local; the element counts are passed correctly (one main bus each).
        check(unsafe { self.processor.setBusArrangements(inputs.as_mut_ptr(), 1, outputs.as_mut_ptr(), 1) }, "stereo is not supported")?;
        self.activate_main_buses(true);
        Ok(())
    }

    fn apply_instrument_layout(&self) -> Result<(), String> {
        if self.out_buses == 0 {
            return Err("the plugin has no audio output (it is not an instrument)".into());
        }
        self.require_f32()?;
        let mut inputs = vec![SpeakerArr::kStereo; self.in_buses];
        let mut outputs = vec![SpeakerArr::kStereo; self.out_buses];
        // SAFETY: the vectors are local; the element counts are passed correctly.
        unsafe {
            self.processor.setBusArrangements(inputs.as_mut_ptr(), self.in_buses as i32, outputs.as_mut_ptr(), self.out_buses as i32)
        };
        let mut arrangement: SpeakerArrangement = 0;
        // SAFETY: `arrangement` is a local variable for the plugin's answer.
        check(unsafe { self.processor.getBusArrangement(BusDirections_::kOutput as i32, 0, &raw mut arrangement) }, "getBusArrangement")?;
        if arrangement != SpeakerArr::kStereo {
            return Err("the plugin's main output is not stereo".into());
        }
        self.activate_main_buses(false);
        Ok(())
    }

    fn require_f32(&self) -> Result<(), String> {
        // SAFETY: a plain call without pointers.
        check(unsafe { self.processor.canProcessSampleSize(SAMPLE_SIZE) }, "32-bit samples are not supported")
    }

    /// Turns on the main output bus (and the main input if `main_input`); the others (sidechain, extra
    /// outputs) are turned off.
    fn activate_main_buses(&self, main_input: bool) {
        for (direction, count) in [(BusDirections_::kInput, self.in_buses), (BusDirections_::kOutput, self.out_buses)] {
            for index in 0..count {
                let on = index == 0 && (direction == BusDirections_::kOutput || main_input);
                // SAFETY: a plain call without pointers.
                unsafe { self.component.activateBus(MediaTypes_::kAudio as i32, direction as i32, index as i32, u8::from(on)) };
            }
        }
    }
}
