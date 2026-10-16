//! What plays each drum of a part (`LaneKit`): a built-in sound, your track, your sample or silence.
//! Shared by building a part (tracks and clips are created from the result) and previewing it (temporary instruments
//! are created from it).

use std::collections::HashSet;
use std::sync::Arc;

use daw_contracts::codes::{patterns, samples, tracks};
use daw_contracts::{
    ContractError, DEFAULT_PITCH, DrumPartRequest, InstrumentCatalog, InstrumentSpec, LaneTarget, NewTrack, Params, SAMPLER_KIND,
    SAMPLER_ROOT_PARAM, SAMPLER_SAMPLE_PARAM, SampleCatalog, Track, TrackControl, TrackId,
};

use crate::plan::{Placement, lanes_of};

/// Drum note numbers by the General MIDI standard: so drums look familiar in the piano roll and in MIDI export.
pub(crate) fn gm_pitch(lane: &str) -> u8 {
    match lane {
        "kick" => 36,
        "rim" => 37,
        "snare" => 38,
        "clap" => 39,
        "hat" => 42,
        "tom_lo" => 45,
        "openhat" => 46,
        "tom_mid" => 47,
        "crash" => 49,
        "tom_hi" => 50,
        "ride" => 51,
        _ => DEFAULT_PITCH,
    }
}

/// What plays a drum.
enum LaneSound {
    /// The drum is silent.
    Skip,
    /// Hits go to an existing track.
    Existing { id: TrackId, name: String, pitch: u8, spec: InstrumentSpec },
    /// The track has to be created first (its description; the hits use its `default_pitch`).
    New(NewTrack),
}

/// Where the hits of one drum of the part go.
pub(crate) struct LaneTrack {
    pub(crate) lane: String,
    sound: LaneSound,
}

impl LaneTrack {
    pub(crate) fn plays(&self) -> bool {
        !matches!(self.sound, LaneSound::Skip)
    }

    /// The track, if it already exists.
    pub(crate) fn id(&self) -> Option<TrackId> {
        match self.sound {
            LaneSound::Existing { id, .. } => Some(id),
            LaneSound::Skip | LaneSound::New(_) => None,
        }
    }

    pub(crate) fn name(&self) -> &str {
        match &self.sound {
            LaneSound::Existing { name, .. } => name,
            LaneSound::New(track) => &track.name,
            LaneSound::Skip => &self.lane,
        }
    }

    /// The pitch the hits sound at.
    pub(crate) fn pitch(&self) -> u8 {
        match &self.sound {
            LaneSound::Existing { pitch, .. } => *pitch,
            LaneSound::New(track) => track.default_pitch,
            LaneSound::Skip => 0,
        }
    }

    /// The description of the track to create.
    pub(crate) fn new_track(&self) -> Option<&NewTrack> {
        if let LaneSound::New(track) = &self.sound { Some(track) } else { None }
    }

    /// The track was created: from now on the hits go to it.
    pub(crate) fn created(&mut self, id: TrackId) {
        if let Some(spec) = self.spec() {
            self.sound = LaneSound::Existing { id, name: self.name().to_string(), pitch: self.pitch(), spec };
        }
    }

    /// The instrument the hits sound with.
    pub(crate) fn spec(&self) -> Option<InstrumentSpec> {
        match &self.sound {
            LaneSound::Existing { spec, .. } => Some(spec.clone()),
            LaneSound::New(track) => track.kind.clone().map(|kind| InstrumentSpec::new(kind, track.params.clone())),
            LaneSound::Skip => None,
        }
    }
}

/// Checks the sound assignments: every drum is given at most once and is in the part.
pub(crate) fn check_assignments(request: &DrumPartRequest, lanes: &[String]) -> Result<(), ContractError> {
    let mut seen = HashSet::new();
    for assignment in &request.lanes {
        if !lanes.contains(&assignment.lane) {
            return Err(patterns::LANE_NOT_IN_PART.with("lane", &assignment.lane).with("lanes", lanes.join(", ")));
        }
        if !seen.insert(assignment.lane.as_str()) {
            return Err(patterns::LANE_ASSIGNED_TWICE.with("lane", &assignment.lane));
        }
    }
    Ok(())
}

pub(crate) struct LaneKit {
    tracks: Arc<dyn TrackControl>,
    instruments: Arc<dyn InstrumentCatalog>,
    samples: Arc<dyn SampleCatalog>,
}

impl LaneKit {
    pub(crate) fn new(tracks: Arc<dyn TrackControl>, instruments: Arc<dyn InstrumentCatalog>, samples: Arc<dyn SampleCatalog>) -> Self {
        Self { tracks, instruments, samples }
    }

    /// The built-in drum's label for new track names ("Kick").
    fn label_of(&self, lane: &str) -> String {
        self.instruments.kinds().iter().find(|k| k.kind == lane).map_or_else(|| lane.to_string(), |k| k.label.to_string())
    }

    /// The sampler's default root note: at it the sound plays without a pitch change.
    fn sampler_root(&self) -> u8 {
        self.instruments
            .defaults(SAMPLER_KIND)
            .ok()
            .and_then(|params| params.get(SAMPLER_ROOT_PARAM).copied())
            .map_or(DEFAULT_PITCH, |root| root.round().clamp(0.0, 127.0) as u8)
    }

    /// For every drum of the part: which track it goes to (see `LaneTarget`). Changes nothing in the project.
    /// If all drums are silent it refuses: there is nothing to play.
    pub(crate) fn resolve(&self, request: &DrumPartRequest, placements: &[Placement]) -> Result<Vec<LaneTrack>, ContractError> {
        let lanes = lanes_of(placements);
        check_assignments(request, &lanes)?;
        let existing = self.tracks.tracks();
        let lane_tracks = lanes
            .into_iter()
            .map(|lane| {
                let target = request.lanes.iter().find(|a| a.lane == lane).map_or(LaneTarget::Auto, |a| a.target);
                let sound = match target {
                    LaneTarget::Skip => LaneSound::Skip,
                    LaneTarget::Auto => self.builtin(&lane, &existing)?,
                    LaneTarget::Track { id, pitch } => user_track(&lane, &existing, id, pitch)?,
                    LaneTarget::Sample { id } => self.sampler(&lane, id)?,
                };
                Ok(LaneTrack { lane, sound })
            })
            .collect::<Result<Vec<_>, ContractError>>()?;
        if !lane_tracks.iter().any(LaneTrack::plays) {
            return Err(patterns::ALL_LANES_SKIPPED.err());
        }
        Ok(lane_tracks)
    }

    /// A built-in drum: your track of this kind (the topmost), otherwise a new one.
    fn builtin(&self, lane: &str, existing: &[Track]) -> Result<LaneSound, ContractError> {
        if let Some(track) = existing.iter().find(|t| t.instrument.as_ref().is_some_and(|i| i.kind == lane))
            && let Some(spec) = track.instrument.clone()
        {
            return Ok(LaneSound::Existing { id: track.id, name: track.name.clone(), pitch: track.default_pitch, spec });
        }
        if !self.instruments.kinds().iter().any(|k| k.kind == lane) {
            return Err(patterns::LANE_INSTRUMENT_NOT_FOUND.with("lane", lane));
        }
        let name = self.label_of(lane);
        Ok(LaneSound::New(NewTrack { name, kind: Some(lane.to_string()), default_pitch: gm_pitch(lane), params: Params::new() }))
    }

    /// A new sampler track "Kick: sample name".
    fn sampler(&self, lane: &str, id: u32) -> Result<LaneSound, ContractError> {
        let sample = self.samples.samples().into_iter().find(|s| s.id == id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        Ok(LaneSound::New(NewTrack {
            name: format!("{}: {}", self.label_of(lane), sample.name),
            kind: Some(SAMPLER_KIND.to_string()),
            default_pitch: self.sampler_root(),
            params: Params::from([(SAMPLER_SAMPLE_PARAM.to_string(), id as f32)]),
        }))
    }
}

/// Your instrument track; an audio track does not play notes.
fn user_track(lane: &str, existing: &[Track], id: TrackId, pitch: Option<u8>) -> Result<LaneSound, ContractError> {
    let track = existing.iter().find(|t| t.id == id).ok_or_else(|| tracks::NOT_FOUND.with("id", id))?;
    let Some(spec) = track.instrument.clone() else {
        return Err(patterns::LANE_ON_AUDIO_TRACK.with("name", &track.name).with("lane", lane));
    };
    Ok(LaneSound::Existing { id, name: track.name.clone(), pitch: pitch.unwrap_or(track.default_pitch), spec })
}

#[cfg(test)]
mod tests {
    use daw_contracts::{LaneAssignment, PatternSlot};

    use super::*;

    #[test]
    fn assignments_for_unknown_or_repeated_drums_are_rejected() {
        let lanes = vec!["kick".to_string(), "snare".to_string()];
        let with = |names: &[&str]| DrumPartRequest {
            start_bar: 0,
            slots: vec![PatternSlot { pattern: "rock_basic".into(), repeat: 1 }],
            replace: false,
            lanes: names.iter().map(|n| LaneAssignment { lane: (*n).into(), target: LaneTarget::Skip }).collect(),
        };
        assert!(check_assignments(&with(&["kick", "snare"]), &lanes).is_ok());
        let unknown = check_assignments(&with(&["crash"]), &lanes).unwrap_err();
        assert!(unknown.is(patterns::LANE_NOT_IN_PART), "{unknown}");
        assert!(unknown.to_string().contains("crash") && unknown.to_string().contains("kick, snare"), "{unknown}");
        assert!(check_assignments(&with(&["kick", "kick"]), &lanes).unwrap_err().is(patterns::LANE_ASSIGNED_TWICE));
    }

    #[test]
    fn drums_use_general_midi_numbers() {
        assert_eq!([gm_pitch("kick"), gm_pitch("snare"), gm_pitch("crash")], [36, 38, 49]);
        assert_eq!(gm_pitch("something_else"), 60);
    }
}
