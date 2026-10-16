//! `Previewer`: "listen to a sample or a file" through a separate engine voice, without touching the project.

use std::path::Path;
use std::sync::Arc;

use daw_contracts::codes::samples;
use daw_contracts::{ContractError, DspCommand, DspSink, EngineStats, SampleDecoder, SampleId, SamplePreview, SampleStore};

pub struct Previewer {
    store: Arc<dyn SampleStore>,
    decoder: Arc<dyn SampleDecoder>,
    dsp: Arc<dyn DspSink>,
    stats: Arc<dyn EngineStats>,
}

impl Previewer {
    pub fn new(store: Arc<dyn SampleStore>, decoder: Arc<dyn SampleDecoder>, dsp: Arc<dyn DspSink>, stats: Arc<dyn EngineStats>) -> Self {
        Self { store, decoder, dsp, stats }
    }
}

impl SamplePreview for Previewer {
    fn play(&self, id: SampleId, from: usize, to: usize, looping: bool) -> Result<(), ContractError> {
        let data = self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        let frames = data.frame_count();
        if frames < 2 || from >= frames || to <= from {
            return Err(samples::REGION_EMPTY.with("from", from).with("to", to).with("frames", frames));
        }
        self.dsp.send(DspCommand::PreviewPlay { data, from, to: to.min(frames), looping })
    }

    fn play_file(&self, path: &Path) -> Result<(), ContractError> {
        let data = Arc::new(self.decoder.decode(path)?);
        let frames = data.frame_count();
        if frames < 2 {
            return Err(samples::PREVIEW_FILE_TOO_SHORT.with("path", path.display()));
        }
        self.dsp.send(DspCommand::PreviewPlay { data, from: 0, to: frames, looping: false })
    }

    fn stop(&self) {
        // The engine may have stopped together with the app: then there is nothing to stop, and that is not a failure.
        let _ = self.dsp.send(DspCommand::PreviewStop);
    }

    fn position(&self) -> Option<usize> {
        self.stats.preview_position()
    }
}
