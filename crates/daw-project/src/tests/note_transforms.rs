//! The `NoteControl::transform_notes` contract: quantize, humanize, transpose, fit to scale, legato.

use super::*;

/// A clip with notes `notes`; returns their ids in order.
fn clip_with(notes: Vec<NewNote>) -> (Rig, ClipId, Vec<NoteId>) {
    let (rig, _, clip) = rig_with_clip();
    let ids = rig.notes.add_notes(clip, notes).unwrap().iter().map(|n| n.id).collect();
    (rig, clip, ids)
}

/// (pitch, start, length) of all notes of the clip.
fn shape(rig: &Rig, clip: ClipId) -> Vec<(u8, u32, u32)> {
    rig.notes.notes(clip).unwrap().iter().map(|n| (n.pitch, n.start_tick, n.length_ticks)).collect()
}

fn quantize(grid: u32, strength: f64, ends: bool) -> NoteTransform {
    NoteTransform::Quantize { grid, strength, ends }
}

#[test]
fn quantize_moves_starts_to_the_grid_and_is_one_undo_step() {
    let (rig, clip, _) = clip_with(vec![note(60, 100, 200), note(62, 130, 50), note(64, 500, 300)]);
    let changed = rig.notes.transform_notes(clip, None, quantize(240, 1.0, false)).unwrap();
    assert_eq!(changed.len(), 3);
    // 100 → 0, 130 → 240 (closer to 240), 500 → 480; the length stays.
    assert_eq!(shape(&rig, clip), [(60, 0, 200), (62, 240, 50), (64, 480, 300)]);
    assert!(rig.service.undo());
    assert_eq!(shape(&rig, clip), [(60, 100, 200), (62, 130, 50), (64, 500, 300)], "one undo step");
}

#[test]
fn quantize_strength_moves_part_of_the_way_and_ends_snap_too() {
    let (rig, clip, _) = clip_with(vec![note(60, 100, 200)]);
    rig.notes.transform_notes(clip, None, quantize(240, 0.5, false)).unwrap();
    assert_eq!(shape(&rig, clip), [(60, 50, 200)], "half way from 100 to 0");

    let (rig, clip, _) = clip_with(vec![note(60, 100, 200), note(62, 250, 10)]);
    rig.notes.transform_notes(clip, None, quantize(240, 1.0, true)).unwrap();
    // End 300 → 240; a short note's end does not collapse onto its start but goes to the next line.
    assert_eq!(shape(&rig, clip), [(60, 0, 240), (62, 240, 240)]);
}

#[test]
fn quantize_keeps_notes_inside_a_manual_length() {
    let (rig, clip, _) = clip_with(vec![note(60, 3800, 30)]);
    rig.notes.set_loop_ticks(clip, BAR_TICKS).unwrap();
    rig.notes.transform_notes(clip, None, quantize(240, 1.0, false)).unwrap();
    assert_eq!(shape(&rig, clip), [(60, BAR_TICKS - 30, 30)], "line 3840 is the content end: the note is moved back");
}

#[test]
fn transforms_check_targets_and_parameters_without_changing_anything() {
    let (rig, clip, ids) = clip_with(vec![note(60, 100, 200), note(62, 130, 50)]);
    let before = shape(&rig, clip);
    let label = rig.service.undo_label();
    assert!(matches!(rig.notes.transform_notes(clip, Some(vec![999]), quantize(240, 1.0, false)), Err(e) if e.is(codes::notes::NOT_FOUND)));
    let none = rig.notes.transform_notes(clip, Some(vec![]), quantize(240, 1.0, false));
    assert!(matches!(none, Err(e) if e.is(codes::notes::NO_NOTES_SELECTED)));
    let fails = |transform, code| matches!(rig.notes.transform_notes(clip, None, transform), Err(e) if e.is(code));
    assert!(fails(quantize(0, 1.0, false), codes::notes::QUANTIZE_GRID_OUT_OF_RANGE), "grid 0");
    assert!(fails(quantize(240, 1.5, false), codes::notes::QUANTIZE_STRENGTH_OUT_OF_RANGE), "strength above 1");
    assert!(fails(quantize(240, f64::NAN, false), codes::notes::QUANTIZE_STRENGTH_OUT_OF_RANGE), "strength NaN");
    let humanize = |timing_ticks, velocity| NoteTransform::Humanize { timing_ticks, velocity, seed: Some(1) };
    assert!(fails(humanize(MAX_HUMANIZE_TICKS + 1, 0), codes::notes::HUMANIZE_TIMING_OUT_OF_RANGE));
    assert!(fails(humanize(0, 127), codes::notes::HUMANIZE_VELOCITY_OUT_OF_RANGE));
    assert!(fails(NoteTransform::Transpose { semitones: 128 }, codes::notes::TRANSPOSE_OUT_OF_RANGE));
    assert!(fails(NoteTransform::Transpose { semitones: i32::MIN }, codes::notes::TRANSPOSE_OUT_OF_RANGE), "no overflow");
    assert!(fails(NoteTransform::FitToScale { root: 12, scale: Scale::Major }, codes::notes::SCALE_ROOT_OUT_OF_RANGE));
    let missing = rig.notes.transform_notes(9999, None, NoteTransform::Legato);
    assert!(matches!(missing, Err(e) if e.is(codes::clips::NOT_FOUND)), "no clip");
    assert_eq!(shape(&rig, clip), before);
    assert_eq!(rig.service.undo_label(), label, "errors record no undo steps");

    // Only the selected notes.
    rig.notes.transform_notes(clip, Some(vec![ids[1]]), quantize(240, 1.0, false)).unwrap();
    assert_eq!(shape(&rig, clip), [(60, 100, 200), (62, 240, 50)]);
}

#[test]
fn transforms_refuse_audio_clips_and_do_nothing_on_an_empty_clip() {
    let (rig, _, clip) = rig_with_clip();
    let label = rig.service.undo_label();
    assert_eq!(rig.notes.transform_notes(clip, None, NoteTransform::Legato).unwrap(), []);
    assert_eq!(rig.service.undo_label(), label, "nothing to change, so no undo step");

    let track = rig.tracks.add_track("Audio", None, 60).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let audio = rig.clips.add_audio_clip(track, 0, sample.id).unwrap().id;
    let legato = rig.notes.transform_notes(audio, None, NoteTransform::Legato);
    assert!(matches!(legato, Err(e) if e.is(codes::notes::AUDIO_CLIP_HAS_NO_NOTES)));
}

#[test]
fn humanize_is_deterministic_with_a_seed_and_stays_in_bounds() {
    let notes = vec![
        note(60, 0, 240),
        note(62, 480, 240),
        NewNote { velocity: 127, ..note(64, 960, 240) },
        NewNote { velocity: 1, ..note(65, 1440, 240) },
    ];
    let run = |seed| {
        let (rig, clip, _) = clip_with(notes.clone());
        rig.notes.transform_notes(clip, None, NoteTransform::Humanize { timing_ticks: 30, velocity: 20, seed: Some(seed) }).unwrap()
    };
    let first = run(7);
    assert_eq!(first, run(7), "one seed, one result");
    assert_ne!(first, run(8));
    for (after, before) in first.iter().zip(&notes) {
        assert!(after.start_tick.abs_diff(before.start_tick) <= 30);
        assert!(after.velocity.abs_diff(before.velocity) <= 20 && (1..=127).contains(&after.velocity));
        assert_eq!(after.length_ticks, before.length_ticks);
    }

    let (rig, clip, _) = clip_with(notes.clone());
    let label = rig.service.undo_label();
    rig.notes.transform_notes(clip, None, NoteTransform::Humanize { timing_ticks: 0, velocity: 0, seed: None }).unwrap();
    assert_eq!(rig.service.undo_label(), label, "zero spread changes nothing");
}

#[test]
fn transpose_shifts_pitch_or_fails_as_a_whole() {
    let (rig, clip, _) = clip_with(vec![note(60, 0, 240), note(120, 240, 240)]);
    rig.notes.transform_notes(clip, None, NoteTransform::Transpose { semitones: -12 }).unwrap();
    assert_eq!(shape(&rig, clip), [(48, 0, 240), (108, 240, 240)]);
    let error = rig.notes.transform_notes(clip, None, NoteTransform::Transpose { semitones: 20 }).unwrap_err();
    assert!(error.is(codes::notes::TRANSPOSED_PITCH_OUT_OF_RANGE), "{error}");
    assert_eq!(shape(&rig, clip), [(48, 0, 240), (108, 240, 240)], "all or nothing");
    assert!(rig.service.undo());
    assert_eq!(shape(&rig, clip), [(60, 0, 240), (120, 240, 240)]);
}

#[test]
fn fit_to_scale_moves_to_the_nearest_degree_and_ties_go_down() {
    let pitches = |rig: &Rig, clip| shape(rig, clip).iter().map(|n| n.0).collect::<Vec<_>>();
    let notes = |list: &[u8]| list.iter().map(|p| note(*p, 0, 240)).collect();

    let (rig, clip, _) = clip_with(notes(&[60, 61, 63, 66, 126, 127]));
    rig.notes.transform_notes(clip, None, NoteTransform::FitToScale { root: 0, scale: Scale::Major }).unwrap();
    assert_eq!(pitches(&rig, clip), [60, 60, 62, 65, 125, 127], "C# → C, D# → D, F# → F, G9 stays");

    let (rig, clip, _) = clip_with(notes(&[1, 2, 61]));
    rig.notes.transform_notes(clip, None, NoteTransform::FitToScale { root: 0, scale: Scale::PentatonicMinor }).unwrap();
    assert_eq!(pitches(&rig, clip), [0, 3, 60], "D is closer to D# than to C");

    let (rig, clip, _) = clip_with(notes(&[61]));
    rig.notes.transform_notes(clip, None, NoteTransform::FitToScale { root: 2, scale: Scale::Major }).unwrap();
    assert_eq!(pitches(&rig, clip), [61], "C# is a step of D major");
    rig.notes.transform_notes(clip, None, NoteTransform::FitToScale { root: 0, scale: Scale::Chromatic }).unwrap();
    assert_eq!(pitches(&rig, clip), [61]);
}

#[test]
fn legato_extends_each_note_to_the_next_start_of_the_group() {
    let (rig, clip, ids) = clip_with(vec![note(60, 0, 100), note(64, 0, 900), note(62, 480, 100), note(65, 960, 50), note(40, 300, 10)]);
    // Note 40 is not in the group: legato ignores its start.
    rig.notes.transform_notes(clip, Some(ids[..4].to_vec()), NoteTransform::Legato).unwrap();
    assert_eq!(shape(&rig, clip), [(60, 0, 480), (64, 0, 480), (62, 480, 480), (65, 960, 50), (40, 300, 10)]);
    assert!(rig.service.undo());
    rig.notes.transform_notes(clip, None, NoteTransform::Legato).unwrap();
    assert_eq!(shape(&rig, clip)[0], (60, 0, 300), "with all notes, up to note 40");
}

#[test]
fn humanize_numbers_match_the_frontend_mock() {
    // `app/src/domain/noteTransforms.ts: humanizeGroup` (mulberry32) gives the same numbers: the mock must match the
    // backend.
    let (rig, clip, _) = clip_with(vec![note(60, 480, 240), note(62, 960, 240)]);
    let changed = rig.notes.transform_notes(clip, None, NoteTransform::Humanize { timing_ticks: 30, velocity: 20, seed: Some(7) }).unwrap();
    let got: Vec<(u32, u8)> = changed.iter().map(|n| (n.start_tick, n.velocity)).collect();
    assert_eq!(got, [(479, 115), (962, 109)]);
}
