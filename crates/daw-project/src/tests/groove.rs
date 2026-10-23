//! The groove contract: `NoteTransform::Groove` (templates) and `NoteTransform::GrooveFromClip` (a groove taken from a
//! clip).

use super::*;

/// A clip with notes `notes`; returns its id.
fn clip_with(rig: &Rig, track: TrackId, at: u32, notes: Vec<NewNote>) -> ClipId {
    let clip = rig.clips.add_note_clip(NewNoteClip::empty(track, at, None)).unwrap().id;
    rig.notes.add_notes(clip, notes).unwrap();
    clip
}

/// Straight sixteenths of one beat at velocity 100.
fn sixteenths() -> Vec<NewNote> {
    (0..4).map(|i| note(60, i * STEP_TICKS, 120)).collect()
}

/// (start, velocity) of all notes of the clip.
fn timing(rig: &Rig, clip: ClipId) -> Vec<(u32, u8)> {
    rig.notes.notes(clip).unwrap().iter().map(|n| (n.start_tick, n.velocity)).collect()
}

fn groove(template: GrooveTemplate, amount: f64) -> NoteTransform {
    NoteTransform::Groove { template, amount }
}

#[test]
fn each_template_moves_off_beat_notes_by_its_ticks_and_scales_velocity() {
    let (rig, track, _) = rig_with_clip();
    let cases = [
        (GrooveTemplate::Swing { grid: SwingGrid::Eighth, percent: 58 }, [(0, 100), (278, 100), (557, 100), (758, 100)]),
        (GrooveTemplate::Swing { grid: SwingGrid::Sixteenth, percent: 66 }, [(0, 100), (317, 100), (480, 100), (797, 100)]),
        (GrooveTemplate::LaidBack { ticks: 20 }, [(0, 100), (260, 100), (500, 100), (740, 100)]),
        (GrooveTemplate::Pushed { ticks: 20 }, [(0, 100), (220, 100), (460, 100), (700, 100)]),
        (GrooveTemplate::Accent { percent: 120 }, [(0, 120), (240, 100), (480, 100), (720, 100)]),
    ];
    for (i, (template, expected)) in cases.into_iter().enumerate() {
        let clip = clip_with(&rig, track, BAR_TICKS * (i as u32 + 1), sixteenths());
        rig.notes.transform_notes(clip, None, groove(template, 1.0)).unwrap();
        assert_eq!(timing(&rig, clip), expected, "{template:?}");
        assert!(rig.notes.notes(clip).unwrap().iter().all(|n| n.length_ticks == 120), "the length stays");
    }
}

#[test]
fn groove_repeats_every_beat_scales_with_amount_and_is_one_undo_step() {
    let (rig, track, _) = rig_with_clip();
    let clip = clip_with(&rig, track, BAR_TICKS, vec![note(60, 960 + 480, 100), note(62, 2 * 960 + 250, 100)]);
    let laid_back = GrooveTemplate::LaidBack { ticks: 40 };
    rig.notes.transform_notes(clip, None, groove(laid_back, 0.5)).unwrap();
    assert_eq!(timing(&rig, clip), [(1460, 100), (2190, 100)]);
    assert!(rig.service.undo());
    assert_eq!(timing(&rig, clip), [(1440, 100), (2170, 100)], "one undo step");

    let label = rig.service.undo_label();
    rig.notes.transform_notes(clip, None, groove(laid_back, 0.0)).unwrap();
    rig.notes.transform_notes(clip, None, groove(GrooveTemplate::Accent { percent: 200 }, 0.0)).unwrap();
    assert_eq!(rig.service.undo_label(), label, "amount 0 changes nothing, so no undo step");
}

#[test]
fn groove_checks_parameters_and_keeps_notes_inside_the_content() {
    let (rig, track, _) = rig_with_clip();
    let clip = clip_with(&rig, track, BAR_TICKS, vec![note(60, 240, 100)]);
    let label = rig.service.undo_label();
    let swing = |percent| groove(GrooveTemplate::Swing { grid: SwingGrid::Eighth, percent }, 1.0);
    let fails = |transform, code| matches!(rig.notes.transform_notes(clip, None, transform), Err(e) if e.is(code));
    assert!(fails(swing(49), codes::notes::SWING_OUT_OF_RANGE));
    assert!(fails(swing(76), codes::notes::SWING_OUT_OF_RANGE));
    let too_far = groove(GrooveTemplate::Pushed { ticks: MAX_GROOVE_SHIFT_TICKS + 1 }, 1.0);
    assert!(fails(too_far, codes::notes::GROOVE_SHIFT_OUT_OF_RANGE));
    assert!(fails(groove(GrooveTemplate::Accent { percent: 99 }, 1.0), codes::notes::ACCENT_OUT_OF_RANGE));
    assert!(fails(groove(GrooveTemplate::LaidBack { ticks: 10 }, 1.5), codes::notes::GROOVE_AMOUNT_OUT_OF_RANGE));
    assert!(fails(groove(GrooveTemplate::LaidBack { ticks: 10 }, f64::NAN), codes::notes::GROOVE_AMOUNT_OUT_OF_RANGE));
    assert_eq!(rig.service.undo_label(), label, "errors record no undo steps");

    let edge = clip_with(&rig, track, 3 * BAR_TICKS, vec![note(60, BAR_TICKS - 240, 240)]);
    rig.notes.set_loop_ticks(edge, BAR_TICKS).unwrap();
    rig.notes.transform_notes(edge, None, groove(GrooveTemplate::LaidBack { ticks: 30 }, 1.0)).unwrap();
    assert_eq!(timing(&rig, edge), [(BAR_TICKS - 240, 100)]);
}

#[test]
fn groove_taken_from_a_swung_clip_reproduces_the_swing_on_a_straight_clip() {
    let (rig, track, _) = rig_with_clip();
    let swung: Vec<NewNote> = (0..32)
        .map(|i| {
            let shift = if i % 2 == 1 { 38 } else { 0 };
            NewNote { velocity: if i % 4 == 0 { 120 } else { 90 }, ..note(36, i * STEP_TICKS + shift, 100) }
        })
        .collect();
    let source = clip_with(&rig, track, 8 * BAR_TICKS, swung.clone());
    let straight = clip_with(&rig, track, 4 * BAR_TICKS, (0..16).map(|i| note(60, i * STEP_TICKS, 100)).collect());
    rig.notes.transform_notes(straight, None, NoteTransform::GrooveFromClip { source, amount: 1.0 }).unwrap();
    for (after, model) in rig.notes.notes(straight).unwrap().iter().zip(&swung) {
        assert!(after.start_tick.abs_diff(model.start_tick) <= 1, "{} vs {}", after.start_tick, model.start_tick);
        assert_eq!(after.velocity, if model.velocity == 120 { 123 } else { 92 });
    }
}

#[test]
fn groove_from_clip_refuses_missing_audio_and_empty_sources() {
    let (rig, track, empty) = rig_with_clip();
    let clip = clip_with(&rig, track, BAR_TICKS, sixteenths());
    let from = |source| NoteTransform::GrooveFromClip { source, amount: 1.0 };
    assert!(matches!(rig.notes.transform_notes(clip, None, from(9999)), Err(e) if e.is(codes::clips::NOT_FOUND)));
    assert!(matches!(rig.notes.transform_notes(clip, None, from(empty)), Err(e) if e.is(codes::notes::GROOVE_SOURCE_EMPTY)));
    let audio_track = rig.tracks.add_track("Audio", None, 60).unwrap();
    let sample = rig.catalog.import(Path::new("/music/loop.wav")).unwrap();
    let audio = rig.clips.add_audio_clip(audio_track, 0, sample.id).unwrap().id;
    assert!(matches!(rig.notes.transform_notes(clip, None, from(audio)), Err(e) if e.is(codes::notes::AUDIO_CLIP_HAS_NO_NOTES)));
    assert_eq!(timing(&rig, clip), [(0, 100), (240, 100), (480, 100), (720, 100)]);
}

#[test]
fn groove_numbers_match_the_frontend_mock() {
    let (rig, track, _) = rig_with_clip();
    let notes = || {
        vec![
            note(60, 0, 240),
            NewNote { velocity: 90, ..note(62, 240, 240) },
            NewNote { velocity: 77, ..note(64, 480, 240) },
            NewNote { velocity: 101, ..note(65, 720, 240) },
        ]
    };
    let clip = clip_with(&rig, track, BAR_TICKS, notes());
    rig.notes.transform_notes(clip, None, groove(GrooveTemplate::Swing { grid: SwingGrid::Eighth, percent: 62 }, 0.5)).unwrap();
    assert_eq!(timing(&rig, clip), [(0, 100), (269, 90), (538, 77), (749, 101)]);
    let clip = clip_with(&rig, track, 2 * BAR_TICKS, notes());
    rig.notes.transform_notes(clip, None, groove(GrooveTemplate::Accent { percent: 130 }, 0.7)).unwrap();
    assert_eq!(timing(&rig, clip), [(0, 121), (240, 90), (480, 77), (720, 101)]);

    let source = [(0, 120), (270, 80), (485, 110), (755, 70)].map(|(at, velocity)| NewNote { velocity, ..note(36, at, 100) });
    let source = clip_with(&rig, track, 3 * BAR_TICKS, source.to_vec());
    let clip = clip_with(&rig, track, 4 * BAR_TICKS, sixteenths());
    rig.notes.transform_notes(clip, None, NoteTransform::GrooveFromClip { source, amount: 1.0 }).unwrap();
    assert_eq!(timing(&rig, clip), [(0, 126), (270, 84), (485, 116), (755, 74)]);
}
