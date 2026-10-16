//! Normalize and reverse of audio clips (`ClipProcessing`): the clip gain or sample changes, the audio is measured and
//! written by the sample editor.

use std::sync::Arc;

use super::*;

/// A saved project with an audio track and a one-second clip at 0 (1920 ticks at 120 BPM) over a 48 000-frame sound
/// whose loudest frame (`peak`) is at `peak_at`.
fn rig_with_audio(peak: f32, peak_at: usize) -> (Rig, TrackId, Clip) {
    let rig = Rig::new();
    rig.files.create(Path::new("/m/P/P.dawproj")).unwrap();
    rig.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let mut frames = vec![0.1; 48_000];
    frames[peak_at] = -peak;
    rig.samples.insert(sample.id, Arc::new(SampleData { sample_rate: 48_000, channels: 1, frames }));
    let track = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, sample.id).unwrap();
    (rig, track, clip)
}

#[test]
fn the_sample_editor_measures_the_peak_of_a_range() {
    let (rig, _, clip) = rig_with_audio(0.8, 30_000);
    let sample = clip.content.sample().unwrap();
    assert_eq!(rig.editor.peak(sample, 0, 48_000).unwrap(), 0.8);
    assert_eq!(rig.editor.peak(sample, 0, 30_000).unwrap(), 0.1, "the end is exclusive");
    assert_eq!(rig.editor.peak(sample, 40_000, 999_999).unwrap(), 0.1, "the end is clamped");
    assert_eq!(rig.editor.peak(sample, 50_000, 60_000).unwrap(), 0.0, "an empty range is silent");
    assert!(matches!(rig.editor.peak(999, 0, 10), Err(e) if e.is(codes::samples::NOT_FOUND)));
}

#[test]
fn normalize_sets_the_gain_so_the_played_peak_hits_the_target() {
    let (rig, _, clip) = rig_with_audio(0.5, 30_000);
    let normalized = rig.processing.normalize(vec![clip.id], -6.0).unwrap();
    let expected = 10f32.powf(-6.0 / 20.0) / 0.5;
    assert!((normalized[0].gain - expected).abs() < 1e-4);
    assert_eq!(rig.service.undo_label().as_deref(), Some("Normalize"));

    // Only the played range counts: after trimming the loud frame away the quiet part sets the gain (capped).
    rig.clips.edit_clip(clip.id, ClipEdit { length_ticks: Some(960), ..Default::default() }).unwrap();
    let quiet = rig.processing.normalize(vec![clip.id], DEFAULT_NORMALIZE_PEAK_DB).unwrap();
    assert_eq!(quiet[0].gain, MAX_CLIP_GAIN, "0.1 would need ×9.7, the gain stops at the maximum");
}

#[test]
fn normalize_validates_the_level_and_the_clips_and_changes_nothing_on_error() {
    let (rig, _, clip) = rig_with_audio(0.5, 100);
    let level = rig.processing.normalize(vec![clip.id], 1.0);
    assert!(matches!(level, Err(e) if e.is(codes::clips::NORMALIZE_LEVEL_OUT_OF_RANGE)));
    assert!(matches!(rig.processing.normalize(vec![clip.id], f32::NAN), Err(e) if e.is(codes::clips::NORMALIZE_LEVEL_NOT_A_NUMBER)));
    assert!(matches!(rig.processing.normalize(vec![clip.id, 999], -1.0), Err(e) if e.is(codes::clips::NOT_FOUND)));
    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let notes = rig.clips.add_note_clip(NewNoteClip::empty(synth, 0, None)).unwrap();
    assert!(matches!(rig.processing.normalize(vec![notes.id], -1.0), Err(e) if e.is(codes::clips::NEEDS_AUDIO_CLIP)));
    assert_eq!(rig.clips.clips().iter().find(|c| c.id == clip.id).unwrap().gain, 1.0, "nothing changed");
    assert_eq!(rig.processing.normalize(Vec::new(), -1.0).unwrap(), []);
}

#[test]
fn reverse_plays_the_same_piece_backwards_from_a_new_sample_in_one_undo_step() {
    let (rig, _, clip) = rig_with_audio(0.5, 100);
    // The clip plays frames 12 000..36 000 of the sound (half a second from a quarter second in).
    let trimmed = rig.clips.trim_start(clip.id, 480).unwrap();
    let trimmed = rig.clips.edit_clip(trimmed.id, ClipEdit { length_ticks: Some(960), ..Default::default() }).unwrap();
    let samples_before = rig.catalog.samples().len();

    let reversed = rig.processing.reverse(clip.id).unwrap();
    let new_sample = reversed.content.sample().unwrap();
    assert_ne!(Some(new_sample), clip.content.sample());
    assert_eq!(rig.catalog.samples().len(), samples_before + 1);
    assert_eq!(reversed.offset_frames, 48_000 - 36_000, "the mirrored offset");
    assert_eq!((reversed.start_tick, reversed.length_ticks, reversed.gain), (trimmed.start_tick, trimmed.length_ticks, trimmed.gain));
    let writes = rig.writer.0.lock().unwrap().clone();
    assert_eq!(writes, [(PathBuf::from("/m/P/samples/loop reversed.wav"), 48_000)]);

    assert_eq!(rig.service.undo_label().as_deref(), Some("Reverse clip"));
    assert!(rig.service.undo());
    assert_eq!(rig.clips.clips()[0], trimmed, "one undo restores the clip");
    assert_eq!(rig.catalog.samples().len(), samples_before, "and drops the reversed sample");
}

#[test]
fn reverse_needs_a_saved_project_an_audio_clip_and_a_single_take() {
    let rig = Rig::new();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let track = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, sample.id).unwrap();
    assert!(matches!(rig.processing.reverse(clip.id), Err(e) if e.is(codes::samples::REVERSE_NEEDS_SAVED_PROJECT)));
    assert_eq!(rig.clips.clips(), std::slice::from_ref(&clip), "nothing changed");

    // Creating the project file starts a new empty project: the clip is made again inside it.
    rig.files.create(Path::new("/m/P/P.dawproj")).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let track = rig.tracks.add_track("Vocals", None, 60).unwrap();
    let clip = rig.clips.add_audio_clip(track, 0, sample.id).unwrap();
    let take = rig.catalog.import(Path::new("/music/take2.wav")).unwrap();
    rig.takes.add_take(clip.id, take.id).unwrap();
    assert!(matches!(rig.processing.reverse(clip.id), Err(e) if e.is(codes::clips::REVERSE_NEEDS_ONE_TAKE)));
    let synth = rig.tracks.add_track("Synth", Some("synth"), 60).unwrap();
    let notes = rig.clips.add_note_clip(NewNoteClip::empty(synth, 0, None)).unwrap();
    assert!(matches!(rig.processing.reverse(notes.id), Err(e) if e.is(codes::clips::NEEDS_AUDIO_CLIP)));
}
