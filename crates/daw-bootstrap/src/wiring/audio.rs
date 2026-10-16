//! Sound and live playing: the sound device host, audio input recording, note recording, MIDI keyboard, sample preview.

use std::sync::Arc;

use daw_audio_cpal::CpalAudio;
use daw_audio_recorder::{AudioRecorder, Deps as AudioRecorderDeps};
use daw_contracts::{
    AppLog, AssetStore, AudioCapture, DspCommand, DspSink, EngineStats, InputMonitor, LiveNoteSink, LogEntry, LogLevel, RecordSource,
    SampleDecoder, SampleStore, TransportControl,
};
use daw_midi::MidiInputService;
use daw_persistence::AudioSettingsFile;
use daw_recorder::Recorder;
use daw_samples::{Previewer, WavWriter};

use super::Devices;
use super::project::ProjectParts;
use crate::SessionConfig;

/// The engine audio thread as the rest of the session sees it.
pub(super) struct EngineLink {
    pub dsp: Arc<dyn DspSink>,
    pub stats: Arc<dyn EngineStats>,
    pub live: Arc<dyn LiveNoteSink>,
    pub record: Arc<dyn RecordSource>,
    pub monitor: Arc<dyn InputMonitor>,
}

pub(super) struct Audio {
    pub host: Arc<CpalAudio>,
    pub audio_record: Arc<AudioRecorder>,
    pub record: Arc<Recorder>,
    pub midi_input: Arc<MidiInputService>,
    pub preview: Arc<Previewer>,
}

/// The sound host; in tests (`Headless`) without devices: the lists are empty, nothing can be opened, the choice is saved.
pub(super) fn host(config: &SessionConfig, devices: &Devices, log: &Arc<dyn AppLog>) -> Arc<CpalAudio> {
    let store = Arc::new(AudioSettingsFile::new(&config.state_dir, log.clone()));
    Arc::new(match devices {
        Devices::System => CpalAudio::new(store, log.clone()),
        Devices::Headless => CpalAudio::without_devices(store, log.clone()),
    })
}

#[allow(clippy::too_many_arguments, reason = "wiring: each dependency is a separate contract, there is nothing to group them by")]
pub(super) fn build(
    host: Arc<CpalAudio>,
    link: EngineLink,
    project: &ProjectParts,
    transport: Arc<dyn TransportControl>,
    samples: Arc<dyn SampleStore>,
    decoder: Arc<dyn SampleDecoder>,
    assets: Arc<dyn AssetStore>,
    log: Arc<dyn AppLog>,
) -> Audio {
    // Input monitoring: the engine gets the reading side of the host input (once, for the whole session).
    if let Some(input) = host.take_live_input()
        && let Err(error) = link.dsp.send(DspCommand::SetLiveInput { input })
    {
        log.record(LogEntry::new(LogLevel::Error, "audio", format!("the monitoring input was not passed to the engine: {error}")));
    }
    // Audio input recording: the same sound host works as `AudioCapture`.
    let audio_record = Arc::new(AudioRecorder::new(AudioRecorderDeps {
        capture: host.clone(),
        devices: host.clone(),
        stats: link.stats.clone(),
        monitor: link.monitor,
        transport: transport.clone(),
        tempo: project.tempo.clone(),
        clips: project.clips.clone(),
        takes: project.takes.clone(),
        tracks: project.tracks.clone(),
        samples: project.samples.clone(),
        writer: Arc::new(WavWriter),
        assets,
        project: project.files.clone(),
        history: project.service.clone(),
        log: log.clone(),
        max_samples: AudioRecorderDeps::DEFAULT_MAX_SAMPLES,
    }));
    let record = Arc::new(Recorder::start(
        link.record,
        project.clips.clone(),
        project.notes.clone(),
        project.service.clone(),
        transport,
        project.tempo.clone(),
        log.clone(),
    ));
    Audio {
        host,
        audio_record,
        record,
        midi_input: Arc::new(MidiInputService::new(link.live, project.midi_mappings.clone(), log)),
        preview: Arc::new(Previewer::new(samples, decoder, link.dsp, link.stats)),
    }
}
