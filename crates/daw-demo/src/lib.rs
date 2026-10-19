//! `DemoBeat` (`DemoProject`): "build a starter beat": drums through a compressor, a synth bass, reverb on a parallel
//! path. It is built through the same track, clip (with notes right away), graph and automation interfaces the user
//! has, all in one edit group that the core then treats as the project's initial state.

use std::sync::Arc;

use daw_contracts::codes::tracks;
use daw_contracts::{
    AutoPoint, AutomationControl, BAR_TICKS, Bpm, ClipControl, ContractError, DEFAULT_VELOCITY, DemoProject, GraphControl, HistoryControl,
    L10n, MASTER, NewNote, NewNoteClip, NodeId, ParamTarget, Params, STEP_TICKS, SYNTH_KIND, TempoControl, TrackControl, TrackId, tr,
};

/// Length of the demo clips: one bar of notes repeats four times. The beat is written in 4/4 (16 steps per bar), so its
/// bar is the constant `BAR_TICKS`, not the tempo map's bar: in another signature the pattern just plays as it is.
const CLIP_BARS: u32 = 4;
/// The bass volume fades in over the first two bars.
const BASS_FADE_IN_TICKS: u32 = 2 * BAR_TICKS;

/// Everything the beat is built with; the bass plays on the built-in synth (`SYNTH_KIND`).
pub struct DemoBeat {
    pub tempo: Arc<dyn TempoControl>,
    pub tracks: Arc<dyn TrackControl>,
    pub clips: Arc<dyn ClipControl>,
    pub graph: Arc<dyn GraphControl>,
    pub automation: Arc<dyn AutomationControl>,
    pub history: Arc<dyn HistoryControl>,
}

impl DemoProject for DemoBeat {
    fn fill_demo(&self) -> Result<(), ContractError> {
        self.history.begin_group(&tr!("Starter beat", "Стартовый бит"));
        if let Err(error) = self.fill() {
            self.history.cancel_group();
            return Err(error);
        }
        self.history.end_group();
        self.history.mark_baseline();
        Ok(())
    }
}

/// Drum tracks of the demo: (name, kind, pitch, note steps in a bar).
const DRUMS: [(L10n, &str, u8, &[u32]); 4] = [
    (L10n { en: "Kick", ru: "Кик" }, "kick", 36, &[0, 4, 8, 12]),
    (L10n { en: "Snare", ru: "Снейр" }, "snare", 38, &[4, 12]),
    (L10n { en: "Hi-hat", ru: "Хэт" }, "hat", 42, &[2, 6, 10, 14]),
    (L10n { en: "Clap", ru: "Клэп" }, "clap", 39, &[]),
];

/// The bass line: (pitch, step, length in steps).
const BASS: [(u8, u32, u32); 5] = [(36, 0, 2), (36, 3, 2), (39, 6, 2), (36, 10, 2), (34, 12, 3)];

/// A note on the step grid, slightly shorter than its steps so adjacent notes do not merge.
fn step_note(pitch: u8, step: u32, length_steps: u32) -> NewNote {
    NewNote { pitch, start_tick: step * STEP_TICKS, length_ticks: length_steps * STEP_TICKS - STEP_TICKS / 6, velocity: DEFAULT_VELOCITY }
}

/// The demo graph nodes.
struct Mix {
    compressor: NodeId,
    bass_bus: NodeId,
}

impl DemoBeat {
    fn fill(&self) -> Result<(), ContractError> {
        self.tempo.set_tempo(Bpm::new(128.0)?)?;
        // Tracks come first, so they lead the list and the graph, as for a user who started with them.
        let drums = DRUMS
            .iter()
            .map(|(name, kind, pitch, _)| self.tracks.add_track(name.get(), Some(kind), *pitch))
            .collect::<Result<Vec<_>, _>>()?;
        let synth = self.tracks.add_track(&tr!("Synth", "Синт"), Some(SYNTH_KIND), 36)?;
        let mix = self.build_mix()?;
        for (track, (_, _, pitch, steps)) in drums.into_iter().zip(DRUMS) {
            self.route(track, mix.compressor)?;
            if !steps.is_empty() {
                self.clip(track, steps.iter().map(|&step| step_note(pitch, step, 1)).collect())?;
            }
        }
        self.route(synth, mix.bass_bus)?;
        self.clip(synth, BASS.iter().map(|&(pitch, step, length)| step_note(pitch, step, length)).collect())?;
        let lane = self.automation.add_lane(ParamTarget::NodeVolume { node: mix.bass_bus })?;
        let clip = self.automation.add_clip(lane, 0, BASS_FADE_IN_TICKS)?;
        let fade = vec![AutoPoint::new(0, 0.2), AutoPoint::new(BASS_FADE_IN_TICKS, 0.9)];
        self.automation.set_points(lane, clip, fade)
    }

    /// Drums → compressor → "Drums" bus → master; synth → "Bass" bus → master; after the compressor the sound also goes
    /// to the reverb → "Reverb" bus → master (a parallel path: it cannot return into the drum bus).
    fn build_mix(&self) -> Result<Mix, ContractError> {
        let g = &self.graph;
        let drums = g.add_bus(&tr!("Drums", "Ударные"), 420.0, 60.0)?;
        let bass_bus = g.add_bus(&tr!("Bass", "Бас"), 420.0, 260.0)?;
        let reverb_bus = g.add_bus(&tr!("Reverb", "Реверб"), 640.0, 360.0)?;
        let compressor = g.add_effect("compressor", &Params::new(), 220.0, 60.0)?;
        let reverb = g.add_effect("reverb", &Params::new(), 420.0, 360.0)?;
        g.set_effect_param(reverb, "mix", 1.0)?;
        g.set_effect_param(reverb, "room", 0.7)?;
        g.set_volume(drums, 0.9)?;
        g.set_volume(bass_bus, 0.9)?;
        g.set_volume(reverb_bus, 0.7)?;
        g.connect(compressor, drums, 1.0, false)?;
        g.connect(compressor, reverb, 0.18, false)?;
        g.connect(reverb, reverb_bus, 1.0, false)?;
        Ok(Mix { compressor, bass_bus })
    }

    /// Routes the track output to node `to` instead of the master.
    fn route(&self, track: TrackId, to: NodeId) -> Result<(), ContractError> {
        let graph = self.graph.graph();
        let source = graph.source_of(track).ok_or_else(|| tracks::NOT_FOUND.with("id", track))?;
        if graph.edges.iter().any(|e| e.from == source.id && e.to == MASTER) {
            self.graph.disconnect(source.id, MASTER, false)?;
        }
        self.graph.connect(source.id, to, 1.0, false)
    }

    /// A note clip of `CLIP_BARS` bars with one bar of notes.
    fn clip(&self, track: TrackId, notes: Vec<NewNote>) -> Result<(), ContractError> {
        self.clips.add_note_clip(NewNoteClip {
            track,
            start_tick: 0,
            length_ticks: Some(CLIP_BARS * BAR_TICKS),
            notes,
            loop_ticks: None,
        })?;
        Ok(())
    }
}
