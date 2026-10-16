//! `DrumPartPreviewer`: "play a part without touching the project": the same patterns and drum sounds a build would give,
//! rendered and looped by `render.rs` (cymbal tails folded onto the loop start).

use std::sync::Arc;

use daw_contracts::codes::patterns;
use daw_contracts::{
    BAR_TICKS, ContractError, DrumPartPreview, DrumPartRequest, DspSink, InstrumentCatalog, PatternLibrary, STEP_TICKS, SampleCatalog,
    TempoControl, TrackControl, TrackId, velocity_level,
};

use crate::kit::{LaneKit, LaneTrack};
use crate::plan::{Placement, plan};
use crate::render::{self, Event, MAX_PREVIEW_BARS, Voice, frame_at};

/// Who plays: a shared track (several drums on it are one instrument) or a drum to be created.
#[derive(PartialEq)]
enum Player {
    Track(TrackId),
    Lane(String),
}

/// Who plays and what: drums sharing a track are one instrument.
struct Scheduled {
    player: Player,
    voice: Voice,
}

/// The hit schedule of all the part's drums, one per instrument, and the loop length in ticks.
fn schedule(placements: &[Placement], lane_tracks: &[LaneTrack], frames_per_tick: f64) -> (Vec<Scheduled>, u32) {
    let base = placements.first().map_or(0, |p| p.start_tick);
    let mut voices: Vec<Scheduled> = Vec::new();
    let mut loop_ticks = 0;
    for placement in placements {
        let pattern_ticks = placement.pattern.bars * BAR_TICKS;
        loop_ticks = loop_ticks.max(placement.start_tick - base + placement.length_ticks());
        for hits in &placement.pattern.lanes {
            let Some(target) = lane_tracks.iter().find(|t| t.lane == hits.lane) else { continue };
            let Some(sound) = target.spec() else { continue };
            let player = target.id().map_or_else(|| Player::Lane(target.lane.clone()), Player::Track);
            let pitch = target.pitch();
            let index = voices.iter().position(|v| v.player == player).unwrap_or_else(|| {
                voices.push(Scheduled { player, voice: Voice { sound, events: Vec::new() } });
                voices.len() - 1
            });
            for repeat in 0..placement.repeat {
                for hit in &hits.hits {
                    let start = placement.start_tick - base + repeat * pattern_ticks + hit.step * STEP_TICKS;
                    let velocity = velocity_level(hit.velocity);
                    voices[index].voice.events.push(Event { frame: frame_at(start, frames_per_tick), on: true, pitch, velocity });
                    voices[index].voice.events.push(Event {
                        frame: frame_at(start + STEP_TICKS, frames_per_tick),
                        on: false,
                        pitch,
                        velocity: 0.0,
                    });
                }
            }
        }
    }
    (voices, loop_ticks)
}

pub struct DrumPartPreviewer {
    library: Arc<dyn PatternLibrary>,
    kit: LaneKit,
    instruments: Arc<dyn InstrumentCatalog>,
    /// The preview tempo is the project tempo at the bar where the part starts (no changes inside the part).
    tempo: Arc<dyn TempoControl>,
    dsp: Arc<dyn DspSink>,
}

impl DrumPartPreviewer {
    pub fn new(
        library: Arc<dyn PatternLibrary>,
        tracks: Arc<dyn TrackControl>,
        instruments: Arc<dyn InstrumentCatalog>,
        samples: Arc<dyn SampleCatalog>,
        tempo: Arc<dyn TempoControl>,
        dsp: Arc<dyn DspSink>,
    ) -> Self {
        let kit = LaneKit::new(tracks, instruments.clone(), samples);
        Self { library, kit, instruments, tempo, dsp }
    }
}

impl DrumPartPreview for DrumPartPreviewer {
    fn play(&self, request: DrumPartRequest) -> Result<(), ContractError> {
        let map = self.tempo.tempo_map();
        let placements = plan(&*self.library, &request, &map)?;
        let bars: u32 = placements.iter().map(|p| p.pattern.bars * p.repeat).sum();
        if bars > MAX_PREVIEW_BARS {
            return Err(patterns::PREVIEW_TOO_LONG.with("max", MAX_PREVIEW_BARS).with("bars", bars));
        }
        let lane_tracks = self.kit.resolve(&request, &placements)?;

        let tempo = map.tempo_at(placements.first().map_or(0.0, |p| f64::from(p.start_tick)));
        let frames_per_tick = render::frames_per_tick(tempo);
        let (scheduled, loop_ticks) = schedule(&placements, &lane_tracks, frames_per_tick);
        let mut voices: Vec<Voice> = scheduled.into_iter().map(|s| s.voice).collect();
        render::play_loop(&*self.instruments, &*self.dsp, &mut voices, frame_at(loop_ticks, frames_per_tick))
    }

    fn stop(&self) {
        render::stop(&*self.dsp);
    }
}
