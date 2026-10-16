//! Tempo fitting of audio clips, pitch shift and crossfades of overlapping audio clips (`ClipControl`), and the
//! processed sound in the engine snapshot.

use std::sync::Arc;

use super::*;

/// An audio track and a clip of the whole sound (1 second, 48 kHz) from zero at 120 BPM.
fn rig_with_audio(file: &str) -> (Rig, TrackId, Clip) {
    let rig = Rig::new();
    rig.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = rig.catalog.import(Path::new(file)).unwrap();
    let track = rig.tracks.add_track("Loop", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, sample.id).unwrap();
    (rig, track, clip)
}

fn warp(source_bpm: Option<f64>) -> ClipEdit {
    ClipEdit { warp: Some(Some(WarpEdit { source_bpm })), ..Default::default() }
}

#[test]
fn warp_guesses_the_tempo_from_the_name_and_keeps_the_same_piece_of_sound() {
    let (rig, _, clip) = rig_with_audio("/music/loop 60bpm.wav");
    assert_eq!(clip.length_ticks, 1920, "1 s at 120 BPM");
    let warped = rig.clips.edit_clip(clip.id, warp(None)).unwrap();
    assert_eq!(warped.warp, Some(Warp { source_bpm: 60.0 }));
    assert_eq!(warped.length_ticks, 960, "1 s at 60 BPM is one beat");
    let job = ProcessJob::new(warped_sample(&warped), 0.5, 0.0);
    assert_plays(&rig, &warped, &job);
    assert_eq!(*rig.processed.kept.lock().unwrap(), [job], "the cache needs only this: at 120 BPM a 60 BPM sound is half as long");

    // The tempo changed: the length in ticks is the same, the sound stretches differently; with equal tempos no
    // processing is needed.
    rig.tempo.set_tempo(Bpm::new(60.0).unwrap()).unwrap();
    rig.service.refresh_clip_audio();
    assert_eq!(rig.clips.clips()[0].length_ticks, 960);
    assert_unprocessed(&rig, clip.id);
}

/// Tempos match and the pitch is not shifted: the clip plays the sample itself without processing.
fn assert_unprocessed(rig: &Rig, clip: ClipId) {
    let snapshot = rig.last_snapshot();
    let sample = warped_sample(rig.clips.clips().iter().find(|c| c.id == clip).unwrap());
    let audio = &snapshot.clip_audio[&(clip, sample)];
    assert!(Arc::ptr_eq(&audio.data, &snapshot.samples[&sample]) && audio.start_frame == 0.0);
    assert!(rig.processed.kept.lock().unwrap().is_empty(), "the cache needs nothing");
}

/// In the snapshot the clip has the sound computed for job `job`.
fn assert_plays(rig: &Rig, clip: &Clip, job: &ProcessJob) {
    let audio = &rig.last_snapshot().clip_audio[&(clip.id, warped_sample(clip))];
    assert!(Arc::ptr_eq(&audio.data, &rig.processed.made(job).expect("the job was computed")), "the sound of job {job:?}");
}

fn warped_sample(clip: &Clip) -> SampleId {
    match clip.content {
        ClipContent::Audio { sample, .. } => sample,
        ClipContent::Notes(_) => unreachable!("an audio clip"),
    }
}

#[test]
fn explicit_tempo_and_turning_warp_off_recount_the_length() {
    let (rig, _, clip) = rig_with_audio("/music/loop.wav");
    let warped = rig.clips.edit_clip(clip.id, warp(Some(90.0))).unwrap();
    assert_eq!(warped.length_ticks, 1440, "1 s at 90 BPM = 1.5 beats");
    let explicit = rig.clips.edit_clip(clip.id, ClipEdit { length_ticks: Some(700), ..warp(Some(120.0)) }).unwrap();
    assert_eq!(explicit.length_ticks, 700, "an explicit length wins");
    let off = rig.clips.edit_clip(clip.id, ClipEdit { warp: Some(None), ..Default::default() }).unwrap();
    assert_eq!((off.warp, off.length_ticks), (None, 700), "120 → 120: the same piece of sound");
    assert!(matches!(rig.clips.edit_clip(clip.id, warp(Some(5.0))), Err(e) if e.is(codes::tempo::INVALID_TEMPO)));
}

#[test]
fn a_name_without_tempo_and_an_odd_length_falls_back_to_the_project_tempo() {
    // 1 second is a whole number of bars only at 240+ BPM: nothing to guess from.
    let (rig, _, clip) = rig_with_audio("/music/vocal.wav");
    let warped = rig.clips.edit_clip(clip.id, warp(None)).unwrap();
    assert_eq!(warped.warp, Some(Warp { source_bpm: 120.0 }));
    assert_eq!(warped.length_ticks, 1920);
    assert_unprocessed(&rig, clip.id);
}

#[test]
fn pitch_is_checked_and_only_for_audio_clips() {
    let (rig, track, clip) = rig_with_audio("/music/loop.wav");
    let shifted = rig.clips.edit_clip(clip.id, ClipEdit { pitch_semitones: Some(-3.5), ..Default::default() }).unwrap();
    assert_eq!((shifted.pitch_semitones, shifted.length_ticks), (-3.5, 1920), "the length does not change");
    assert_plays(&rig, &shifted, &ProcessJob::new(warped_sample(&shifted), 1.0, -3.5));
    let too_far = ClipEdit { pitch_semitones: Some(30.0), ..Default::default() };
    assert!(matches!(rig.clips.edit_clip(clip.id, too_far), Err(e) if e.is(codes::clips::PITCH_OUT_OF_RANGE)));
    let nan = ClipEdit { pitch_semitones: Some(f32::NAN), ..Default::default() };
    assert!(matches!(rig.clips.edit_clip(clip.id, nan), Err(e) if e.is(codes::clips::PITCH_NOT_A_NUMBER)));

    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let notes = rig.clips.add_note_clip(NewNoteClip::empty(synth, 0, None)).unwrap();
    let pitch = ClipEdit { pitch_semitones: Some(2.0), ..Default::default() };
    assert!(matches!(rig.clips.edit_clip(notes.id, pitch), Err(e) if e.is(codes::clips::WARP_NEEDS_AUDIO_CLIP)));
    assert!(matches!(rig.clips.edit_clip(notes.id, warp(Some(100.0))), Err(e) if e.is(codes::clips::WARP_NEEDS_AUDIO_CLIP)));
    assert_eq!(rig.clips.clips().iter().filter(|c| c.track == track).count(), 1);
}

#[test]
fn while_the_new_sound_is_computed_the_clip_keeps_the_previous_one() {
    let (rig, _, clip) = rig_with_audio("/music/loop 60bpm.wav");
    rig.clips.edit_clip(clip.id, warp(None)).unwrap();
    let old = ProcessJob::new(warped_sample(&clip), 0.5, 0.0);
    assert_plays(&rig, &clip, &old);
    *rig.processed.ready.lock().unwrap() = false;
    rig.tempo.set_tempo(Bpm::new(100.0).unwrap()).unwrap();
    rig.service.refresh_clip_audio();
    assert_plays(&rig, &clip, &old);
    let new = ProcessJob::new(warped_sample(&clip), 0.6, 0.0);
    assert_eq!(*rig.processed.kept.lock().unwrap(), std::slice::from_ref(&new), "only the new one is needed");
    *rig.processed.ready.lock().unwrap() = true;
    rig.service.refresh_clip_audio();
    assert_plays(&rig, &clip, &new);
}

#[test]
fn splitting_a_warped_clip_counts_frames_by_its_own_tempo() {
    let (rig, _, clip) = rig_with_audio("/music/loop 60bpm.wav");
    rig.clips.edit_clip(clip.id, warp(None)).unwrap();
    let right = rig.clips.split_clip(clip.id, 480).unwrap();
    assert_eq!(right.offset_frames, 24_000, "half a beat at 60 BPM is half a second");
    assert_eq!(right.warp, Some(Warp { source_bpm: 60.0 }));
}

#[test]
fn audio_clips_may_overlap_by_their_edges_only() {
    let (rig, track, first) = rig_with_audio("/music/loop.wav");
    let sample = warped_sample(&first);
    let second = rig.clips.add_audio_clip(track, 1500, sample).unwrap();
    assert_eq!(second.crossfades(&rig.clips.clips()), (420, 0), "the overlap 1500..1920 is a crossfade");
    assert_eq!(first.crossfades(&rig.clips.clips()), (0, 420));
    assert!(rig.clips.add_audio_clip(track, 1500, sample).is_err(), "the same start");
    let third = rig.clips.add_audio_clip(track, 1800, sample);
    assert!(matches!(third, Err(e) if e.is(codes::clips::TOO_MANY_AUDIO_CLIPS_AT_ONCE)), "a third clip at one point");
    let inside = ClipEdit { start_tick: Some(100), length_ticks: Some(500), ..Default::default() };
    assert!(rig.clips.edit_clip(second.id, inside).is_err(), "inside another");
    let around = ClipEdit { start_tick: Some(0), length_ticks: Some(5000), ..Default::default() };
    assert!(rig.clips.edit_clip(second.id, around).is_err(), "another inside");
    // A third clip after the end of the first overlaps only the second: two at every point.
    let third = rig.clips.add_audio_clip(track, 3000, sample).unwrap();
    assert_eq!(third.crossfades(&rig.clips.clips()), (420, 0));
    // A batch edit is checked on the result: the second may overlap the edges of both neighbours, but not cover the
    // third entirely.
    let both_edges = ClipEdit { start_tick: Some(1000), length_ticks: Some(2500), ..Default::default() };
    rig.clips.edit_clips(vec![(second.id, both_edges)]).unwrap();
    let wide = ClipEdit { start_tick: Some(1000), length_ticks: Some(4500), ..Default::default() };
    assert!(rig.clips.edit_clips(vec![(second.id, wide)]).is_err());
    assert!(rig.clips.copy_clips(vec![first.id], 100).is_err(), "a copy inside others");
}

#[test]
fn note_clips_still_must_not_overlap() {
    let (rig, track, _) = rig_with_clip();
    assert!(rig.clips.add_note_clip(NewNoteClip::empty(track, BAR_TICKS / 2, None)).is_err());
    assert!(rig.clips.add_note_clip(NewNoteClip::empty(track, BAR_TICKS, None)).is_ok(), "edge to edge is fine");
}

#[test]
fn a_tempo_change_inside_a_warped_clip_asks_for_a_curve_and_moves_with_the_clip() {
    let (rig, _, clip) = rig_with_audio("/music/loop 60bpm.wav");
    rig.clips.edit_clip(clip.id, ClipEdit { length_ticks: Some(1920), ..warp(None) }).unwrap();
    let job = |rig: &Rig| rig.processed.kept.lock().unwrap().first().cloned().expect("a job");
    assert!(job(&rig).curve().is_none(), "one tempo inside the clip: one stretch");
    let changes = vec![
        TempoChange { tick: 0, bpm: Bpm::new(120.0).unwrap(), ramp: false },
        TempoChange { tick: 960, bpm: Bpm::new(90.0).unwrap(), ramp: false },
    ];
    rig.tempo.set_tempo_changes(changes).unwrap();
    rig.service.refresh_clip_audio();
    let curved = job(&rig);
    assert_eq!(curved.curve().map(<[StretchAnchor]>::len), Some(3), "start, tempo change, end");
    assert_plays(&rig, &rig.clips.clips()[0], &curved);
    assert_eq!(rig.last_snapshot().clip_audio[&(clip.id, warped_sample(&clip))].start_frame, 0.0, "the curve starts at the clip start");
    // A clip entirely after the tempo change is one stretch again (at tempo 90).
    rig.clips.edit_clip(clip.id, ClipEdit { start_tick: Some(960), ..Default::default() }).unwrap();
    let moved = job(&rig);
    assert!(moved.curve().is_none() && (moved.stretch() - 60.0 / 90.0).abs() < 1e-6);
}
