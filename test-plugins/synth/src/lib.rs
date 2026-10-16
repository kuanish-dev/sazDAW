//! Synth — a polyphonic subtractive synth (VST3 instrument) with a visual window.
//! Glue with the host: parameters (`params`), sound (`engine`), display (`visuals` + `editor`).

mod curves;
mod editor;
mod engine;
mod params;
mod visuals;
mod widgets;

use nih_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;

use engine::Engine;
use params::SynthParams;
use visuals::{Visuals, SCOPE_DECIMATION};

/// CC number of the modulation wheel.
const MODULATION_CC: u8 = 1;

/// A block quieter than this counts as silence (for "the instance is sounding").
const SOUND_THRESHOLD: f32 = 1e-4;

struct Synth {
    params: Arc<SynthParams>,
    engine: Engine,
    visuals: Arc<Visuals>,
}

impl Default for Synth {
    fn default() -> Self {
        Self { params: Arc::new(SynthParams::default()), engine: Engine::default(), visuals: Arc::new(Visuals::default()) }
    }
}

impl Plugin for Synth {
    const NAME: &'static str = "Synth";
    const VENDOR: &'static str = "DAW";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] =
        &[AudioIOLayout { main_input_channels: None, main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() }];

    // Not only notes: the host sends pitch bend and CC (mod wheel) — in VST3 these are parameters from `IMidiMapping`.
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.visuals.clone())
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        self.engine.prepare(config.sample_rate);
        // A window opened on another plugin instance (as the DAW host does) finds this one by its sound.
        visuals::register(&self.visuals);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.visuals.set_notes(0);
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let mut next_event = ctx.next_event();
        let mut since_scope_sample = 0;
        let mut block_peak = 0.0_f32;
        for (frame, mut channels) in buffer.iter_samples().enumerate() {
            let settings = self.params.next_settings();
            while let Some(event) = next_event {
                if event.timing() > frame as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => self.engine.note_on(note, velocity),
                    NoteEvent::NoteOff { note, .. } => self.engine.note_off(note, settings.release),
                    NoteEvent::Choke { .. } => self.engine.all_notes_off(settings.release),
                    // nih-plug gives bend as 0..1 with the middle at 0.5.
                    NoteEvent::MidiPitchBend { value, .. } => self.engine.set_pitch_bend(value * 2.0 - 1.0),
                    NoteEvent::MidiCC { cc: MODULATION_CC, value, .. } => self.engine.set_modulation(value),
                    _ => {}
                }
                next_event = ctx.next_event();
            }
            let sample = self.engine.next_sample(&settings);
            for out in channels.iter_mut() {
                *out = sample;
            }
            block_peak = block_peak.max(sample.abs());
            since_scope_sample += 1;
            if since_scope_sample == SCOPE_DECIMATION {
                since_scope_sample = 0;
                self.visuals.push_sample(sample);
            }
        }
        if block_peak > SOUND_THRESHOLD {
            self.visuals.mark_sound();
        }
        self.visuals.set_notes(self.engine.active_notes());
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for Synth {
    const VST3_CLASS_ID: [u8; 16] = *b"DawSynthPlugin01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

// The warning comes from the foreign macro `vst3_com::vtable` inside `nih_export_vst3!` (an extra `;`
// in an expression); it cannot be fixed in our code. The compiler ignores an attribute on the macro call itself,
// so the call is wrapped in a module and the warning is silenced only there.
#[allow(semicolon_in_expressions_from_non_local_macros, reason = "an extra `;` in the foreign vst3_com macro, not in our code")]
mod vst3_export {
    use super::Synth;
    use nih_plug::nih_export_vst3;

    nih_export_vst3!(Synth);
}
