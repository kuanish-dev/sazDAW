//! The `ClipControl` contract: clips on the timeline and the loop.

use super::*;

#[test]
fn clip_kind_must_match_the_track_kind() {
    let rig = Rig::new();
    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let audio = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();

    assert!(
        matches!(rig.clips.add_note_clip(NewNoteClip::empty(audio, 0, None)), Err(e) if e.is(codes::clips::NOTE_CLIP_NEEDS_INSTRUMENT_TRACK)),
        "a note clip on an audio track"
    );
    assert!(
        matches!(rig.clips.add_audio_clip(synth, 0, sample.id), Err(e) if e.is(codes::clips::AUDIO_CLIP_NEEDS_AUDIO_TRACK)),
        "an audio clip on an instrument track"
    );
    assert!(rig.clips.add_note_clip(NewNoteClip::empty(9999, 0, None)).is_err());
    let notes = rig.clips.add_note_clip(NewNoteClip::empty(synth, 0, None)).unwrap();
    let wave = rig.clips.add_audio_clip(audio, 0, sample.id).unwrap();

    let to_audio = ClipEdit { track: Some(audio), ..Default::default() };
    let to_synth = ClipEdit { track: Some(synth), ..Default::default() };
    assert!(rig.clips.edit_clip(notes.id, to_audio).is_err(), "moving to a track of another kind");
    assert!(rig.clips.edit_clip(wave.id, to_synth).is_err());
    assert!(matches!(rig.notes.notes(wave.id), Err(e) if e.is(codes::notes::AUDIO_CLIP_HAS_NO_NOTES)), "an audio clip has no notes");
    assert!(rig.notes.add_note(wave.id, note(60, 0, 10)).is_err());

    let other = rig.tracks.add_track("Another synth", Some("synth"), 60).unwrap();
    let moved = rig.clips.edit_clip(notes.id, ClipEdit { track: Some(other), ..Default::default() }).unwrap();
    assert_eq!(moved.track, other);
}

#[test]
fn note_clips_start_empty_and_repeat_their_content_metadata() {
    let (rig, track, clip) = rig_with_clip();
    let created = rig.clips.clips().into_iter().find(|c| c.id == clip).unwrap();
    assert_eq!((created.track, created.start_tick, created.length_ticks), (track, 0, BAR_TICKS));
    assert_eq!(created.content, ClipContent::Notes(NoteContent::empty()));

    rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    rig.clips.edit_clip(clip, ClipEdit { length_ticks: Some(BAR_TICKS * 8), ..Default::default() }).unwrap();
    let stretched = content(&rig, clip);
    assert_eq!(stretched.loop_ticks, BAR_TICKS);
    assert_eq!(rig.clips.clips()[0].length_ticks, BAR_TICKS * 8);

    let long = rig.clips.add_note_clip(NewNoteClip::empty(track, BAR_TICKS * 8, Some(BAR_TICKS * 4))).unwrap();
    assert_eq!((long.length_ticks, content(&rig, long.id).loop_ticks), (BAR_TICKS * 4, BAR_TICKS));
}

#[test]
fn clips_reject_overlaps_and_bad_placement() {
    let (rig, track, clip) = rig_with_clip();
    assert!(rig.clips.add_note_clip(NewNoteClip::empty(track, 100, None)).is_err(), "overlaps the first one");
    let next = rig.clips.add_note_clip(NewNoteClip::empty(track, BAR_TICKS, None)).unwrap();
    assert!(
        matches!(rig.clips.add_note_clip(NewNoteClip::empty(track, 0, Some(0))), Err(e) if e.is(codes::clips::RANGE_OUT_OF_BOUNDS)),
        "zero length"
    );
    assert!(rig.clips.add_note_clip(NewNoteClip::empty(track, u32::MAX, None)).is_err());

    assert!(rig.clips.edit_clip(clip, ClipEdit { length_ticks: Some(BAR_TICKS + 1), ..Default::default() }).is_err());
    assert!(rig.clips.edit_clip(clip, ClipEdit { start_tick: Some(BAR_TICKS), ..Default::default() }).is_err());
    assert!(rig.clips.edit_clip(next.id, ClipEdit { gain: Some(9.0), ..Default::default() }).is_err());
    assert!(rig.clips.edit_clip(9999, ClipEdit::default()).is_err());
    let other = rig.tracks.add_track("Other", Some("synth"), 60).unwrap();
    let moved = rig.clips.edit_clip(next.id, ClipEdit { track: Some(other), start_tick: Some(0), ..Default::default() }).unwrap();
    assert_eq!((moved.track, moved.start_tick), (other, 0));

    rig.clips.remove_clip(next.id).unwrap();
    assert!(rig.clips.remove_clip(next.id).is_err());
}

#[test]
fn splitting_a_note_clip_copies_notes_and_sets_the_offset() {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.add_notes(clip, vec![note(60, 0, 240), note(62, 960, 240)]).unwrap();
    rig.clips.edit_clip(clip, ClipEdit { length_ticks: Some(BAR_TICKS * 3), ..Default::default() }).unwrap();

    let right = rig.clips.split_clip(clip, 960).unwrap();
    assert_eq!((right.start_tick, right.length_ticks, right.offset_ticks), (960, BAR_TICKS * 3 - 960, 960));
    let left = rig.clips.clips().into_iter().find(|c| c.id == clip).unwrap();
    assert_eq!((left.length_ticks, left.offset_ticks), (960, 0));
    let (ClipContent::Notes(l), ClipContent::Notes(r)) = (&left.content, &right.content) else { panic!("note clips") };
    assert_eq!(l.notes, r.notes, "both halves got a copy of the notes");
    assert_eq!(r.notes.len(), 2);

    let again = rig.clips.split_clip(right.id, BAR_TICKS + 480).unwrap();
    assert_eq!(again.offset_ticks, 480, "(960 + 3360 - 960) % bar");

    assert!(matches!(rig.clips.split_clip(clip, 0), Err(e) if e.is(codes::clips::SPLIT_OUTSIDE_CLIP)));
    assert!(rig.clips.split_clip(clip, 5000).is_err());
    assert!(rig.clips.split_clip(9999, 10).is_err());
    assert!(rig.service.undo(), "a split is one step");
    assert_eq!(rig.clips.clips().len(), 2);
}

#[test]
fn clips_can_be_edited_with_gain_and_fades() {
    let (rig, _, clip) = rig_with_clip();
    let edited = rig
        .clips
        .edit_clip(clip, ClipEdit { gain: Some(0.5), fade_in_ticks: Some(120), fade_out_ticks: Some(99_999), ..Default::default() })
        .unwrap();
    assert_eq!((edited.gain, edited.fade_in_ticks, edited.fade_out_ticks), (0.5, 120, BAR_TICKS), "a fade is no longer than the clip");
    assert!(rig.clips.edit_clip(clip, ClipEdit { gain: Some(f32::NAN), ..Default::default() }).is_err());
}

#[test]
fn duplicate_clip_deep_copies_and_fails_when_the_place_is_taken() {
    let (rig, track, clip) = rig_with_clip();
    let note = rig.notes.add_note(clip, note(60, 0, 240)).unwrap();
    let copy = rig.clips.duplicate_clip(clip).unwrap();
    assert_ne!(copy.id, clip);
    assert_eq!((copy.track, copy.start_tick, copy.length_ticks), (track, BAR_TICKS, BAR_TICKS));
    assert_eq!(rig.notes.notes(copy.id).unwrap(), [note], "the copy has the same notes");

    rig.notes.remove_note(copy.id, note.id).unwrap();
    assert_eq!(rig.notes.notes(clip).unwrap().len(), 1);

    assert!(rig.clips.duplicate_clip(clip).is_err(), "the place is taken by the copy");
    assert!(rig.clips.duplicate_clip(9999).is_err());
    assert_eq!(rig.clips.clips().len(), 2);
    let third = rig.clips.duplicate_clip(copy.id).unwrap();
    assert_eq!(third.start_tick, BAR_TICKS * 2);
}

#[test]
fn loop_region_always_reaches_the_transport() {
    let rig = Rig::new();
    assert_eq!(rig.clips.loop_region(), None);
    rig.clips.set_loop_region(Some((0, 7680))).unwrap();
    assert_eq!(rig.clips.loop_region(), Some((0, 7680)));
    assert_eq!(rig.transport.loop_region(), Some((0, 7680)), "there is no mode: the loop always works");
    assert_eq!(rig.last_snapshot().project.loop_region, Some((0, 7680)));
    rig.clips.set_loop_region(None).unwrap();
    assert_eq!(rig.transport.loop_region(), None);
    assert!(matches!(rig.clips.set_loop_region(Some((10, 10))), Err(e) if e.is(codes::clips::LOOP_REGION_BACKWARDS)));
    assert!(rig.clips.set_loop_region(Some((20, 10))).is_err());
}

#[test]
fn the_song_end_reaches_the_transport_rounded_up_to_a_bar() {
    let rig = Rig::new();
    let bar = u64::from(BAR_TICKS);
    assert_eq!(rig.transport.wrap_region(), None, "an empty arrangement has nothing to loop");
    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let clip = rig.clips.add_note_clip(NewNoteClip::empty(synth, BAR_TICKS, Some(BAR_TICKS + 960))).unwrap();
    assert_eq!(rig.transport.wrap_region(), Some((0, 3 * bar)), "the song loops at the end of the bar its last clip ends in");

    rig.clips.set_loop_region(Some((0, BAR_TICKS))).unwrap();
    assert_eq!(rig.transport.wrap_region(), Some((0, bar)), "the loop region wins");
    rig.clips.set_loop_region(None).unwrap();

    rig.clips.remove_clip(clip.id).unwrap();
    assert_eq!(rig.transport.wrap_region(), None);
    assert!(rig.service.undo());
    assert_eq!(rig.transport.wrap_region(), Some((0, 3 * bar)), "undo brings the song end back too");
}

#[test]
fn audio_clips_get_their_length_from_the_sample_and_tempo() {
    let rig = Rig::new();
    rig.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let track = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, sample.id).unwrap();
    assert_eq!(clip.length_ticks, 1920);
    assert_eq!(clip.content, ClipContent::audio(sample.id));
    assert!(matches!(rig.clips.add_audio_clip(track, 5000, 424242), Err(e) if e.is(codes::samples::NOT_FOUND)));
    assert!(
        matches!(rig.clips.add_audio_clip(track, 0, sample.id), Err(e) if e.is(codes::clips::AUDIO_CLIP_INSIDE_ANOTHER)),
        "with the same start it lies inside the other"
    );
    let right = rig.clips.split_clip(clip.id, 960).unwrap();
    assert_eq!(right.offset_frames, 24_000, "half a second from the file start");
    assert_eq!(right.offset_ticks, 0, "an audio clip has no note phase");
}

#[test]
fn splitting_an_audio_clip_whose_sound_is_gone_is_not_found() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let sample = rig.catalog.import(Path::new("/music/vocal.wav")).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, sample.id).unwrap();
    rig.samples.remove(sample.id);
    assert!(matches!(rig.clips.split_clip(clip.id, 100), Err(e) if e.is(codes::samples::NOT_FOUND)));
    assert_eq!(rig.clips.clips(), [clip], "the clip is untouched");
}

fn clip_with(track: TrackId, start_tick: u32, notes: Vec<NewNote>, loop_ticks: Option<u32>) -> NewNoteClip {
    NewNoteClip { track, start_tick, length_ticks: Some(BAR_TICKS * 4), notes, loop_ticks }
}

#[test]
fn note_clip_with_content_equals_empty_clip_then_replace_then_loop() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let notes = vec![note(60, 0, 240), note(62, 1800, 600), note(64, 2000, 240)];
    for loop_ticks in [None, Some(1920)] {
        let start = if loop_ticks.is_none() { 0 } else { BAR_TICKS * 4 };
        let one = rig.clips.add_note_clip(clip_with(track, start, notes.clone(), loop_ticks)).unwrap();

        let three = rig.clips.add_note_clip(NewNoteClip::empty(track, start + BAR_TICKS * 8, Some(BAR_TICKS * 4))).unwrap();
        rig.notes.replace_notes(three.id, notes.clone()).unwrap();
        if let Some(loop_ticks) = loop_ticks {
            rig.notes.set_loop_ticks(three.id, loop_ticks).unwrap();
        }
        assert_eq!(content(&rig, one.id), content(&rig, three.id), "loop {loop_ticks:?}");
        assert_eq!(one.content, ClipContent::Notes(content(&rig, one.id)), "the created clip is returned");
    }
    let auto = rig.clips.clips()[0].clone();
    let ClipContent::Notes(auto) = auto.content else { panic!("a note clip") };
    assert_eq!(
        (auto.loop_ticks, auto.auto_length, auto.notes.len()),
        (BAR_TICKS, true, 3),
        "the automatic length reaches the end of the notes' bar"
    );
}

#[test]
fn note_clip_with_content_is_one_undo_step() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let before = rig.service.undo_label();
    let clip = rig.clips.add_note_clip(clip_with(track, 0, vec![note(60, 0, 240)], Some(960))).unwrap();
    assert_eq!(content(&rig, clip.id).loop_ticks, 960);
    assert!(rig.service.undo());
    assert!(rig.clips.clips().is_empty(), "one undo removes the clip with its notes");
    assert_eq!(rig.service.undo_label(), before);
}

#[test]
fn note_clip_with_bad_content_creates_nothing() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let before = rig.service.undo_label();
    let rejected = [
        clip_with(track, 0, vec![note(60, 0, 240), NewNote { velocity: 0, ..note(62, 240, 240) }], None),
        clip_with(track, 0, vec![note(60, MAX_CLIP_LOOP_TICKS, 240)], None),
        clip_with(track, 0, vec![note(60, 0, 240)], Some(100)),
        clip_with(track, 0, vec![note(60, 0, 240)], Some(0)),
        clip_with(track, 0, vec![note(60, 0, 240)], Some(MAX_CLIP_LOOP_TICKS + STEP_TICKS)),
        clip_with(track, 0, (0..=MAX_NOTES_PER_CLIP as u32).map(|i| note(60, i, 1)).collect(), None),
        clip_with(9999, 0, vec![note(60, 0, 240)], None),
    ];
    for clip in rejected {
        assert!(rig.clips.add_note_clip(clip.clone()).is_err(), "{:?}", clip.loop_ticks);
    }
    assert!(matches!(
        rig.clips.add_note_clip(clip_with(track, 0, (0..=MAX_NOTES_PER_CLIP as u32).map(|i| note(60, i, 1)).collect(), None)),
        Err(e) if e.is(codes::notes::TOO_MANY_NOTES)
    ));
    assert!(rig.clips.clips().is_empty(), "no clips");
    assert_eq!(rig.service.undo_label(), before, "no undo steps");

    let full: Vec<NewNote> = (0..MAX_NOTES_PER_CLIP as u32).map(|i| note(60, i, 1)).collect();
    assert_eq!(content(&rig, rig.clips.add_note_clip(clip_with(track, 0, full, None)).unwrap().id).notes.len(), MAX_NOTES_PER_CLIP);
}

#[test]
fn fade_shapes_are_edited_and_kept() {
    let (rig, _, clip) = rig_with_clip();
    let edited = rig
        .clips
        .edit_clip(
            clip,
            ClipEdit { fade_in_shape: Some(FadeShape::SCurve), fade_out_shape: Some(FadeShape::Exponential), ..Default::default() },
        )
        .unwrap();
    assert_eq!((edited.fade_in_shape, edited.fade_out_shape), (FadeShape::SCurve, FadeShape::Exponential));
    let moved = rig.clips.edit_clip(clip, ClipEdit { start_tick: Some(BAR_TICKS), ..Default::default() }).unwrap();
    assert_eq!((moved.fade_in_shape, moved.fade_out_shape), (FadeShape::SCurve, FadeShape::Exponential), "other edits keep the shapes");
}

/// An audio track with a one-second clip (1920 ticks at 120 BPM, 48 000 frames) at `start`.
fn audio_clip_at(start: u32) -> (Rig, TrackId, Clip) {
    let rig = Rig::new();
    rig.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let track = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, start, sample.id).unwrap();
    (rig, track, clip)
}

#[test]
fn trimming_the_start_moves_the_offset_with_it_and_keeps_the_end() {
    let (rig, _, clip) = audio_clip_at(960);
    let trimmed = rig.clips.trim_start(clip.id, 1440).unwrap();
    assert_eq!((trimmed.start_tick, trimmed.length_ticks), (1440, 1440), "the end stays at 2880");
    assert_eq!(trimmed.offset_frames, 12_000, "a quarter second of sound is cut off");

    let back = rig.clips.trim_start(clip.id, 1200).unwrap();
    assert_eq!((back.start_tick, back.offset_frames), (1200, 6_000), "the beginning comes back");
    let clamped = rig.clips.trim_start(clip.id, 0).unwrap();
    assert_eq!((clamped.start_tick, clamped.length_ticks, clamped.offset_frames), (960, 1920, 0), "nothing before the file start");

    let past = rig.clips.trim_start(clip.id, 2880);
    assert!(matches!(past, Err(e) if e.is(codes::clips::TRIM_PAST_END)));
    assert!(rig.service.undo());
    assert_eq!(rig.clips.clips()[0].start_tick, 1200, "each trim is one undo step");
}

#[test]
fn trimming_a_note_clip_turns_its_loop_phase() {
    let (rig, _, clip) = rig_with_clip();
    let trimmed = rig.clips.trim_start(clip, 960).unwrap();
    assert_eq!((trimmed.start_tick, trimmed.length_ticks, trimmed.offset_ticks), (960, BAR_TICKS - 960, 960));
    let moved = rig.clips.edit_clip(clip, ClipEdit { start_tick: Some(BAR_TICKS), ..Default::default() }).unwrap();
    let extended = rig.clips.trim_start(clip, moved.start_tick - 1920).unwrap();
    assert_eq!(extended.offset_ticks, BAR_TICKS - 960, "revealing wraps around the loop");
    assert!(rig.clips.trim_start(9999, 0).is_err());
}

#[test]
fn trimming_checks_the_place_on_the_track() {
    let (rig, track, clip) = rig_with_clip();
    let next = rig.clips.add_note_clip(NewNoteClip::empty(track, BAR_TICKS, None)).unwrap();
    let overlap = rig.clips.trim_start(next.id, BAR_TICKS / 2);
    assert!(matches!(overlap, Err(e) if e.is(codes::clips::NOTE_CLIPS_OVERLAP)));
    assert_eq!(rig.clips.clips().iter().find(|c| c.id == next.id), Some(&next), "the failed trim changed nothing");
    assert!(rig.clips.trim_start(clip, BAR_TICKS / 2).is_ok());
}
