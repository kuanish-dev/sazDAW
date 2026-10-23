//! The pattern variation contract: `NoteTransform::Vary` and `NoteControl::vary_to_new_clip`.

use super::*;

/// A one-bar pattern: eight eighth/quarter notes of four pitches at different velocities (as in the mock,
/// `app/src/pianoroll/variation.ts`).
fn pattern() -> Vec<NewNote> {
    (0..8_u32)
        .map(|i| NewNote {
            pitch: [48, 51, 55, 58][i as usize % 4],
            start_tick: i * 480,
            length_ticks: 240 + (i % 2) * 240,
            velocity: 90 + i as u8,
        })
        .collect()
}

fn clip_with(notes: Vec<NewNote>) -> (Rig, ClipId) {
    let (rig, _, clip) = rig_with_clip();
    rig.notes.add_notes(clip, notes).unwrap();
    (rig, clip)
}

fn vary(seed: u32, density: f64, pitch: f64, rhythm: f64, velocity: f64) -> Variation {
    Variation { seed: Some(seed), density, pitch, rhythm, velocity, scale: None }
}

/// (pitch, start, length, velocity).
fn shape(notes: &[Note]) -> Vec<(u8, u32, u32, u8)> {
    notes.iter().map(|n| (n.pitch, n.start_tick, n.length_ticks, n.velocity)).collect()
}

fn no_same_pitch_overlaps(notes: &[Note]) -> bool {
    notes
        .iter()
        .enumerate()
        .all(|(i, a)| notes[i + 1..].iter().all(|b| a.pitch != b.pitch || a.end_tick() <= b.start_tick || b.end_tick() <= a.start_tick))
}

#[test]
fn same_seed_gives_the_same_notes_and_another_seed_differs() {
    let run = |seed| {
        let (rig, clip) = clip_with(pattern());
        rig.notes.transform_notes(clip, None, NoteTransform::Vary(vary(seed, 0.5, 0.6, 0.6, 0.5))).unwrap();
        shape(&rig.notes.notes(clip).unwrap())
    };
    assert_eq!(run(3), run(3), "one seed, one result");
    assert_ne!(run(3), run(4));
}

#[test]
fn downbeats_stay_and_same_pitches_never_overlap() {
    for seed in 0..40 {
        let (rig, clip) = clip_with(pattern());
        rig.notes.transform_notes(clip, None, NoteTransform::Vary(vary(seed, 0.8, 1.0, 1.0, 0.0))).unwrap();
        let notes = rig.notes.notes(clip).unwrap();
        assert!(notes.iter().any(|n| (n.pitch, n.start_tick, n.length_ticks) == (48, 0, 240)), "the bar anchor is in place (seed {seed})");
        assert!(no_same_pitch_overlaps(&notes), "seed {seed}: {notes:?}");
        assert!(notes.iter().all(|n| n.end_tick() <= BAR_TICKS), "inside the content");
        let pitches = [48, 51, 55, 58];
        assert!(notes.iter().all(|n| pitches.contains(&n.pitch)), "without a scale, only the pattern's pitches");
    }
}

#[test]
fn pitch_moves_stay_in_the_scale() {
    let scale = Some(ScaleKey { root: 2, scale: Scale::Dorian });
    for seed in 0..20 {
        let (rig, clip) = clip_with(pattern());
        let variation = Variation { scale, ..vary(seed, 0.0, 1.0, 0.0, 0.0) };
        let changed = rig.notes.transform_notes(clip, None, NoteTransform::Vary(variation)).unwrap();
        for (after, before) in changed.iter().zip(pattern()) {
            if after.pitch != before.pitch {
                assert!(Scale::Dorian.contains(2, after.pitch), "{} is outside D dorian", after.pitch);
                assert!(after.pitch.abs_diff(before.pitch) <= 2, "a neighbouring step");
            }
        }
    }
}

#[test]
fn density_removes_or_adds_within_bounds() {
    let count = |density, seed| {
        let (rig, clip) = clip_with(pattern());
        rig.notes.transform_notes(clip, None, NoteTransform::Vary(vary(seed, density, 0.0, 0.0, 0.0))).unwrap();
        rig.notes.notes(clip).unwrap()
    };
    for seed in 0..20 {
        let thinner = count(-0.5, seed);
        assert!(thinner.len() <= 8 && thinner.iter().any(|n| n.start_tick == 0), "at -0.5 the anchor stays");
        let empty_bar_keeps_half = count(-1.0, seed);
        assert!(empty_bar_keeps_half.iter().all(|n| n.start_tick == 0), "at -1 at most the anchor stays");
        let denser = count(1.0, seed);
        assert!(denser.len() > 8 && denser.len() <= 16, "up to eight notes added: {}", denser.len());
        let pitches = [48, 51, 55, 58];
        assert!(denser.iter().all(|n| pitches.contains(&n.pitch) && n.start_tick.is_multiple_of(STEP_TICKS)));
    }
    assert_eq!(count(1.0, 1).len(), count(1.0, 1).len());
}

#[test]
fn vary_checks_parameters_and_zero_changes_nothing() {
    let (rig, clip) = clip_with(pattern());
    let label = rig.service.undo_label();
    for bad in [vary(1, -1.5, 0.0, 0.0, 0.0), vary(1, 0.0, 1.5, 0.0, 0.0), vary(1, 0.0, 0.0, -0.1, 0.0), vary(1, 0.0, 0.0, 0.0, f64::NAN)] {
        assert!(matches!(rig.notes.transform_notes(clip, None, NoteTransform::Vary(bad)), Err(e) if e.kind() == ErrorKind::Invalid));
    }
    let bad_root = Variation { scale: Some(ScaleKey { root: 12, scale: Scale::Major }), ..vary(1, 0.0, 1.0, 0.0, 0.0) };
    let root = rig.notes.transform_notes(clip, None, NoteTransform::Vary(bad_root));
    assert!(matches!(root, Err(e) if e.is(codes::notes::SCALE_ROOT_OUT_OF_RANGE)));
    rig.notes.transform_notes(clip, None, NoteTransform::Vary(Variation::default())).unwrap();
    assert_eq!(rig.service.undo_label(), label, "errors and a zero variation record no undo steps");
}

#[test]
fn vary_selected_notes_only_and_undo_restores_everything() {
    let (rig, clip) = clip_with(pattern());
    let before = shape(&rig.notes.notes(clip).unwrap());
    let ids: Vec<NoteId> = rig.notes.notes(clip).unwrap().iter().skip(4).map(|n| n.id).collect();
    rig.notes.transform_notes(clip, Some(ids), NoteTransform::Vary(vary(9, 0.5, 1.0, 1.0, 1.0))).unwrap();
    let after = shape(&rig.notes.notes(clip).unwrap());
    assert_eq!(after[..4], before[..4], "notes outside the group do not change");
    assert!(rig.service.undo());
    assert_eq!(shape(&rig.notes.notes(clip).unwrap()), before, "one undo step");
}

#[test]
fn vary_to_new_clip_puts_a_varied_copy_right_after_in_one_undo_step() {
    let (rig, clip) = clip_with(pattern());
    let original = rig.clips.clips().into_iter().find(|c| c.id == clip).unwrap();
    let clips_before = rig.clips.clips().len();
    let notes_before = shape(&rig.notes.notes(clip).unwrap());
    let copy = rig.notes.vary_to_new_clip(clip, None, vary(5, 0.5, 0.5, 0.5, 0.5)).unwrap();
    assert_eq!(u64::from(copy.start_tick), original.end_tick());
    assert_eq!(copy.track, original.track);
    assert_eq!(shape(&rig.notes.notes(clip).unwrap()), notes_before, "the source clip does not change");
    assert_ne!(shape(&rig.notes.notes(copy.id).unwrap()), notes_before, "the copy is different");
    assert_eq!(rig.service.undo_label().as_deref(), Some("Variation to new clip"));
    assert!(rig.service.undo());
    assert_eq!(rig.clips.clips().len(), clips_before, "the copy and the variation are one step");

    let dense = rig.notes.vary_to_new_clip(clip, None, vary(5, 2.0, 0.0, 0.0, 0.0));
    assert!(matches!(dense, Err(e) if e.is(codes::notes::VARIATION_DENSITY_OUT_OF_RANGE)));
    assert_eq!(rig.clips.clips().len(), clips_before, "an error leaves no copy");
    assert!(matches!(rig.notes.vary_to_new_clip(9999, None, Variation::default()), Err(e) if e.is(codes::clips::NOT_FOUND)));
}

#[test]
fn variation_numbers_match_the_frontend_mock() {
    let (rig, clip) = clip_with(pattern());
    let changed = rig.notes.transform_notes(clip, None, NoteTransform::Vary(vary(42, 0.5, 0.6, 0.6, 0.5))).unwrap();
    let kept = [(48, 0, 240, 91), (51, 240, 480, 103), (58, 960, 240, 96), (55, 1440, 240, 78), (51, 1920, 240, 79), (51, 2400, 480, 93)];
    let kept_tail = [(51, 2880, 240, 101), (55, 3360, 480, 88)];
    let added = [(55, 1680, 240, 93), (55, 720, 240, 92), (51, 3120, 480, 95), (48, 3600, 240, 90)];
    let expected: Vec<_> = kept.into_iter().chain(kept_tail).chain(added).collect();
    assert_eq!(shape(&changed), expected);

    let (rig, clip) = clip_with(pattern());
    let minor = Variation { scale: Some(ScaleKey { root: 0, scale: Scale::Minor }), ..vary(7, -0.4, 0.5, 0.5, 0.0) };
    let changed = rig.notes.transform_notes(clip, None, NoteTransform::Vary(minor)).unwrap();
    let ids: Vec<NoteId> = changed.iter().map(|n| n.id).collect();
    assert_eq!(ids, [1, 2, 3, 6, 7], "notes 4, 5 and 8 are removed");
    assert_eq!(shape(&changed), [(48, 0, 240, 90), (50, 480, 480, 91), (55, 1200, 240, 92), (51, 2400, 480, 95), (56, 3120, 240, 96)]);
}
