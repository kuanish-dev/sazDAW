//! Test doubles for file I/O: an in-memory project and tempo map.

use std::sync::Mutex;

use daw_contracts::codes::{clips, general};
use daw_contracts::{
    BAR_TICKS, Bpm, Clip, ClipContent, ClipControl, ClipEdit, ClipId, ContractError, ControllerKind, ControllerLane, ControllerPoint,
    HistoryControl, HistoryMark, MAX_CLIP_LOOP_TICKS, NewNote, NewNoteClip, Note, NoteContent, NoteControl, NoteId, Preferences,
    PreferencesStore, SampleId, SignatureChange, TempoChange, TempoControl, TempoMap, TimeSignature, Track, TrackControl, TrackId,
};

/// An in-memory project, enough for file I/O: tracks, clips and their notes.
#[derive(Default)]
pub(super) struct Fake {
    pub tracks: Mutex<Vec<Track>>,
    pub clips: Mutex<Vec<Clip>>,
    /// Edit groups: (opened, closed, cancelled).
    pub groups: Mutex<(u32, u32, u32)>,
}

fn unsupported<T>() -> Result<T, ContractError> {
    Err(general::INTERNAL.with("details", "not used by the tests"))
}

impl Fake {
    pub fn track(&self, id: TrackId, name: &str, instrument: bool) {
        let instrument = instrument.then(|| daw_contracts::InstrumentSpec::new("synth", Default::default()));
        self.tracks.lock().unwrap().push(Track::new(id, name, instrument, 60));
    }

    pub fn note_clip(&self, id: ClipId, track: TrackId, start: u32, length: u32, offset: u32, loop_ticks: u32) {
        let mut content = NoteContent::empty();
        content.loop_ticks = loop_ticks;
        self.clips.lock().unwrap().push(Clip {
            id,
            track,
            start_tick: start,
            length_ticks: length,
            content: ClipContent::Notes(content),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: offset,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        });
    }

    pub fn set_notes(&self, id: ClipId, notes: &[(u8, u32, u32)]) {
        self.replace_notes(
            id,
            notes.iter().map(|&(pitch, start_tick, length_ticks)| NewNote { pitch, start_tick, length_ticks, velocity: 100 }).collect(),
        )
        .unwrap();
    }

    pub fn notes_of(&self, id: ClipId) -> Vec<(u8, u32, u32)> {
        let mut notes: Vec<_> = self.notes(id).unwrap().iter().map(|n| (n.pitch, n.start_tick, n.length_ticks)).collect();
        notes.sort_unstable();
        notes
    }
}

impl ClipControl for Fake {
    fn clips(&self) -> Vec<Clip> {
        self.clips.lock().unwrap().clone()
    }
    fn loop_region(&self) -> Option<(u32, u32)> {
        None
    }
    fn add_note_clip(&self, _: NewNoteClip) -> Result<Clip, ContractError> {
        unsupported()
    }
    fn add_note_clips(&self, _: Vec<NewNoteClip>) -> Result<Vec<Clip>, ContractError> {
        unsupported()
    }
    fn add_audio_clip(&self, _: TrackId, _: u32, _: SampleId) -> Result<Clip, ContractError> {
        unsupported()
    }
    fn edit_clip(&self, _: ClipId, _: ClipEdit) -> Result<Clip, ContractError> {
        unsupported()
    }
    fn split_clip(&self, _: ClipId, _: u32) -> Result<Clip, ContractError> {
        unsupported()
    }
    fn trim_start(&self, _: ClipId, _: u32) -> Result<Clip, ContractError> {
        unsupported()
    }
    fn duplicate_clip(&self, _: ClipId) -> Result<Clip, ContractError> {
        unsupported()
    }
    fn remove_clip(&self, _: ClipId) -> Result<(), ContractError> {
        unsupported()
    }
    fn edit_clips(&self, _: Vec<(ClipId, ClipEdit)>) -> Result<Vec<Clip>, ContractError> {
        unsupported()
    }
    fn remove_clips(&self, _: Vec<ClipId>) -> Result<(), ContractError> {
        unsupported()
    }
    fn copy_clips(&self, _: Vec<ClipId>, _: u32) -> Result<Vec<Clip>, ContractError> {
        unsupported()
    }
    fn set_loop_region(&self, _: Option<(u32, u32)>) -> Result<(), ContractError> {
        unsupported()
    }
}

impl TrackControl for Fake {
    fn tracks(&self) -> Vec<Track> {
        self.tracks.lock().unwrap().clone()
    }
    fn add_tracks(&self, _: Vec<daw_contracts::NewTrack>) -> Result<Vec<TrackId>, ContractError> {
        unsupported()
    }
    fn add_track(&self, _: &str, _: Option<&str>, _: u8) -> Result<TrackId, ContractError> {
        unsupported()
    }
    fn remove_track(&self, _: TrackId) -> Result<(), ContractError> {
        unsupported()
    }
    fn rename_track(&self, _: TrackId, _: &str) -> Result<(), ContractError> {
        unsupported()
    }
    fn set_instrument_param(&self, _: TrackId, _: &str, _: f32) -> Result<(), ContractError> {
        unsupported()
    }
    fn set_instrument(&self, _: TrackId, _: &str, _: daw_contracts::Params, _: Option<&str>) -> Result<(), ContractError> {
        unsupported()
    }
    fn move_track(&self, _: TrackId, _: usize) -> Result<(), ContractError> {
        unsupported()
    }
    fn set_monitor(&self, _: TrackId, _: daw_contracts::MonitorMode) -> Result<(), ContractError> {
        unsupported()
    }
    fn set_color(&self, _: TrackId, _: Option<&str>) -> Result<(), ContractError> {
        unsupported()
    }
    fn set_height(&self, _: TrackId, _: Option<u16>) -> Result<(), ContractError> {
        unsupported()
    }
}

impl NoteControl for Fake {
    fn notes(&self, clip: ClipId) -> Result<Vec<Note>, ContractError> {
        match self.clips.lock().unwrap().iter().find(|c| c.id == clip).map(|c| &c.content) {
            Some(ClipContent::Notes(content)) => Ok(content.notes.clone()),
            _ => Err(clips::NOT_FOUND.with("id", clip)),
        }
    }
    fn add_note(&self, _: ClipId, _: NewNote) -> Result<Note, ContractError> {
        unsupported()
    }
    fn update_note(&self, _: ClipId, _: NoteId, _: NewNote) -> Result<Note, ContractError> {
        unsupported()
    }
    fn remove_note(&self, _: ClipId, _: NoteId) -> Result<(), ContractError> {
        unsupported()
    }
    fn clear_notes(&self, _: ClipId) -> Result<(), ContractError> {
        unsupported()
    }
    fn add_notes(&self, _: ClipId, _: Vec<NewNote>) -> Result<Vec<Note>, ContractError> {
        unsupported()
    }
    fn update_notes(&self, _: ClipId, _: Vec<(NoteId, NewNote)>) -> Result<Vec<Note>, ContractError> {
        unsupported()
    }
    fn remove_notes(&self, _: ClipId, _: Vec<NoteId>) -> Result<(), ContractError> {
        unsupported()
    }
    /// Like the real implementation: checks bounds and fits the auto length to bars.
    fn replace_notes(&self, clip: ClipId, notes: Vec<NewNote>) -> Result<usize, ContractError> {
        let mut clips = self.clips.lock().unwrap();
        let Some(ClipContent::Notes(content)) = clips.iter_mut().find(|c| c.id == clip).map(|c| &mut c.content) else {
            return Err(clips::NOT_FOUND.with("id", clip));
        };
        let limit = if content.auto_length { MAX_CLIP_LOOP_TICKS } else { content.loop_ticks };
        for note in &notes {
            note.validate(limit)?;
        }
        content.notes = notes.iter().enumerate().map(|(i, n)| Note::from_new(i as NoteId + 1, *n)).collect();
        if content.auto_length {
            let end = notes.iter().map(|n| n.start_tick + n.length_ticks).max().unwrap_or(0);
            content.loop_ticks = end.div_ceil(BAR_TICKS).max(1) * BAR_TICKS;
        }
        Ok(notes.len())
    }
    fn set_loop_ticks(&self, _: ClipId, _: u32) -> Result<(), ContractError> {
        unsupported()
    }
    fn transform_notes(&self, _: ClipId, _: Option<Vec<NoteId>>, _: daw_contracts::NoteTransform) -> Result<Vec<Note>, ContractError> {
        unsupported()
    }
    fn vary_to_new_clip(
        &self,
        _: ClipId,
        _: Option<Vec<NoteId>>,
        _: daw_contracts::Variation,
    ) -> Result<daw_contracts::Clip, ContractError> {
        unsupported()
    }
    fn set_controller_lane(&self, clip: ClipId, kind: ControllerKind, points: Vec<ControllerPoint>) -> Result<(), ContractError> {
        let mut clips = self.clips.lock().unwrap();
        let Some(ClipContent::Notes(content)) = clips.iter_mut().find(|c| c.id == clip).map(|c| &mut c.content) else {
            return Err(clips::NOT_FOUND.with("id", clip));
        };
        content.controllers.retain(|lane| lane.kind != kind);
        if !points.is_empty() {
            content.controllers.push(ControllerLane { kind, points });
        }
        Ok(())
    }
}

impl HistoryControl for Fake {
    fn undo(&self) -> bool {
        false
    }
    fn redo(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo_label(&self) -> Option<String> {
        None
    }
    fn begin_group(&self, _: &str) {
        self.groups.lock().unwrap().0 += 1;
    }
    fn end_group(&self) -> bool {
        self.groups.lock().unwrap().1 += 1;
        true
    }
    fn cancel_group(&self) {
        self.groups.lock().unwrap().2 += 1;
    }
    fn mark_baseline(&self) {}
    fn checkpoint(&self) -> HistoryMark {
        0
    }
    fn squash(&self, _: HistoryMark, _: &str) -> Result<bool, ContractError> {
        Ok(false)
    }
}

/// An in-memory tempo map; 120 BPM, 4/4 by default.
pub(super) struct FakeTempo(pub Mutex<TempoMap>);

impl Default for FakeTempo {
    fn default() -> Self {
        Self(Mutex::new(TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR)))
    }
}

impl TempoControl for FakeTempo {
    fn tempo_map(&self) -> TempoMap {
        self.0.lock().unwrap().clone()
    }
    fn set_tempo_changes(&self, changes: Vec<TempoChange>) -> Result<(), ContractError> {
        let map = self.tempo_map().with_tempos(changes)?;
        self.set_tempo_map(map)
    }
    fn set_signature_changes(&self, changes: Vec<SignatureChange>) -> Result<(), ContractError> {
        let map = self.tempo_map().with_signatures(changes)?;
        self.set_tempo_map(map)
    }
    fn set_tempo_map(&self, map: TempoMap) -> Result<(), ContractError> {
        *self.0.lock().unwrap() = map;
        Ok(())
    }
    fn set_tempo(&self, _: Bpm) -> Result<(), ContractError> {
        unsupported()
    }
    fn set_time_signature(&self, _: TimeSignature) -> Result<(), ContractError> {
        unsupported()
    }
}

/// Preferences with the given author.
#[derive(Default)]
pub(super) struct FakePreferences(pub Mutex<Preferences>);

impl FakePreferences {
    pub fn with_author(author: &str) -> Self {
        let mut preferences = Preferences::default();
        preferences.profile.author = author.into();
        Self(Mutex::new(preferences))
    }
}

impl PreferencesStore for FakePreferences {
    fn get(&self) -> Preferences {
        self.0.lock().unwrap().clone()
    }
    fn set(&self, preferences: Preferences) -> Result<Preferences, ContractError> {
        *self.0.lock().unwrap() = preferences.clone();
        Ok(preferences)
    }
}
