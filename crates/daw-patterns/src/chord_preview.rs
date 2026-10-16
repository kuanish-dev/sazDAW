//! `ChordPartPreviewer`: "play a chord part without touching the project": the same plan a build would write
//! (`chord_part::plan`), every repeat, played by the target track's instrument (or the new kind with its defaults),
//! rendered and looped by `render.rs`.

use std::sync::Arc;

use daw_contracts::codes::patterns;
use daw_contracts::{
    ChordPartPreview, ChordPartRequest, ContractError, DspSink, InstrumentCatalog, TempoControl, TrackControl, velocity_level,
};

use crate::chord_part::plan;
use crate::render::{self, Event, MAX_PREVIEW_BARS, Voice, frame_at};

pub struct ChordPartPreviewer {
    tracks: Arc<dyn TrackControl>,
    instruments: Arc<dyn InstrumentCatalog>,
    /// The preview tempo is the project tempo at the bar where the part starts (no changes inside the part).
    tempo: Arc<dyn TempoControl>,
    dsp: Arc<dyn DspSink>,
}

impl ChordPartPreviewer {
    pub fn new(
        tracks: Arc<dyn TrackControl>,
        instruments: Arc<dyn InstrumentCatalog>,
        tempo: Arc<dyn TempoControl>,
        dsp: Arc<dyn DspSink>,
    ) -> Self {
        Self { tracks, instruments, tempo, dsp }
    }
}

impl ChordPartPreview for ChordPartPreviewer {
    fn play(&self, request: ChordPartRequest) -> Result<(), ContractError> {
        let map = self.tempo.tempo_map();
        let plan = plan(&request, &*self.tracks, &*self.instruments, &map)?;
        if plan.bars() > MAX_PREVIEW_BARS {
            return Err(patterns::PREVIEW_TOO_LONG.with("max", MAX_PREVIEW_BARS).with("bars", plan.bars()));
        }
        let frames_per_tick = render::frames_per_tick(map.tempo_at(f64::from(plan.start_tick)));
        let mut events = Vec::with_capacity(plan.notes.len() * 2 * plan.repeat as usize);
        for pass in 0..plan.repeat {
            for note in &plan.notes {
                let start = pass * plan.pass_ticks + note.start_tick;
                let velocity = velocity_level(note.velocity);
                events.push(Event { frame: frame_at(start, frames_per_tick), on: true, pitch: note.pitch, velocity });
                let end = frame_at(start + note.length_ticks, frames_per_tick);
                events.push(Event { frame: end, on: false, pitch: note.pitch, velocity: 0.0 });
            }
        }
        let mut voices = [Voice { sound: plan.sound.clone(), events }];
        render::play_loop(&*self.instruments, &*self.dsp, &mut voices, frame_at(plan.length_ticks(), frames_per_tick))
    }

    fn stop(&self) {
        render::stop(&*self.dsp);
    }
}
