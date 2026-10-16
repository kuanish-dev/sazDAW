//! `ProjectBounce` (`TrackBounce`): replaces a track with its sound — "Bounce to audio", "Freeze" and "Unfreeze".
//! `TrackRender` (offline render) computes the track sound; the rest goes through the project's other entities —
//! the sample catalog, tracks, clips and the graph — as one undo group; the core writes the frozen mark on the
//! instrument.

use std::path::PathBuf;
use std::sync::Arc;

use daw_contracts::codes::{export, tracks};
use daw_contracts::{
    AssetStore, ClipControl, ContractError, GraphControl, HistoryControl, ProjectView, SampleCatalog, SampleId, SampleWriter, Track,
    TrackBounce, TrackControl, TrackId, TrackRender, TrackSound, tr,
};

use crate::{ProjectService, ops};

/// The project entities through which the audio copy takes its place.
pub struct BounceParts {
    pub tracks: Arc<dyn TrackControl>,
    pub clips: Arc<dyn ClipControl>,
    pub graph: Arc<dyn GraphControl>,
    pub samples: Arc<dyn SampleCatalog>,
}

pub struct ProjectBounce {
    project: Arc<ProjectService>,
    render: Arc<dyn TrackRender>,
    writer: Arc<dyn SampleWriter>,
    assets: Arc<dyn AssetStore>,
    parts: BounceParts,
}

/// What to do with the original track once the copy is in place.
#[derive(Clone, Copy)]
enum Mode {
    /// Mute it (the node's mute).
    Bounce,
    /// Mark the instrument frozen: the engine does not create it.
    Freeze,
}

impl ProjectBounce {
    pub fn new(
        project: Arc<ProjectService>,
        render: Arc<dyn TrackRender>,
        writer: Arc<dyn SampleWriter>,
        assets: Arc<dyn AssetStore>,
        parts: BounceParts,
    ) -> Self {
        Self { project, render, writer, assets, parts }
    }

    /// Track sound → WAV → a copy below the track. The long render runs BEFORE the undo group: the project is not
    /// locked while it computes, and only the placement goes into the undo step.
    fn make_copy(&self, track: TrackId, tail_seconds: f32, mode: Mode) -> Result<TrackId, ContractError> {
        let snapshot = self.project.snapshot();
        let original = ops::track(&snapshot, track)?.clone();
        if original.instrument.as_ref().is_some_and(|spec| spec.frozen.is_some()) {
            return Err(tracks::FROZEN.with("id", track));
        }
        if matches!(mode, Mode::Freeze) && original.is_audio() {
            return Err(export::FREEZE_NEEDS_INSTRUMENT.with("id", track));
        }
        let project_file = self.project.path().ok_or_else(|| export::BOUNCE_NEEDS_SAVED_PROJECT.err())?;
        let sound = self.render.render_track(&snapshot, track, tail_seconds)?;
        let suffix = match mode {
            Mode::Bounce => tr!("audio", "аудио"),
            Mode::Freeze => tr!("frozen", "заморозка"),
        };
        let name = format!("{} ({suffix})", original.name);
        let path = self.assets.new_asset_path(&project_file, &name);
        self.writer.write(&path, &sound.data)?;

        let label = match mode {
            Mode::Bounce => tr!("Bounce to audio «{}»", "Свести в аудио «{}»", original.name),
            Mode::Freeze => tr!("Freeze «{}»", "Заморозить «{}»", original.name),
        };
        self.project.begin_group(&label);
        match self.place(&original, &name, path, &sound, mode) {
            Ok(copy) => {
                self.project.end_group();
                Ok(copy)
            }
            Err(error) => {
                self.project.cancel_group();
                Err(error)
            }
        }
    }

    /// A sample, an audio track right below the original, a clip at the first clip's place, and the same output as the
    /// original track's chain.
    fn place(&self, original: &Track, name: &str, path: PathBuf, sound: &TrackSound, mode: Mode) -> Result<TrackId, ContractError> {
        let parts = &self.parts;
        let sample = parts.samples.import(&path)?;
        let copy = parts.tracks.add_track(name, None, original.default_pitch)?;
        let below = parts.tracks.tracks().iter().position(|t| t.id == original.id).map_or(usize::MAX, |index| index + 1);
        parts.tracks.move_track(copy, below)?;
        parts.clips.add_audio_clip(copy, sound.start_tick, sample.id)?;

        let graph = parts.graph.graph();
        let source = graph.source_of(original.id).ok_or_else(|| tracks::NO_SOURCE_NODE.with("id", original.id))?.id;
        let copy_node = graph.source_of(copy).ok_or_else(|| tracks::NO_SOURCE_NODE.with("id", copy))?.id;
        // The track's volume and pan are already in the sound (the source fader comes before the inserts), and so is its
        // place on the stage with its share of the room: the copy plays at unity, off the stage. So are its polarity,
        // width and a later time alignment; an earlier one is not (a render cannot play ahead of time), so the copy
        // keeps that lead.
        parts.graph.set_volume(copy_node, 1.0)?;
        let lead = graph.node(source).and_then(|node| node.kind.strip()).map_or(0.0, |strip| strip.delay_ms.min(0.0));
        if lead < 0.0 {
            parts.graph.set_delay(copy_node, lead)?;
        }
        let path = graph.insert_path(source);
        if let Some(output) = path.output {
            let last = path.effects.last().copied().unwrap_or(source);
            let level = graph.outgoing(last).find(|e| e.to == output && !e.sidechain && !e.pre_fader).map_or(1.0, |e| e.level);
            parts.graph.set_output(copy_node, output)?;
            parts.graph.connect(copy_node, output, level, false)?;
        }
        match mode {
            Mode::Bounce => parts.graph.set_mute(source, true)?,
            Mode::Freeze => self.set_frozen(original.id, Some(copy))?,
        }
        Ok(copy)
    }

    fn set_frozen(&self, track: TrackId, copy: Option<TrackId>) -> Result<(), ContractError> {
        self.project.edit(&tr!("Freeze", "Заморозка"), |project| {
            let spec = ops::track_mut(project, track)?.instrument.as_mut().ok_or_else(|| tracks::NO_INSTRUMENT.with("id", track))?;
            spec.frozen = copy;
            Ok(())
        })
    }

    /// The copy and its samples that play nowhere else (they came with the copy).
    fn remove_copy(&self, copy: TrackId) -> Result<(), ContractError> {
        let samples: Vec<SampleId> =
            self.project.read(|p| p.clips.iter().filter(|c| c.track == copy).filter_map(|c| c.content.sample()).collect());
        self.parts.tracks.remove_track(copy)?;
        for sample in samples {
            let used = self.project.read(|p| p.clips.iter().any(|c| c.content.sample() == Some(sample)));
            let known = self.project.read(|p| p.samples.iter().any(|s| s.id == sample));
            if !used && known {
                self.parts.samples.remove(sample)?;
            }
        }
        Ok(())
    }
}

impl TrackBounce for ProjectBounce {
    fn bounce(&self, track: TrackId, tail_seconds: f32) -> Result<TrackId, ContractError> {
        self.make_copy(track, tail_seconds, Mode::Bounce)
    }

    fn freeze(&self, track: TrackId, tail_seconds: f32) -> Result<TrackId, ContractError> {
        self.make_copy(track, tail_seconds, Mode::Freeze)
    }

    fn unfreeze(&self, track: TrackId) -> Result<(), ContractError> {
        let (name, frozen, copy_exists) = self.project.read(|p| {
            let original = ops::track(p, track)?;
            let frozen = original.instrument.as_ref().and_then(|spec| spec.frozen);
            let copy_exists = frozen.is_some_and(|copy| p.track(copy).is_some_and(Track::is_audio));
            Ok::<_, ContractError>((original.name.clone(), frozen, copy_exists))
        })?;
        let Some(copy) = frozen else {
            return Err(export::NOT_FROZEN.with("id", track));
        };
        self.project.begin_group(&tr!("Unfreeze «{name}»", "Разморозить «{name}»"));
        let result = if copy_exists { self.remove_copy(copy) } else { Ok(()) }.and_then(|()| self.set_frozen(track, None));
        match result {
            Ok(()) => {
                self.project.end_group();
                Ok(())
            }
            Err(error) => {
                self.project.cancel_group();
                Err(error)
            }
        }
    }
}
