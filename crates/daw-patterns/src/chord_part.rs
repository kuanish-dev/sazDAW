//! `ChordPartBuilder`: "turn a chord progression and a strum into a note clip". `plan` checks the whole request and
//! works out the notes (`strum.rs`, `voicing.rs`) without touching the project; the preview plays the same plan, so what
//! you hear is what gets built. The build is one undo group: a new track if asked, the replaced clips removed, one clip
//! whose content is one pass of the progression, looping for the repeats; a failure halfway rolls the group back.

use std::sync::Arc;

use daw_contracts::codes::{general, patterns, tracks};
use daw_contracts::{
    BAR_TICKS, Chord, ChordPartControl, ChordPartReport, ChordPartRequest, ChordTarget, ClipControl, ClipId, ContractError, DEFAULT_PITCH,
    HistoryControl, InstrumentCatalog, InstrumentSpec, L10n, MAX_CHORD_BARS, NewNote, NewNoteClip, NewTrack, Params, TempoControl,
    TempoMap, TrackControl, TrackId, strum,
};

use crate::plan::{MAX_REPEAT, bar_start_tick, overlapping};
use crate::strum::{progression_notes, spread_ticks, strokes};
use crate::voicing::style;

const LABEL: L10n = L10n { en: "Chord part", ru: "Партия аккордов" };

/// Everything a chord part needs, checked and worked out.
pub(crate) struct ChordPlan {
    /// The target track; `None` means a new one, `new_track`.
    pub(crate) track: Option<TrackId>,
    pub(crate) new_track: Option<NewTrack>,
    /// The track's name, for refusals and the report.
    pub(crate) name: String,
    /// What plays the part (the preview creates it).
    pub(crate) sound: InstrumentSpec,
    pub(crate) start_tick: u32,
    /// One pass of the progression: its length and notes (from the pass start).
    pub(crate) pass_ticks: u32,
    pub(crate) notes: Vec<NewNote>,
    pub(crate) repeat: u32,
}

impl ChordPlan {
    pub(crate) fn length_ticks(&self) -> u32 {
        self.pass_ticks * self.repeat
    }

    pub(crate) fn bars(&self) -> u32 {
        self.length_ticks() / BAR_TICKS
    }
}

/// The chords of every bar; a bar needs one or two.
fn chords(bars: &[Vec<String>]) -> Result<Vec<Vec<Chord>>, ContractError> {
    if bars.is_empty() {
        return Err(patterns::NO_CHORDS.err());
    }
    if bars.len() > MAX_CHORD_BARS as usize {
        return Err(patterns::TOO_MANY_CHORD_BARS.with("max", MAX_CHORD_BARS).with("count", bars.len()));
    }
    bars.iter()
        .enumerate()
        .map(|(index, symbols)| {
            if !(1..=2).contains(&symbols.len()) {
                return Err(patterns::BAR_CHORD_COUNT.with("bar", index + 1).with("count", symbols.len()));
            }
            symbols.iter().map(|symbol| Chord::parse(symbol)).collect()
        })
        .collect()
}

/// Checks the request and works out the part; changes nothing.
pub(crate) fn plan(
    request: &ChordPartRequest,
    tracks: &dyn TrackControl,
    instruments: &dyn InstrumentCatalog,
    map: &TempoMap,
) -> Result<ChordPlan, ContractError> {
    let strum = strum(&request.strum)?;
    let bars = chords(&request.bars)?;
    if !(1..=MAX_REPEAT).contains(&request.repeat) {
        return Err(patterns::REPEAT_OUT_OF_RANGE.with("pattern", strum.id).with("max", MAX_REPEAT).with("repeat", request.repeat));
    }
    let (track, new_track, name, sound) = match &request.target {
        ChordTarget::Track { id } => {
            let track = tracks.tracks().into_iter().find(|track| track.id == *id).ok_or_else(|| tracks::NOT_FOUND.with("id", id))?;
            let sound = track.instrument.ok_or_else(|| patterns::CHORDS_ON_AUDIO_TRACK.with("name", &track.name))?;
            (Some(track.id), None, track.name, sound)
        }
        ChordTarget::New { kind } => {
            let info = instruments
                .kinds()
                .into_iter()
                .find(|info| info.kind == kind)
                .ok_or_else(|| tracks::UNKNOWN_INSTRUMENT.with("kind", kind))?;
            let name = info.label.get().to_string();
            let new_track = NewTrack { name: name.clone(), kind: Some(kind.clone()), default_pitch: DEFAULT_PITCH, params: Params::new() };
            (None, Some(new_track), name, InstrumentSpec::new(kind.clone(), instruments.defaults(kind)?))
        }
    };
    let start_tick = bar_start_tick(map, request.start_bar)?;
    let spread = spread_ticks(map.tempo_at(f64::from(start_tick)));
    let notes = progression_notes(&bars, &strokes(strum.grid), style(&sound.kind), spread);
    let pass_ticks = u32::try_from(bars.len()).unwrap_or(MAX_CHORD_BARS) * BAR_TICKS;
    Ok(ChordPlan { track, new_track, name, sound, start_tick, pass_ticks, notes, repeat: request.repeat })
}

pub struct ChordPartBuilder {
    tracks: Arc<dyn TrackControl>,
    clips: Arc<dyn ClipControl>,
    history: Arc<dyn HistoryControl>,
    instruments: Arc<dyn InstrumentCatalog>,
    /// Tempo map: bar `start_bar` and the strum's speed.
    tempo: Arc<dyn TempoControl>,
}

impl ChordPartBuilder {
    pub fn new(
        tracks: Arc<dyn TrackControl>,
        clips: Arc<dyn ClipControl>,
        history: Arc<dyn HistoryControl>,
        instruments: Arc<dyn InstrumentCatalog>,
        tempo: Arc<dyn TempoControl>,
    ) -> Self {
        Self { tracks, clips, history, instruments, tempo }
    }

    fn apply(&self, plan: ChordPlan, replaced: Vec<ClipId>) -> Result<ChordPartReport, ContractError> {
        let bars = plan.bars();
        let length_ticks = plan.length_ticks();
        let (track, created_track) = match (plan.track, plan.new_track) {
            (Some(track), _) => (track, None),
            (None, new_track) => {
                let ids = self.tracks.add_tracks(new_track.into_iter().collect())?;
                let id = ids.first().copied().ok_or_else(|| general::INTERNAL.with("details", "add_tracks returned no id"))?;
                (id, Some(plan.name))
            }
        };
        if !replaced.is_empty() {
            self.clips.remove_clips(replaced)?;
        }
        let clip = self.clips.add_note_clip(NewNoteClip {
            track,
            start_tick: plan.start_tick,
            length_ticks: Some(length_ticks),
            notes: plan.notes,
            loop_ticks: Some(plan.pass_ticks),
        })?;
        Ok(ChordPartReport { track, created_track, clip: clip.id, bars })
    }
}

impl ChordPartControl for ChordPartBuilder {
    fn build(&self, request: ChordPartRequest) -> Result<ChordPartReport, ContractError> {
        let map = self.tempo.tempo_map();
        let plan = plan(&request, &*self.tracks, &*self.instruments, &map)?;

        // Occupied space is checked before any edit: either a clear refusal or the clips get replaced.
        let end = plan.start_tick.saturating_add(plan.length_ticks());
        let replaced = plan.track.map(|track| overlapping(&self.clips.clips(), track, plan.start_tick, end)).unwrap_or_default();
        if !replaced.is_empty() && !request.replace {
            return Err(patterns::PLACE_TAKEN
                .with("name", &plan.name)
                .with("from", request.start_bar)
                .with("to", map.bar_index(f64::from(end.saturating_sub(1)))));
        }

        self.history.begin_group(LABEL.get());
        let result = self.apply(plan, replaced);
        if result.is_err() {
            // A failure halfway: bring the project back as it was so half a part is not left behind.
            self.history.cancel_group();
        } else {
            self.history.end_group();
        }
        result
    }
}
