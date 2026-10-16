//! Syncing the audio thread's DSP objects with the model, and publishing snapshots.
//!
//! The order matters: missing objects are created first, then the snapshot is published, and only then are stale ones
//! removed — so the engine never sees a node without its object in a snapshot. A failure to create or pass an object
//! does not break the edit: it goes to the log (`source: "dsp"`) and to the project warnings, and is retried only after
//! the kind or the parameters change.

use std::collections::HashMap;
use std::sync::Arc;

use daw_contracts::codes::{graph, notes, tracks};
use daw_contracts::{
    Clip, ClipAudio, ClipId, ContractError, DspCommand, EngineSnapshot, InstrumentSpec, LogEntry, NodeId, NodeKind, NoteEffectId,
    PluginState, ProcessJob, Project, SampleData, SampleId, TrackId,
};

use super::{Inner, ProjectService};

/// What was created in the audio thread for a track's instrument; when the key changes the instrument is recreated.
/// The plugin state is part of the key: it cannot be passed to a live instance in the audio thread, only to a new one.
#[derive(Clone, PartialEq, Default)]
struct InstrumentKey {
    kind: String,
    rebuild: Vec<(String, u32)>,
    state: Option<Arc<PluginState>>,
}

/// What was created in the audio thread for a node's effect: the kind and the plugin state (params go via `set_param`).
#[derive(Clone, PartialEq)]
struct EffectKey {
    kind: String,
    state: Option<Arc<PluginState>>,
}

/// Objects already in the audio thread, and attempts that failed (so they are not repeated on every edit).
#[derive(Default)]
pub(super) struct DspState {
    instruments: HashMap<TrackId, InstrumentKey>,
    effects: HashMap<NodeId, EffectKey>,
    failed_instruments: HashMap<TrackId, InstrumentKey>,
    failed_effects: HashMap<NodeId, EffectKey>,
    /// MIDI effect chains in the audio thread: effect ids and kinds in order. Another set or order recreates the whole
    /// chain (params and bypass travel with the snapshot). A failed attempt is remembered too.
    note_effects: HashMap<TrackId, Vec<(NoteEffectId, String)>>,
    /// Processed sound of clip takes in the last snapshot: while new sound is computed (the tempo changed), the clip
    /// plays the old one instead of going silent.
    clip_audio: HashMap<(ClipId, SampleId), ClipAudio>,
}

impl DspState {
    /// Forgets failed attempts: after a project opens (say, plugins have been found since) they are tried again.
    pub(super) fn forget_failures(&mut self) {
        self.failed_instruments.clear();
        self.failed_effects.clear();
    }
}

impl ProjectService {
    /// Creates missing DSP objects, publishes the snapshot, then removes stale ones.
    pub(super) fn sync(&self, inner: &mut Inner) {
        let region = inner.project.loop_region.map(|(start, end)| (u64::from(start), u64::from(end)));
        self.deps.transport.set_loop(region);
        self.deps.transport.set_song_end(inner.project.song_loop_end());
        self.sync_instruments(inner);
        self.sync_effects(inner);
        self.sync_note_effects(inner);
        self.publish_snapshot(inner);
        self.retire_gone(inner);
    }

    fn instrument_key(&self, spec: &InstrumentSpec) -> InstrumentKey {
        let rebuild = self
            .deps
            .instruments
            .rebuild_on(&spec.kind)
            .into_iter()
            .map(|name| (name.to_string(), spec.params.get(name).copied().unwrap_or(0.0).to_bits()))
            .collect();
        InstrumentKey { kind: spec.kind.clone(), rebuild, state: spec.state.clone() }
    }

    fn sync_instruments(&self, inner: &mut Inner) {
        for track in &inner.project.tracks {
            // A frozen instrument is not created: the track's audio copy plays instead.
            let Some(spec) = track.sounding_instrument() else { continue };
            let key = self.instrument_key(spec);
            let dsp = &mut inner.dsp;
            if dsp.instruments.get(&track.id) == Some(&key) || dsp.failed_instruments.get(&track.id) == Some(&key) {
                continue;
            }
            let sent = self
                .deps
                .instruments
                .create(&spec.kind, &spec.params, spec.state.as_deref())
                .and_then(|instrument| self.deps.dsp.send(DspCommand::AddInstrument { track: track.id, instrument }));
            match sent {
                Ok(()) => {
                    dsp.failed_instruments.remove(&track.id);
                    dsp.instruments.insert(track.id, key);
                }
                Err(error) => {
                    dsp.failed_instruments.insert(track.id, key);
                    let warning = tracks::INSTRUMENT_NOT_CREATED.with("track", &track.name).with("kind", &spec.kind).with("details", error);
                    self.report(&mut inner.warnings, warning);
                }
            }
        }
    }

    fn sync_effects(&self, inner: &mut Inner) {
        for node in &inner.project.graph.nodes {
            let NodeKind::Effect { effect: kind, params, state, .. } = &node.kind else { continue };
            let key = EffectKey { kind: kind.clone(), state: state.clone() };
            let dsp = &mut inner.dsp;
            if dsp.effects.get(&node.id) == Some(&key) || dsp.failed_effects.get(&node.id) == Some(&key) {
                continue;
            }
            let sent = self
                .deps
                .effects
                .create(kind, params, state.as_deref())
                .and_then(|effect| self.deps.dsp.send(DspCommand::AddEffect { node: node.id, effect }));
            match sent {
                Ok(()) => {
                    dsp.failed_effects.remove(&node.id);
                    dsp.effects.insert(node.id, key);
                }
                Err(error) => {
                    dsp.failed_effects.insert(node.id, key);
                    let warning = graph::EFFECT_NOT_CREATED.with("name", &node.name).with("kind", kind).with("details", error);
                    self.report(&mut inner.warnings, warning);
                }
            }
        }
    }

    /// MIDI effect chains: new ones and those whose set changed are rebuilt, vanished ones become an empty chain.
    /// An effect that fails to be created stays out of the chain (the failure goes to the log); the others work.
    fn sync_note_effects(&self, inner: &mut Inner) {
        let mut wanted: HashMap<TrackId, Vec<(NoteEffectId, String)>> = HashMap::new();
        for track in inner.project.tracks.iter().filter(|t| !t.note_effects.is_empty()) {
            wanted.insert(track.id, track.note_effects.iter().map(|e| (e.id, e.kind.clone())).collect());
        }
        let gone: Vec<TrackId> = inner.dsp.note_effects.keys().filter(|id| !wanted.contains_key(id)).copied().collect();
        for track in gone {
            match self.deps.dsp.send(DspCommand::SetNoteEffects { track, chain: Vec::new() }) {
                Ok(()) => drop(inner.dsp.note_effects.remove(&track)),
                Err(error) => {
                    self.report(&mut inner.warnings, notes::NOTE_EFFECTS_NOT_CLEARED.with("id", track).with("details", error));
                }
            }
        }
        for track in &inner.project.tracks {
            let Some(key) = wanted.remove(&track.id) else { continue };
            if inner.dsp.note_effects.get(&track.id) == Some(&key) {
                continue;
            }
            let mut chain = Vec::new();
            for spec in &track.note_effects {
                match self.deps.note_effects.create(&spec.kind, &spec.params) {
                    Ok(effect) => chain.push((spec.id, effect)),
                    Err(error) => {
                        let warning =
                            notes::NOTE_EFFECT_NOT_CREATED.with("track", &track.name).with("kind", &spec.kind).with("details", error);
                        self.report(&mut inner.warnings, warning);
                    }
                }
            }
            match self.deps.dsp.send(DspCommand::SetNoteEffects { track: track.id, chain }) {
                Ok(()) => drop(inner.dsp.note_effects.insert(track.id, key)),
                Err(error) => {
                    self.report(&mut inner.warnings, notes::NOTE_EFFECTS_NOT_SENT.with("track", &track.name).with("details", error));
                }
            }
        }
    }

    pub(super) fn publish_snapshot(&self, inner: &mut Inner) {
        inner.version += 1;
        let clip_audio = self.collect_clip_audio(&inner.project, &mut inner.dsp);
        let snapshot = EngineSnapshot {
            version: inner.version,
            project: inner.project.clone(),
            order: inner.project.graph.order(),
            samples: self.collect_samples(&inner.project),
            clip_audio,
        };
        self.deps.snapshots.publish(Arc::new(snapshot));
    }

    /// Whether the project has clips with tempo fitting or pitch: their sound depends on the tempo and background work.
    pub(super) fn needs_processing(inner: &Inner) -> bool {
        !inner.dsp.clip_audio.is_empty() || inner.project.clips.iter().any(Clip::has_sound_processing)
    }

    /// Sound of fitted or pitched clip takes for the snapshot (for a comp — of each sounding take): ready from
    /// `ProcessedAudio`, otherwise the previous one of the same take of this clip (while the new one is computed in the
    /// background); without processing — the sample itself. The cache needs nothing else — `retain`.
    fn collect_clip_audio(&self, project: &Project, dsp: &mut DspState) -> HashMap<(ClipId, SampleId), ClipAudio> {
        let mut jobs = Vec::new();
        let mut published = HashMap::new();
        for clip in project.clips.iter().filter(|clip| clip.has_sound_processing()) {
            for take in clip.sounding_takes() {
                let Some(source) = self.deps.samples.get(take) else { continue };
                let fresh = match ProcessJob::for_clip(clip, take, &project.tempo_map, source.sample_rate) {
                    Some(job) => {
                        let audio = self.deps.processed.ready(&job).map(|data| ClipAudio { data, start_frame: job.start_frame(clip) });
                        jobs.push(job);
                        audio
                    }
                    None => Some(ClipAudio { data: source, start_frame: clip.offset_frames as f64 }),
                };
                let key = (clip.id, take);
                if let Some(audio) = fresh.or_else(|| dsp.clip_audio.get(&key).cloned()) {
                    published.insert(key, audio);
                }
            }
        }
        self.deps.processed.retain(&jobs);
        dsp.clip_audio.clone_from(&published);
        published
    }

    /// Removes from the audio thread the objects of tracks and nodes that are no longer in the model.
    fn retire_gone(&self, inner: &mut Inner) {
        let project = &inner.project;
        let dsp = &mut inner.dsp;
        let has_instrument = |id: &TrackId| project.track(*id).is_some_and(|t| t.sounding_instrument().is_some());
        let has_effect = |id: &NodeId| project.graph.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Effect { .. }));
        dsp.failed_instruments.retain(|id, _| has_instrument(id));
        dsp.failed_effects.retain(|id, _| has_effect(id));
        let gone_instruments: Vec<TrackId> = dsp.instruments.keys().filter(|id| !has_instrument(id)).copied().collect();
        let gone_effects: Vec<NodeId> = dsp.effects.keys().filter(|id| !has_effect(id)).copied().collect();
        for track in gone_instruments {
            match self.deps.dsp.send(DspCommand::RemoveInstrument { track }) {
                Ok(()) => drop(dsp.instruments.remove(&track)),
                Err(error) => {
                    self.report(&mut inner.warnings, tracks::INSTRUMENT_NOT_REMOVED.with("id", track).with("details", error));
                }
            }
        }
        for node in gone_effects {
            match self.deps.dsp.send(DspCommand::RemoveEffect { node }) {
                Ok(()) => drop(dsp.effects.remove(&node)),
                Err(error) => self.report(&mut inner.warnings, graph::EFFECT_NOT_REMOVED.with("id", node).with("details", error)),
            }
        }
    }

    /// A sync failure goes to the log and to the project warnings (they are shown after opening).
    fn report(&self, warnings: &mut Vec<ContractError>, warning: ContractError) {
        self.deps.log.record(LogEntry::error("dsp", warning.message()));
        warnings.push(warning);
    }

    fn collect_samples(&self, project: &Project) -> HashMap<SampleId, Arc<SampleData>> {
        project.samples.iter().filter_map(|sample| self.deps.samples.get(sample.id).map(|data| (sample.id, data))).collect()
    }
}
