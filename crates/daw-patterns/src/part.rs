//! `DrumPartBuilder`: "turn a sequence of patterns into note clips on drum tracks".
//!
//! Works only through the track, clip, note, sample and history interfaces. A part is built whole or not at all:
//! everything is checked up front and the edits go as one undo group; if something still fails halfway, the group is
//! rolled back. The request decides what plays each drum (`LaneTarget`, see `kit.rs`).

use std::sync::Arc;

use daw_contracts::codes::patterns;
use daw_contracts::{
    BAR_TICKS, ClipControl, ClipId, ContractError, DrumPartControl, DrumPartReport, DrumPartRequest, HistoryControl, InstrumentCatalog,
    L10n, NewNote, NewNoteClip, NewTrack, PatternLibrary, STEP_TICKS, SampleCatalog, TempoControl, TrackControl, TrackId,
};

use crate::kit::{LaneKit, LaneTrack};
use crate::plan::{Placement, overlapping, plan};

const LABEL: L10n = L10n { en: "Drum part", ru: "Партия ударных" };

/// Merges the notes of several drums on one track: two notes of the same pitch at the same moment become one, the louder.
fn merge_notes(mut notes: Vec<NewNote>) -> Vec<NewNote> {
    notes.sort_by_key(|n| (n.start_tick, n.pitch, std::cmp::Reverse(n.velocity)));
    notes.dedup_by_key(|n| (n.start_tick, n.pitch));
    notes
}

pub struct DrumPartBuilder {
    library: Arc<dyn PatternLibrary>,
    tracks: Arc<dyn TrackControl>,
    clips: Arc<dyn ClipControl>,
    history: Arc<dyn HistoryControl>,
    /// Tempo map: bar `start_bar` is found through the signature changes.
    tempo: Arc<dyn TempoControl>,
    kit: LaneKit,
}

impl DrumPartBuilder {
    pub fn new(
        library: Arc<dyn PatternLibrary>,
        tracks: Arc<dyn TrackControl>,
        clips: Arc<dyn ClipControl>,
        history: Arc<dyn HistoryControl>,
        instruments: Arc<dyn InstrumentCatalog>,
        samples: Arc<dyn SampleCatalog>,
        tempo: Arc<dyn TempoControl>,
    ) -> Self {
        let kit = LaneKit::new(tracks.clone(), instruments, samples);
        Self { library, tracks, clips, history, tempo, kit }
    }

    fn apply(
        &self,
        placements: &[Placement],
        lane_tracks: &mut [LaneTrack],
        to_remove: Vec<ClipId>,
    ) -> Result<DrumPartReport, ContractError> {
        let new_tracks: Vec<NewTrack> = lane_tracks.iter().filter_map(LaneTrack::new_track).cloned().collect();
        let created_names: Vec<String> = new_tracks.iter().map(|t| t.name.clone()).collect();
        let created_ids = self.tracks.add_tracks(new_tracks)?;
        for (lane, id) in lane_tracks.iter_mut().filter(|t| t.new_track().is_some()).zip(created_ids) {
            lane.created(id);
        }
        self.clips.remove_clips(to_remove)?;

        let mut clip_ids = Vec::new();
        for placement in placements {
            let mut per_track: Vec<(TrackId, Vec<NewNote>)> = Vec::new();
            for hits in &placement.pattern.lanes {
                let Some(target) = lane_tracks.iter().find(|t| t.lane == hits.lane && t.plays()) else { continue };
                let Some(track) = target.id() else { continue };
                let notes = hits.hits.iter().map(|hit| NewNote {
                    pitch: target.pitch(),
                    start_tick: hit.step * STEP_TICKS,
                    length_ticks: STEP_TICKS,
                    velocity: hit.velocity,
                });
                match per_track.iter_mut().find(|(t, _)| *t == track) {
                    Some((_, existing)) => existing.extend(notes),
                    None => per_track.push((track, notes.collect())),
                }
            }
            for (track, notes) in per_track {
                let clip = self.clips.add_note_clip(NewNoteClip {
                    track,
                    start_tick: placement.start_tick,
                    length_ticks: Some(placement.length_ticks()),
                    notes: merge_notes(notes),
                    loop_ticks: Some(placement.pattern.bars * BAR_TICKS),
                })?;
                clip_ids.push(clip.id);
            }
        }
        let mut tracks: Vec<TrackId> = Vec::new();
        for id in lane_tracks.iter().filter_map(LaneTrack::id) {
            if !tracks.contains(&id) {
                tracks.push(id);
            }
        }
        Ok(DrumPartReport {
            bars: placements.iter().map(|p| p.pattern.bars * p.repeat).sum(),
            clips: clip_ids,
            tracks,
            created_tracks: created_names,
        })
    }
}

impl DrumPartControl for DrumPartBuilder {
    fn build(&self, request: DrumPartRequest) -> Result<DrumPartReport, ContractError> {
        let map = self.tempo.tempo_map();
        let placements = plan(&*self.library, &request, &map)?;
        let mut lane_tracks = self.kit.resolve(&request, &placements)?;

        let start = placements.first().map_or(0, |p| p.start_tick);
        let end = placements.last().map_or(start, |p| p.start_tick + p.length_ticks());
        let clips = self.clips.clips();
        let mut to_remove: Vec<ClipId> = Vec::new();
        let mut checked: Vec<TrackId> = Vec::new();
        for lane in &lane_tracks {
            let Some(track) = lane.id() else { continue };
            if checked.contains(&track) {
                continue;
            }
            checked.push(track);
            let busy = overlapping(&clips, track, start, end);
            if !busy.is_empty() && !request.replace {
                return Err(patterns::PLACE_TAKEN
                    .with("name", lane.name())
                    .with("from", request.start_bar)
                    .with("to", map.bar_index(f64::from(end.saturating_sub(1)))));
            }
            to_remove.extend(busy);
        }

        self.history.begin_group(LABEL.get());
        let result = self.apply(&placements, &mut lane_tracks, to_remove);
        if result.is_err() {
            self.history.cancel_group();
        } else {
            self.history.end_group();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_drums_on_one_track_merge_into_one_set_of_notes_keeping_the_louder_hit() {
        let note = |start_tick, pitch, velocity| NewNote { pitch, start_tick, length_ticks: STEP_TICKS, velocity };
        let merged = merge_notes(vec![note(960, 38, 60), note(0, 38, 100), note(960, 38, 115), note(960, 40, 90)]);
        assert_eq!(
            merged.iter().map(|n| (n.start_tick, n.pitch, n.velocity)).collect::<Vec<_>>(),
            [(0, 38, 100), (960, 38, 115), (960, 40, 90)]
        );
    }
}
