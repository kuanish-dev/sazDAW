//! Building a drum part on a real session: tracks, clips, notes, undo in one step, refusals without traces.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: failing with a clear message is the check itself"
)]

use daw_bootstrap::build_session;
use daw_contracts::codes::{patterns, samples, tracks};
use daw_contracts::{
    BAR_TICKS, ClipContent, Controls, DrumPartRequest, LaneAssignment, LaneTarget, PatternSlot, STEP_TICKS, SignatureChange, TimeSignature,
};

use crate::common::param;

fn request(start_bar: u32, slots: &[(&str, u32)], replace: bool) -> DrumPartRequest {
    DrumPartRequest {
        start_bar,
        slots: slots.iter().map(|(pattern, repeat)| PatternSlot { pattern: (*pattern).into(), repeat: *repeat }).collect(),
        replace,
        lanes: vec![],
    }
}

fn track_of(controls: &Controls, kind: &str) -> u32 {
    controls.tracks.tracks().into_iter().find(|t| t.instrument.as_ref().is_some_and(|i| i.kind == kind)).unwrap().id
}

fn notes_of(controls: &Controls, kind: &str) -> Vec<(u32, u8)> {
    let track = track_of(controls, kind);
    let mut clips: Vec<_> = controls.clips.clips().into_iter().filter(|c| c.track == track).collect();
    clips.sort_by_key(|c| c.start_tick);
    clips.iter().flat_map(|c| controls.notes.notes(c.id).unwrap().into_iter().map(|n| (c.start_tick + n.start_tick, n.velocity))).collect()
}

#[test]
fn a_part_creates_the_drum_tracks_and_a_clip_per_drum_per_step() {
    let session = build_session(false);
    let c = &session.controls;
    let report = c.drum_part.build(request(0, &[("rock_basic", 3), ("fill_tom_cascade", 1)], false)).unwrap();

    assert_eq!(report.bars, 4);
    assert_eq!(report.created_tracks, ["Kick", "Snare", "Hi-hat", "High tom", "Mid tom", "Low tom"]);
    assert_eq!(report.clips.len(), 3 + 6);
    assert_eq!(c.tracks.tracks().len(), 6);

    let kick = track_of(c, "kick");
    let first = c.clips.clips().into_iter().filter(|c| c.track == kick).min_by_key(|c| c.start_tick).unwrap();
    assert_eq!((first.start_tick, first.length_ticks), (0, 3 * BAR_TICKS));
    let ClipContent::Notes(content) = &first.content else { panic!("a note clip") };
    assert_eq!(content.loop_ticks, BAR_TICKS);
    assert!(!content.auto_length, "the content length is fixed");

    let kick_notes = notes_of(c, "kick");
    assert_eq!(kick_notes[0], (0, 115));
    assert_eq!(kick_notes[1], (8 * STEP_TICKS, 115));
    assert!(kick_notes.iter().any(|(tick, _)| *tick == 3 * BAR_TICKS + 15 * STEP_TICKS), "a kick at the end of the fill");
}

#[test]
fn the_start_bar_counts_bars_of_the_signature_map() {
    let session = build_session(false);
    let c = &session.controls;
    let three_four = TimeSignature::new(3, 4).unwrap();
    c.tempo
        .set_signature_changes(vec![
            SignatureChange { tick: 0, signature: TimeSignature::FOUR_FOUR },
            SignatureChange { tick: 2 * BAR_TICKS, signature: three_four },
        ])
        .unwrap();
    c.drum_part.build(request(4, &[("rock_basic", 1)], false)).unwrap();
    let kick = track_of(c, "kick");
    let clip = c.clips.clips().into_iter().find(|clip| clip.track == kick).unwrap();
    assert_eq!(clip.start_tick, 2 * BAR_TICKS + 2 * 2880);

    let error = c.drum_part.build(request(3, &[("rock_basic", 1)], false)).unwrap_err();
    assert!(error.is(patterns::PLACE_TAKEN) && param(&error, "from") == Some("3"), "{error}");
}

#[test]
fn the_whole_part_is_one_undo_step() {
    let session = build_session(false);
    let c = &session.controls;
    c.drum_part.build(request(0, &[("rock_basic", 2), ("fill_snare_roll", 1)], false)).unwrap();
    assert!(!c.tracks.tracks().is_empty() && !c.clips.clips().is_empty());
    assert!(c.history.undo());
    assert!(c.tracks.tracks().is_empty() && c.clips.clips().is_empty(), "one undo removed both the tracks and the clips");
    assert!(c.history.redo());
    assert_eq!(c.clips.clips().len(), 3 + 3);
}

#[test]
fn a_part_that_fails_midway_leaves_the_previous_part_alone() {
    let session = build_session(false);
    let c = &session.controls;
    let filler: Vec<_> = (0..daw_contracts::MAX_TRACKS - 5)
        .map(|i| daw_contracts::NewTrack {
            name: format!("Empty {i}"),
            kind: None,
            default_pitch: 60,
            params: daw_contracts::Params::new(),
        })
        .collect();
    c.tracks.add_tracks(filler).unwrap();
    c.drum_part.build(request(0, &[("rock_basic", 1)], false)).unwrap();
    let after_first = (c.tracks.tracks(), c.clips.clips());

    assert!(c.drum_part.build(request(1, &[("fill_tom_cascade", 1)], false)).is_err());
    assert_eq!((c.tracks.tracks(), c.clips.clips()), after_first, "the first part is intact, the second left no traces");
    assert_eq!(c.history.undo_label().as_deref(), Some("Drum part"), "the first part's step is in place");
}

#[test]
fn existing_drum_tracks_are_reused_and_a_second_part_can_follow_the_first() {
    let session = build_session(false);
    let c = &session.controls;
    let kick = c.tracks.add_track("My kick", Some("kick"), 36).unwrap();
    let report = c.drum_part.build(request(0, &[("rock_basic", 1)], false)).unwrap();
    assert_eq!(report.created_tracks, ["Snare", "Hi-hat"], "the kick was already there");
    assert!(report.tracks.contains(&kick));
    c.drum_part.build(request(1, &[("break_stop", 1), ("intro_crash_start", 1)], false)).unwrap();
    assert_eq!(c.tracks.tracks().iter().filter(|t| t.instrument.as_ref().is_some_and(|i| i.kind == "kick")).count(), 1);
}

#[test]
fn busy_space_is_refused_unless_replace_is_set_and_nothing_changes_on_refusal() {
    let session = build_session(false);
    let c = &session.controls;
    c.drum_part.build(request(0, &[("rock_basic", 4)], false)).unwrap();
    let before = (c.tracks.tracks(), c.clips.clips());

    let error = c.drum_part.build(request(2, &[("funk_sixteenth", 1)], false)).unwrap_err();
    assert!(error.is(patterns::PLACE_TAKEN) && param(&error, "name") == Some("Kick"), "{error}");
    assert_eq!((c.tracks.tracks(), c.clips.clips()), before, "the refusal left nothing");

    c.drum_part.build(request(2, &[("funk_sixteenth", 1)], true)).unwrap();
    let kick = track_of(c, "kick");
    let mut spans: Vec<(u32, u32)> = c.clips.clips().iter().filter(|c| c.track == kick).map(|c| (c.start_tick, c.length_ticks)).collect();
    spans.sort_unstable();
    assert_eq!(spans, [(2 * BAR_TICKS, BAR_TICKS)], "the old clip is wholly replaced by the new one");
}

#[test]
fn an_unknown_pattern_or_empty_part_changes_nothing() {
    let session = build_session(false);
    let c = &session.controls;
    for slots in [&[("rock_basic", 1), ("no_such", 1)][..], &[][..]] {
        assert!(c.drum_part.build(request(0, slots, false)).is_err());
        assert!(c.tracks.tracks().is_empty() && c.clips.clips().is_empty());
        assert!(!c.history.can_undo(), "the error left no undo step");
    }
}

#[test]
fn a_later_part_in_the_song_starts_at_the_requested_bar() {
    let session = build_session(false);
    let c = &session.controls;
    c.drum_part.build(request(8, &[("house_four", 2)], false)).unwrap();
    let kick = track_of(c, "kick");
    let clip = c.clips.clips().into_iter().find(|c| c.track == kick).unwrap();
    assert_eq!((clip.start_tick, clip.length_ticks), (8 * BAR_TICKS, 2 * BAR_TICKS));
}

fn assign(request: DrumPartRequest, lanes: &[(&str, LaneTarget)]) -> DrumPartRequest {
    DrumPartRequest {
        lanes: lanes.iter().map(|(lane, target)| LaneAssignment { lane: (*lane).into(), target: *target }).collect(),
        ..request
    }
}

#[test]
fn a_drum_can_play_on_the_users_own_track_and_the_built_in_one_is_not_created() {
    let session = build_session(false);
    let c = &session.controls;
    let mine = c.tracks.add_track("My kick", Some("sampler"), 48).unwrap();
    let report = c
        .drum_part
        .build(assign(request(0, &[("rock_basic", 2)], false), &[("kick", LaneTarget::Track { id: mine, pitch: None })]))
        .unwrap();

    assert_eq!(report.created_tracks, ["Snare", "Hi-hat"], "the kick plays on your track, the built-in one is not created");
    assert!(report.tracks.contains(&mine));
    assert!(!c.tracks.tracks().iter().any(|t| t.instrument.as_ref().is_some_and(|i| i.kind == "kick")));
    let clip = c.clips.clips().into_iter().find(|c| c.track == mine).unwrap();
    let pitches: Vec<u8> = c.notes.notes(clip.id).unwrap().iter().map(|n| n.pitch).collect();
    assert_eq!(pitches, [48, 48], "the pitch is your track's step input pitch: the sample plays unshifted");
}

#[test]
fn the_pitch_of_a_users_track_can_be_overridden() {
    let session = build_session(false);
    let c = &session.controls;
    let mine = c.tracks.add_track("My snare", Some("sampler"), 60).unwrap();
    c.drum_part
        .build(assign(request(0, &[("rock_basic", 1)], false), &[("snare", LaneTarget::Track { id: mine, pitch: Some(72) })]))
        .unwrap();
    let clip = c.clips.clips().into_iter().find(|c| c.track == mine).unwrap();
    assert!(c.notes.notes(clip.id).unwrap().iter().all(|n| n.pitch == 72));
}

#[test]
fn skipped_drums_are_not_played_and_get_no_track() {
    let session = build_session(false);
    let c = &session.controls;
    let report = c.drum_part.build(assign(request(0, &[("intro_crash_start", 1)], false), &[("crash", LaneTarget::Skip)])).unwrap();
    assert_eq!(report.created_tracks, ["Kick", "Snare", "Hi-hat"]);
    assert!(!c.tracks.tracks().iter().any(|t| t.instrument.as_ref().is_some_and(|i| i.kind == "crash")));
    let everything_skipped = assign(request(0, &[("break_stop", 1)], false), &[("kick", LaneTarget::Skip), ("crash", LaneTarget::Skip)]);
    assert!(c.drum_part.build(everything_skipped).unwrap_err().is(patterns::ALL_LANES_SKIPPED));
}

#[test]
fn several_drums_can_share_one_track_and_get_one_clip_per_step() {
    let session = build_session(false);
    let c = &session.controls;
    let shared = c.tracks.add_track("All on one", Some("sampler"), 60).unwrap();
    let to_shared = |lane| (lane, LaneTarget::Track { id: shared, pitch: None });
    let report = c
        .drum_part
        .build(assign(request(0, &[("rock_basic", 1)], false), &[to_shared("kick"), to_shared("snare"), to_shared("hat")]))
        .unwrap();
    assert!(report.created_tracks.is_empty());
    assert_eq!(report.clips.len(), 1, "three drums on one track make one clip");
    let notes = c.notes.notes(report.clips[0]).unwrap();
    let mut starts: Vec<u32> = notes.iter().map(|n| n.start_tick / STEP_TICKS).collect();
    starts.dedup();
    assert_eq!(starts, (0..16).step_by(2).collect::<Vec<u32>>(), "and exactly one note per moment");
    assert_eq!(notes.iter().find(|n| n.start_tick == 0).unwrap().velocity, 115, "kick (strong) and hat (normal) coincide: the louder wins");
}

#[test]
fn wrong_targets_are_refused_before_anything_changes() {
    let session = build_session(false);
    let c = &session.controls;
    let audio = c.tracks.add_track("Audio", None, 60).unwrap();
    let before = (c.tracks.tracks(), c.clips.clips());
    for (lanes, expected) in [
        (vec![("kick", LaneTarget::Track { id: audio, pitch: None })], patterns::LANE_ON_AUDIO_TRACK),
        (vec![("kick", LaneTarget::Track { id: 9_999, pitch: None })], tracks::NOT_FOUND),
        (vec![("kick", LaneTarget::Sample { id: 9_999 })], samples::NOT_FOUND),
        (vec![("crash", LaneTarget::Skip)], patterns::LANE_NOT_IN_PART),
    ] {
        let error = c.drum_part.build(assign(request(0, &[("rock_basic", 1)], false), &lanes)).unwrap_err();
        assert!(error.is(expected), "{error}");
        assert_eq!((c.tracks.tracks(), c.clips.clips()), before, "the refusal left nothing");
    }
}
