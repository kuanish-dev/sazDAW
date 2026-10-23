//! Offline rendering of the project faster than real time: a separate engine instance with its own instruments and
//! effects (`OfflineExporter::load_engine`), so live playback is not interrupted.
//!
//! Built on it: export to files (`Exporter`, `export/`: range, loudness; stems in `stems.rs`), writing WAV/FLAC/MP3
//! files (`encode/`), one track's sound for bouncing to audio (`TrackRender`, `track.rs`), project sound levels for
//! checking the mix (`LevelMeter`, `MixAnalysis`, `analysis/`).
//!
//! The crate also owns the app's BS.1770 / EBU R128 meters (`loudness.rs`), so everything that measures loudness uses
//! one implementation: the export, the level analysis, and the live master meter (`LiveMeter`, `MasterMeter`,
//! `live/`), which reads the playing master from the engine's `MasterTap` rather than an offline render.

mod analysis;
mod encode;
mod export;
mod live;
mod loudness;
mod stems;
mod track;

use std::collections::HashMap;
use std::sync::Arc;

use daw_contracts::{
    AudioRenderer, ClipAudio, ClipId, ContractError, DspCommand, DspSink, EffectCatalog, EngineFactory, EngineSnapshot, InstrumentCatalog,
    NodeKind, NoteEffectCatalog, ProcessJob, ProcessedAudio, Project, ProjectView, SampleId, SampleStore, StreamConfig, TransportControl,
};

pub use analysis::LevelMeter;
pub use live::LiveMeter;

const CHUNK_FRAMES: usize = 4096;

pub struct OfflineExporter {
    project: Arc<dyn ProjectView>,
    instruments: Arc<dyn InstrumentCatalog>,
    effects: Arc<dyn EffectCatalog>,
    note_effects: Arc<dyn NoteEffectCatalog>,
    samples: Arc<dyn SampleStore>,
    processed: Arc<dyn ProcessedAudio>,
    engines: Arc<dyn EngineFactory>,
}

/// An engine loaded with a project and ready to play from the start.
struct LoadedEngine {
    renderer: Box<dyn AudioRenderer>,
    dsp: Arc<dyn DspSink>,
    transport: Arc<dyn TransportControl>,
}

impl OfflineExporter {
    pub fn new(
        project: Arc<dyn ProjectView>,
        instruments: Arc<dyn InstrumentCatalog>,
        effects: Arc<dyn EffectCatalog>,
        note_effects: Arc<dyn NoteEffectCatalog>,
        samples: Arc<dyn SampleStore>,
        processed: Arc<dyn ProcessedAudio>,
        engines: Arc<dyn EngineFactory>,
    ) -> Self {
        Self { project, instruments, effects, note_effects, samples, processed, engines }
    }

    /// Audio of clip takes with tempo fitting and pitch (for a comp, every sounding take) by the export tempo map:
    /// computed right away (the export waits); a failure is an export error, not a silent clip.
    fn clip_audio(&self, project: &Project) -> Result<HashMap<(ClipId, SampleId), ClipAudio>, ContractError> {
        let mut result = HashMap::new();
        for clip in project.clips.iter().filter(|clip| clip.has_sound_processing()) {
            for take in clip.sounding_takes() {
                let Some(source) = self.samples.get(take) else { continue };
                let audio = match ProcessJob::for_clip(clip, take, &project.tempo_map, source.sample_rate) {
                    Some(job) => ClipAudio { data: self.processed.compute(&job)?, start_frame: job.start_frame(clip) },
                    None => ClipAudio { data: source, start_frame: clip.offset_frames as f64 },
                };
                result.insert((clip.id, take), audio);
            }
        }
        Ok(result)
    }

    /// A new engine with instruments, effects, MIDI effects and the project snapshot (without the loop: an export plays
    /// from start to end).
    fn load_engine(&self, project: &Project, sample_rate: u32) -> Result<LoadedEngine, ContractError> {
        let engine = self.engines.create();
        let mut renderer = engine.renderer;
        renderer.prepare(StreamConfig { sample_rate, ..StreamConfig::DEFAULT });

        for track in &project.tracks {
            let Some(spec) = track.sounding_instrument() else {
                continue;
            };
            let instrument = self.instruments.create(&spec.kind, &spec.params, spec.state.as_deref())?;
            engine.dsp.send(DspCommand::AddInstrument { track: track.id, instrument })?;
        }
        for track in project.tracks.iter().filter(|t| !t.note_effects.is_empty()) {
            let chain = track
                .note_effects
                .iter()
                .map(|spec| Ok((spec.id, self.note_effects.create(&spec.kind, &spec.params)?)))
                .collect::<Result<Vec<_>, ContractError>>()?;
            engine.dsp.send(DspCommand::SetNoteEffects { track: track.id, chain })?;
        }
        for node in &project.graph.nodes {
            if let NodeKind::Effect { effect, params, state, .. } = &node.kind {
                let effect = self.effects.create(effect, params, state.as_deref())?;
                engine.dsp.send(DspCommand::AddEffect { node: node.id, effect })?;
            }
        }
        let samples = project.samples.iter().filter_map(|s| self.samples.get(s.id).map(|data| (s.id, data))).collect();
        let clip_audio = self.clip_audio(project)?;
        let mut export_project = project.clone();
        export_project.loop_region = None;
        let order = export_project.graph.order();
        engine.snapshots.publish(Arc::new(EngineSnapshot { version: 1, project: export_project, order, samples, clip_audio }));
        Ok(LoadedEngine { renderer, dsp: engine.dsp, transport: engine.transport })
    }
}
