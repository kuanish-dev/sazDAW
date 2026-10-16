//! Session wiring by area: catalogs (`catalogs.rs`), project (`project.rs`), sound (`audio.rs`), drum and chord parts (`parts.rs`).
//! Here is the overall build order and the resulting `Controls`.

mod arrangement;
mod audio;
mod catalogs;
mod parts;
mod project;

use std::sync::Arc;

use daw_contracts::{
    AppLog, AssetStore, Controls, MixAnalysis, PreferencesStore, ProcessedAudio, ProjectStorage, SampleDecoder, SampleStore,
};
use daw_demo::DemoBeat;
use daw_engine::create_engine;
use daw_metronome::create_metronome;
use daw_midi::MidiFiles;
use daw_mixcheck::MixChecker;
use daw_persistence::{FileLog, FsAssets, JsonStorage, MetronomeSettingsFile, PreferencesFile, ProjectAutosave, RecentStore, SourceStore};
use daw_render::{LevelMeter, LiveMeter, OfflineExporter};
use daw_samples::{FileDecoder, FolderSounds, SampleBank};
use daw_stretch::StretchCache;
use daw_summary::Summarizer;
use daw_transport::create_transport;
use daw_vst3::{Vst3CrashGuard, Vst3Editors};

use crate::plugins::{self, PluginDeps};
use crate::{Session, SessionConfig, engines};

/// Which sound devices the session sees.
pub(crate) enum Devices {
    /// The real system devices.
    System,
    /// No sound card (tests, scripts): the lists are empty, nothing can be opened.
    Headless,
}

pub(crate) fn build(config: &SessionConfig, devices: &Devices) -> Session {
    // All app failures (backend, interface, panics) go to one file next to the app state.
    let log: Arc<dyn AppLog> = Arc::new(FileLog::open(&config.state_dir.join("logs")));
    let (transport, clock) = create_transport();
    let metronome_settings = Arc::new(MetronomeSettingsFile::new(&config.state_dir, log.clone()));
    let (metronome, metronome_processor) = create_metronome(metronome_settings, log.clone());
    let engine = create_engine(clock, vec![metronome_processor]);
    let link = engine.link;
    // Opening the preferences makes the saved language current, so it goes before anything that builds labels.
    let preferences: Arc<dyn PreferencesStore> = Arc::new(PreferencesFile::open(&config.state_dir, log.clone()));
    // Freeing what the engine let go and logging its losses happen in their own thread while the engine lives.
    crate::engine_watch::spawn(Arc::downgrade(&link) as _, Arc::downgrade(&link) as _, log.clone());

    let bank: Arc<dyn SampleStore> = Arc::new(SampleBank::new());
    let decoder: Arc<dyn SampleDecoder> = Arc::new(FileDecoder::new(log.clone()));
    let assets: Arc<dyn AssetStore> = Arc::new(FsAssets);
    let storage: Arc<dyn ProjectStorage> = Arc::new(JsonStorage::new(log.clone()));
    let recent = Arc::new(RecentStore::open(&config.state_dir, log.clone()));
    // The plugin crash marker is read before anything that can create a plugin: the report on the previous session and the guard of this one.
    let crash_guard = Arc::new(Vst3CrashGuard::open(&config.state_dir.join(plugins::GUARD_FILE), &*recent, &*storage, log.clone()));
    let catalogs =
        catalogs::build(config, devices, bank.clone(), PluginDeps { preferences: &preferences, marker: crash_guard.marker(), log: &log });
    let processed: Arc<dyn ProcessedAudio> = Arc::new(StretchCache::new(bank.clone(), log.clone()));

    let project = project::build(
        project::Engine { snapshots: link.clone(), dsp: link.clone(), transport: transport.clone(), hold: link.clone() },
        project::Storage {
            storage: storage.clone(),
            recent: recent.clone(),
            assets: assets.clone(),
            decoder: decoder.clone(),
            samples: bank.clone(),
            processed: processed.clone(),
        },
        catalogs.instruments.clone(),
        catalogs.effects.clone(),
        catalogs.note_effects.clone(),
        log.clone(),
    );
    let host = audio::host(config, devices, &log);
    let audio = audio::build(
        host,
        audio::EngineLink { dsp: link.clone(), stats: link.clone(), live: link.clone(), record: link.clone(), monitor: link.clone() },
        &project,
        transport.clone(),
        bank.clone(),
        decoder,
        assets.clone(),
        log.clone(),
    );
    let parts = parts::build(&project, catalogs.instruments.clone(), link.clone(), &*log);
    // Plugin windows write edits to the project through the same contracts as the interface; computer keys in an
    // instrument window play live notes on its track.
    let plugin_editors = Arc::new(Vst3Editors::new(
        catalogs.directory,
        project.graph.clone(),
        project.tracks.clone(),
        project.service.clone(),
        project.plugin_states.clone(),
        log.clone(),
        link.clone(),
    ));
    let sound_sources = Arc::new(SourceStore::open(&config.state_dir, log.clone()));
    let exporter = Arc::new(OfflineExporter::new(
        project.service.clone(),
        catalogs.instruments.clone(),
        catalogs.effects.clone(),
        catalogs.note_effects.clone(),
        bank,
        processed.clone(),
        Arc::new(engines::OfflineEngines),
    ));
    // Checking the work by ear (for the AI): levels come from the same offline engine as the export.
    let analysis: Arc<dyn MixAnalysis> = Arc::new(LevelMeter::new(exporter.clone()));
    // The live loudness meter measures the playing master with the export's own BS.1770 meters.
    let master_meter = Arc::new(LiveMeter::new(link.clone(), log.clone()));
    let mix_check = Arc::new(MixChecker::new(
        analysis.clone(),
        catalogs.instruments.clone(),
        catalogs.effects.clone(),
        catalogs.plugins.clone(),
        log.clone(),
    ));
    let arrangement =
        arrangement::build(&config.state_dir, &project, exporter.clone(), storage.clone(), assets.clone(), catalogs.instruments.clone());
    // Background jobs change the sound of tempo-fitted clips without a project edit: the watch refreshes the snapshot.
    crate::clip_audio_watch::spawn(Arc::downgrade(&project.service), processed, &*log);
    // The starter beat is built with the same interfaces the user has.
    let demo = Arc::new(DemoBeat {
        tempo: project.tempo.clone(),
        tracks: project.tracks.clone(),
        clips: project.clips.clone(),
        graph: project.graph.clone(),
        automation: project.automation.clone(),
        history: project.service.clone(),
    });

    let controls = Controls {
        midi_files: Arc::new(MidiFiles::new(
            project.clips.clone(),
            project.tracks.clone(),
            project.notes.clone(),
            project.service.clone(),
            project.tempo.clone(),
            preferences.clone(),
        )),
        autosave: Arc::new(ProjectAutosave::new(project.files.clone(), log.clone())),
        sounds: Arc::new(FolderSounds::new(project.files.clone(), sound_sources.clone())),
        transport,
        tempo: project.tempo,
        metronome,
        tracks: project.tracks,
        clips: project.clips,
        takes: project.takes,
        notes: project.notes,
        note_effects: project.note_effects,
        markers: project.markers,
        folders: project.folders,
        graph: project.graph,
        stage: project.stage,
        automation: project.automation,
        automation_writer: project.automation_writer,
        history: project.service.clone(),
        project: project.files,
        view: project.service,
        demo,
        samples: project.samples,
        sample_editor: project.sample_editor,
        clip_processing: project.clip_processing,
        preview: audio.preview,
        exporter,
        midi_input: audio.midi_input,
        midi_mappings: project.midi_mappings,
        live_notes: link.clone(),
        record: audio.record,
        node_scope: link.clone(),
        master_meter,
        mono: link.clone(),
        stats: link,
        instruments: catalogs.instruments,
        effects: catalogs.effects,
        note_effect_kinds: catalogs.note_effects,
        plugins: catalogs.plugins,
        plugin_editors,
        plugin_crash: crash_guard,
        storage,
        recent,
        sound_sources,
        log,
        audio: audio.host.clone(),
        audio_record: audio.audio_record,
        patterns: parts.patterns,
        drum_part: parts.drum_part,
        drum_preview: parts.drum_preview,
        chord_part: parts.chord_part,
        chord_preview: parts.chord_preview,
        preferences,
        bounce: arrangement.bounce,
        templates: arrangement.templates,
        presets: arrangement.presets,
        analysis,
        mix_check,
        clear: project.clear,
        summary: Arc::new(Summarizer),
    };
    Session { controls, renderer: engine.renderer, audio: audio.host }
}
