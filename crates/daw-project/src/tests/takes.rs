//! The `TakeControl` contract (takes of an audio clip) and the input monitoring mode of an audio track
//! (`TrackControl::set_monitor`).

use super::*;

/// An audio track, a clip of the first sound from zero (1 s at 120 BPM = 1920 ticks) and two more take sounds.
fn rig_with_takes() -> (Rig, Clip, SampleId, SampleId) {
    let rig = Rig::new();
    rig.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let first = rig.catalog.import(Path::new("/p/take 1.wav")).unwrap().id;
    let second = rig.catalog.import(Path::new("/p/take 2.wav")).unwrap().id;
    let third = rig.catalog.import(Path::new("/p/take 3.wav")).unwrap().id;
    let track = rig.tracks.add_track("Voice", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, first).unwrap();
    (rig, clip, second, third)
}

fn sound(clip: &Clip) -> (SampleId, Vec<SampleId>) {
    match &clip.content {
        ClipContent::Audio { sample, takes, .. } => (*sample, takes.clone()),
        ClipContent::Notes(_) => unreachable!("an audio clip"),
    }
}

fn current(rig: &Rig, id: ClipId) -> Clip {
    rig.clips.clips().into_iter().find(|c| c.id == id).unwrap()
}

#[test]
fn a_new_take_plays_and_keeps_the_previous_one_without_moving_the_clip() {
    let (rig, clip, second, _) = rig_with_takes();
    let first = sound(&clip).0;
    let changed = rig.takes.add_take(clip.id, second).unwrap();
    assert_eq!(sound(&changed), (second, vec![first, second]));
    assert_eq!((changed.start_tick, changed.length_ticks), (clip.start_tick, clip.length_ticks), "the place and length are the same");
    assert_eq!(rig.last_snapshot().project.clip(clip.id).unwrap().content.sample(), Some(second), "the engine plays the new take");

    assert!(rig.service.undo(), "a take is one undo step");
    assert_eq!(sound(&current(&rig, clip.id)), (first, vec![]));
}

#[test]
fn switching_the_active_take_is_one_undo_step_and_the_same_take_is_no_step() {
    let (rig, clip, second, third) = rig_with_takes();
    let first = sound(&clip).0;
    rig.takes.add_take(clip.id, second).unwrap();
    rig.takes.add_take(clip.id, third).unwrap();
    let label = rig.service.undo_label();
    rig.takes.set_active_take(clip.id, third).unwrap();
    assert_eq!(rig.service.undo_label(), label, "an already active take records no undo step");

    assert_eq!(sound(&rig.takes.set_active_take(clip.id, first).unwrap()).0, first);
    assert!(rig.service.undo());
    assert_eq!(sound(&current(&rig, clip.id)).0, third);
    assert!(matches!(rig.takes.set_active_take(clip.id, 999), Err(e) if e.is(codes::clips::NOT_A_TAKE)), "not a take of the clip");
}

#[test]
fn removing_takes_falls_back_to_the_last_one_and_never_leaves_a_clip_without_sound() {
    let (rig, clip, second, third) = rig_with_takes();
    let first = sound(&clip).0;
    rig.takes.add_take(clip.id, second).unwrap();
    rig.takes.add_take(clip.id, third).unwrap();
    assert_eq!(sound(&rig.takes.remove_take(clip.id, third).unwrap()), (second, vec![first, second]), "the active take is removed");
    assert_eq!(sound(&rig.takes.remove_take(clip.id, first).unwrap()), (second, vec![]), "one is left: a plain clip");
    assert!(
        matches!(rig.takes.remove_take(clip.id, second), Err(e) if e.is(codes::clips::ONLY_SOUND)),
        "the clip's only sound is not removed"
    );
    assert!(rig.catalog.samples().iter().any(|s| s.id == third), "the sample stays in the project");
    assert!(rig.service.undo());
    assert_eq!(sound(&current(&rig, clip.id)), (second, vec![first, second]));
}

#[test]
fn takes_need_an_audio_clip_and_a_known_sample() {
    let (rig, clip, second, _) = rig_with_takes();
    assert!(matches!(rig.takes.add_take(clip.id, 999), Err(e) if e.is(codes::samples::NOT_FOUND)));
    assert!(matches!(rig.takes.add_take(999, second), Err(e) if e.is(codes::clips::NOT_FOUND)));
    let again = rig.takes.add_take(clip.id, sound(&clip).0);
    assert!(matches!(again, Err(e) if e.is(codes::clips::TAKE_ALREADY_ADDED)), "the sound is already in the clip");
    let (notes_rig, _, note_clip) = rig_with_clip();
    let sample = notes_rig.catalog.import(Path::new("/p/x.wav")).unwrap().id;
    assert!(matches!(notes_rig.takes.add_take(note_clip, sample), Err(e) if e.is(codes::clips::NOTE_CLIP_HAS_NO_TAKES)));
}

#[test]
fn removing_a_sample_drops_it_from_takes_and_removes_only_clips_left_without_sound() {
    let (rig, clip, second, _) = rig_with_takes();
    let first = sound(&clip).0;
    rig.takes.add_take(clip.id, second).unwrap();
    let lone = rig.clips.add_audio_clip(clip.track, 10_000, second).unwrap();
    rig.catalog.remove(second).unwrap();
    assert_eq!(sound(&current(&rig, clip.id)), (first, vec![]), "the clip went back to the remaining take");
    assert!(rig.clips.clips().iter().all(|c| c.id != lone.id), "the clip whose only sound it was is removed");
}

#[test]
fn takes_share_the_place_crossfades_and_warp_of_the_clip() {
    let (rig, clip, second, _) = rig_with_takes();
    let neighbour = rig.clips.add_audio_clip(clip.track, 1_500, sound(&clip).0).unwrap();
    let before = clip.crossfades(&rig.clips.clips());
    rig.clips.edit_clip(clip.id, ClipEdit { warp: Some(Some(WarpEdit { source_bpm: Some(120.0) })), ..Default::default() }).unwrap();
    rig.clips.edit_clip(clip.id, ClipEdit { pitch_semitones: Some(2.0), ..Default::default() }).unwrap();

    rig.takes.add_take(clip.id, second).unwrap();
    let changed = current(&rig, clip.id);
    assert_eq!(changed.crossfades(&rig.clips.clips()), before, "the crossfade with the neighbour is the same");
    assert_eq!((changed.warp, changed.pitch_semitones), (Some(Warp { source_bpm: 120.0 }), 2.0), "fitting and pitch are shared");
    assert!(rig.processed.kept.lock().unwrap().iter().any(|job| job.sample == second), "processing computes the new take's sound");
    assert!(rig.clips.clips().iter().any(|c| c.id == neighbour.id));
}

#[test]
fn monitor_mode_lives_on_the_audio_source_node_and_is_undoable() {
    let rig = Rig::new();
    let audio = rig.tracks.add_track("Voice", None, 60).unwrap();
    let node = source(&rig, audio);
    let monitor = |rig: &Rig| match rig.graph.graph().node(node).unwrap().kind {
        NodeKind::Audio { monitor, .. } => monitor,
        _ => unreachable!("an audio track node"),
    };
    assert_eq!(monitor(&rig), MonitorMode::Off, "off by default");
    rig.tracks.set_monitor(audio, MonitorMode::Auto).unwrap();
    assert_eq!(monitor(&rig), MonitorMode::Auto);
    let label = rig.service.undo_label();
    rig.tracks.set_monitor(audio, MonitorMode::Auto).unwrap();
    assert_eq!(rig.service.undo_label(), label, "the same mode records no undo step");
    assert!(rig.service.undo());
    assert_eq!(monitor(&rig), MonitorMode::Off);

    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    assert!(matches!(rig.tracks.set_monitor(synth, MonitorMode::On), Err(e) if e.is(codes::tracks::MONITOR_NEEDS_AUDIO_TRACK)));
    assert!(matches!(rig.tracks.set_monitor(999, MonitorMode::On), Err(e) if e.is(codes::tracks::NOT_FOUND)));
}

fn comp(clip: &Clip) -> Vec<(u32, SampleId)> {
    clip.comp().iter().map(|s| (s.start_ticks, s.take)).collect()
}

#[test]
fn choosing_a_take_range_is_one_undo_step_and_reaches_the_engine() {
    let (rig, clip, second, _) = rig_with_takes();
    let first = sound(&clip).0;
    rig.takes.add_take(clip.id, second).unwrap();
    let comped = rig.takes.choose_take_range(clip.id, 480, 960, first).unwrap();
    assert_eq!(comp(&comped), [(0, second), (480, first), (960, second)]);
    assert_eq!((comped.start_tick, comped.length_ticks), (clip.start_tick, clip.length_ticks), "the place and length are the same");
    assert_eq!(comp(rig.last_snapshot().project.clip(clip.id).unwrap()), comp(&comped), "the engine sees the comp");
    assert!(rig.service.undo(), "a range is one undo step");
    assert_eq!(comp(&current(&rig, clip.id)), []);

    let whole =
        rig.takes.set_comp(clip.id, vec![CompSegment { start_ticks: 0, take: first }, CompSegment { start_ticks: 960, take: second }]);
    assert_eq!(comp(&whole.unwrap()), [(0, first), (960, second)]);
    assert_eq!(sound(&rig.takes.set_active_take(clip.id, second).unwrap()).0, second);
    assert_eq!(comp(&current(&rig, clip.id)), [], "choosing a whole take drops the comp, even if it is already active");
    assert!(rig.service.undo());
    assert_eq!(comp(&current(&rig, clip.id)).len(), 2);
}

#[test]
fn comp_is_validated_and_follows_the_clip() {
    let (rig, clip, second, third) = rig_with_takes();
    let first = sound(&clip).0;
    rig.takes.add_take(clip.id, second).unwrap();
    let bad = |segments: Vec<(u32, SampleId)>| {
        let segments = segments.into_iter().map(|(start_ticks, take)| CompSegment { start_ticks, take }).collect();
        rig.takes.set_comp(clip.id, segments)
    };
    assert!(matches!(bad(vec![(10, first)]), Err(e) if e.is(codes::clips::COMP_NOT_FROM_START)), "not from zero");
    assert!(matches!(bad(vec![(0, first), (0, second)]), Err(e) if e.is(codes::clips::COMP_UNORDERED)), "not increasing");
    assert!(matches!(bad(vec![(0, first), (1920, second)]), Err(e) if e.is(codes::clips::COMP_PAST_END)), "beyond the clip end");
    assert!(matches!(bad(vec![(0, first), (100, third)]), Err(e) if e.is(codes::clips::NOT_A_TAKE)), "not a take of the clip");
    let empty = rig.takes.choose_take_range(clip.id, 900, 900, first);
    assert!(matches!(empty, Err(e) if e.is(codes::clips::TAKE_RANGE_EMPTY)), "an empty range");
    assert!(matches!(rig.takes.set_comp(999, vec![]), Err(e) if e.is(codes::clips::NOT_FOUND)));

    bad(vec![(0, first), (600, second), (1200, first)]).unwrap();
    let right = rig.clips.split_clip(clip.id, 900).unwrap();
    assert_eq!(comp(&current(&rig, clip.id)), [(0, first), (600, second)], "the left part is before the cut");
    assert_eq!(comp(&right), [(0, second), (300, first)], "the right part is shifted to its start");
    let shorter = rig.clips.edit_clip(clip.id, ClipEdit { length_ticks: Some(500), ..Default::default() }).unwrap();
    assert_eq!((comp(&shorter), sound(&shorter).0), (vec![], first), "shorter than the first joint: one take");
}

#[test]
fn a_comped_clip_with_processing_gets_the_sound_of_every_take_in_use() {
    let (rig, clip, second, third) = rig_with_takes();
    let first = sound(&clip).0;
    rig.takes.add_take(clip.id, second).unwrap();
    rig.takes.add_take(clip.id, third).unwrap();
    rig.clips.edit_clip(clip.id, ClipEdit { pitch_semitones: Some(2.0), ..Default::default() }).unwrap();
    rig.takes.choose_take_range(clip.id, 0, 960, first).unwrap();
    let snapshot = rig.last_snapshot();
    let mut keys: Vec<(ClipId, SampleId)> = snapshot.clip_audio.keys().copied().collect();
    keys.sort_unstable();
    assert_eq!(keys, [(clip.id, first), (clip.id, third)], "the first (the range) and the third (the rest) sound; the second does not");
    let mut kept: Vec<SampleId> = rig.processed.kept.lock().unwrap().iter().map(|job| job.sample).collect();
    kept.sort_unstable();
    assert_eq!(kept, [first, third]);
}
