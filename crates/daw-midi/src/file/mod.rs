//! Project MIDI files: export of the project or a clip to a standard file and import of notes and controllers into a clip.
//!
//! Parsing is in `read`, building the file's tracks in `write`, tempo map ↔ meta events in `tempo`;
//! this module only works with the project through the contracts.

mod read;
mod tempo;
mod write;

#[cfg(test)]
mod fakes;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

use daw_contracts::codes::{clips, midi, notes as note_codes};
use daw_contracts::{
    ClipContent, ClipControl, ClipId, ContractError, ControllerKind, ControllerPoint, HistoryControl, MAX_CLIP_LOOP_TICKS, MidiFileIo,
    NewNote, NoteContent, NoteControl, PreferencesStore, TempoControl, TempoMap, TrackControl, tr,
};
use midly::Smf;

use read::{extract_controllers, extract_notes, imported_controllers, imported_notes, ticks_per_beat};
use tempo::read_tempo_map;
use write::{FileTrack, raw_content, unrolled, unrolled_controllers, write_file};

pub struct MidiFiles {
    clips: Arc<dyn ClipControl>,
    tracks: Arc<dyn TrackControl>,
    notes: Arc<dyn NoteControl>,
    /// Import into a clip: notes and controller lanes as one undo step.
    history: Arc<dyn HistoryControl>,
    /// Tempo map: written to the file and replaced by the file's map (`import_tempo`).
    tempo: Arc<dyn TempoControl>,
    /// Where the file's author comes from (user preferences).
    preferences: Arc<dyn PreferencesStore>,
}

impl MidiFiles {
    pub fn new(
        clips: Arc<dyn ClipControl>,
        tracks: Arc<dyn TrackControl>,
        notes: Arc<dyn NoteControl>,
        history: Arc<dyn HistoryControl>,
        tempo: Arc<dyn TempoControl>,
        preferences: Arc<dyn PreferencesStore>,
    ) -> Self {
        Self { clips, tracks, notes, history, tempo, preferences }
    }

    /// The clip's notes and its start tick.
    fn note_content(&self, id: ClipId) -> Result<(NoteContent, u32), ContractError> {
        let clip = self.clips.clips().into_iter().find(|c| c.id == id).ok_or_else(|| clips::NOT_FOUND.with("id", id))?;
        match clip.content {
            ClipContent::Notes(content) => Ok((content, clip.start_tick)),
            ClipContent::Audio { .. } => Err(note_codes::AUDIO_CLIP_HAS_NO_NOTES.with("id", id)),
        }
    }

    fn write(&self, path: &Path, map: &TempoMap, named: &[FileTrack]) -> Result<(), ContractError> {
        let author = self.preferences.get().profile.author;
        let bytes = write_file(map, &author, named)?;
        std::fs::write(path, bytes).map_err(|e| midi::FILE_NOT_WRITTEN.with("path", path.display()).with("details", e))
    }

    /// Replaces the clip's notes and all its controller lanes with the file's; lanes missing in the file are removed.
    fn replace_content(
        &self,
        clip: ClipId,
        before: &NoteContent,
        notes: Vec<NewNote>,
        lanes: Vec<(ControllerKind, Vec<ControllerPoint>)>,
    ) -> Result<usize, ContractError> {
        let count = self.notes.replace_notes(clip, notes)?;
        for old in before.controllers.iter().filter(|old| !lanes.iter().any(|(kind, _)| *kind == old.kind)) {
            self.notes.set_controller_lane(clip, old.kind, Vec::new())?;
        }
        for (kind, points) in lanes {
            self.notes.set_controller_lane(clip, kind, points)?;
        }
        Ok(count)
    }
}

impl MidiFileIo for MidiFiles {
    fn export_project(&self, path: &Path) -> Result<(), ContractError> {
        let clips = self.clips.clips();
        let mut named = Vec::new();
        for track in self.tracks.tracks() {
            let (mut notes, mut controllers) = (Vec::new(), Vec::new());
            for clip in clips.iter().filter(|c| c.track == track.id) {
                if let ClipContent::Notes(content) = &clip.content {
                    notes.extend(unrolled(clip, content));
                    controllers.extend(unrolled_controllers(clip, content));
                }
            }
            if !notes.is_empty() || !controllers.is_empty() {
                named.push(FileTrack { name: track.name, notes, controllers });
            }
        }
        self.write(path, &self.tempo.tempo_map(), &named)
    }

    /// The clip's notes and controllers as they are (content of `loop_ticks` length, no repeats): importing back gives the same.
    /// Time in the file counts from the clip start, so tempo and signature are the ones at the clip start, without changes.
    fn export_clip(&self, clip: ClipId, path: &Path) -> Result<(), ContractError> {
        let (content, start) = self.note_content(clip)?;
        let map = self.tempo.tempo_map();
        let at_start = TempoMap::constant(map.tempo_at(f64::from(start)), map.signature_at(f64::from(start)).signature);
        self.write(path, &at_start, &[raw_content("Clip", &content)])
    }

    fn import_into(&self, clip: ClipId, path: &Path) -> Result<usize, ContractError> {
        let (content, _) = self.note_content(clip)?;
        let bytes = read_file(path)?;
        let smf = parse(&bytes)?;
        let per_beat = ticks_per_beat(smf.header.timing)?;
        let track = smf.tracks.iter().find(|t| !extract_notes(t).is_empty()).ok_or_else(|| midi::FILE_HAS_NO_NOTES.err())?;

        let limit = if content.auto_length { MAX_CLIP_LOOP_TICKS } else { content.loop_ticks };
        let notes = imported_notes(&extract_notes(track), per_beat, limit);
        let lanes = imported_controllers(&extract_controllers(track), per_beat, limit);
        self.history.begin_group(&tr!("Import MIDI", "Импорт MIDI"));
        let result = self.replace_content(clip, &content, notes, lanes);
        if result.is_ok() {
            self.history.end_group();
        } else {
            self.history.cancel_group();
        }
        result
    }

    fn import_tempo(&self, path: &Path) -> Result<(), ContractError> {
        let bytes = read_file(path)?;
        self.tempo.set_tempo_map(read_tempo_map(&parse(&bytes)?)?)
    }
}

fn parse(bytes: &[u8]) -> Result<Smf<'_>, ContractError> {
    Smf::parse(bytes).map_err(|e| midi::BAD_FILE.with("details", e))
}

fn read_file(path: &Path) -> Result<Vec<u8>, ContractError> {
    std::fs::read(path).map_err(|e| midi::FILE_NOT_READ.with("path", path.display()).with("details", e))
}
