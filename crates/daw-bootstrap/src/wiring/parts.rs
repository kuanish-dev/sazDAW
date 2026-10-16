//! Parts from patterns: drum parts (ready beats) and chord parts (strums), each built into the project and previewed
//! through the same contracts as the user interface.

use std::sync::Arc;

use daw_contracts::{
    AppLog, ChordPartControl, ChordPartPreview, DrumPartControl, DrumPartPreview, DspSink, InstrumentCatalog, PatternLibrary,
};
use daw_patterns::{ChordPartBuilder, ChordPartPreviewer, DrumPartBuilder, DrumPartPreviewer, PatternBank};

use super::project::ProjectParts;

pub(super) struct Parts {
    pub patterns: Arc<dyn PatternLibrary>,
    pub drum_part: Arc<dyn DrumPartControl>,
    pub drum_preview: Arc<dyn DrumPartPreview>,
    pub chord_part: Arc<dyn ChordPartControl>,
    pub chord_preview: Arc<dyn ChordPartPreview>,
}

pub(super) fn build(project: &ProjectParts, instruments: Arc<dyn InstrumentCatalog>, dsp: Arc<dyn DspSink>, log: &dyn AppLog) -> Parts {
    let patterns: Arc<dyn PatternLibrary> = Arc::new(PatternBank::builtin(log));
    let drum_part = Arc::new(DrumPartBuilder::new(
        patterns.clone(),
        project.tracks.clone(),
        project.clips.clone(),
        project.service.clone(),
        instruments.clone(),
        project.samples.clone(),
        project.tempo.clone(),
    ));
    let drum_preview = Arc::new(DrumPartPreviewer::new(
        patterns.clone(),
        project.tracks.clone(),
        instruments.clone(),
        project.samples.clone(),
        project.tempo.clone(),
        dsp.clone(),
    ));
    let chord_part = Arc::new(ChordPartBuilder::new(
        project.tracks.clone(),
        project.clips.clone(),
        project.service.clone(),
        instruments.clone(),
        project.tempo.clone(),
    ));
    let chord_preview = Arc::new(ChordPartPreviewer::new(project.tracks.clone(), instruments, project.tempo.clone(), dsp));
    Parts { patterns, drum_part, drum_preview, chord_part, chord_preview }
}
