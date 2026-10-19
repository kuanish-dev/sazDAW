//! The `KeyControl` contract: the song's key, and step lines whose scale degrees count in it.

use super::*;

const F_MINOR: ScaleKey = ScaleKey { root: 5, scale: Scale::Minor };

fn step_clip(track: TrackId, steps: &str) -> NewStepClip {
    NewStepClip { track, start_tick: 0, length_ticks: None, steps: steps.into(), step_ticks: STEP_TICKS, gate: 0.5, octave: 2 }
}

#[test]
fn the_key_is_project_data_with_undo() {
    let rig = Rig::new();
    assert_eq!(rig.key.key(), None, "a new project has no key");
    rig.key.set_key(Some(F_MINOR)).unwrap();
    assert_eq!(rig.key.key(), Some(F_MINOR));
    assert_eq!(rig.service.snapshot().key, Some(F_MINOR));
    let label = rig.service.undo_label();
    rig.key.set_key(Some(F_MINOR)).unwrap();
    assert_eq!(rig.service.undo_label(), label, "the same key is no undo step");
    rig.key.set_key(None).unwrap();
    assert_eq!(rig.key.key(), None);
    assert!(rig.service.undo());
    assert_eq!(rig.key.key(), Some(F_MINOR));
}

#[test]
fn a_tonic_above_b_is_refused() {
    let rig = Rig::new();
    let error = rig.key.set_key(Some(ScaleKey { root: 12, scale: Scale::Major })).unwrap_err();
    assert!(error.is(codes::notes::SCALE_ROOT_OUT_OF_RANGE));
    assert_eq!(rig.key.key(), None);
}

#[test]
fn a_step_line_becomes_one_looping_clip_in_one_undo_step() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Acid", Some("synth"), 60).unwrap();
    let before = rig.service.undo_label();
    let clip = rig.clips.add_step_clip(step_clip(track, "F2 F3! - Eb3~ | F3 _ C3 -")).unwrap();
    assert_eq!(clip.length_ticks, 8 * STEP_TICKS, "without a length the clip is one pass of the line");
    let content = content(&rig, clip.id);
    assert_eq!((content.loop_ticks, content.auto_length), (8 * STEP_TICKS, false));
    let pitches: Vec<u8> = content.notes.iter().map(|n| n.pitch).collect();
    assert_eq!(pitches, [41, 53, 51, 53, 48]);
    assert!(rig.service.undo());
    assert_eq!(rig.service.undo_label(), before, "the clip and its notes were one step");
    assert!(rig.clips.clips().is_empty());
}

#[test]
fn scale_degrees_need_the_key_and_follow_it() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Bass", Some("synth"), 60).unwrap();
    let error = rig.clips.add_step_clip(step_clip(track, "1 5 8 -")).unwrap_err();
    assert!(error.is(codes::patterns::DEGREE_WITHOUT_KEY));
    assert!(rig.clips.clips().is_empty(), "a refused line creates nothing");

    rig.key.set_key(Some(F_MINOR)).unwrap();
    let longer = NewStepClip { length_ticks: Some(BAR_TICKS * 2), ..step_clip(track, "1 3 5 8") };
    let clip = rig.clips.add_step_clip(longer).unwrap();
    assert_eq!(clip.length_ticks, BAR_TICKS * 2, "a longer clip repeats the line");
    let pitches: Vec<u8> = content(&rig, clip.id).notes.iter().map(|n| n.pitch).collect();
    assert_eq!(pitches, [41, 44, 48, 53], "F2 Ab2 C3 F3: degree 1 in octave 2 of F minor");
}

#[test]
fn a_line_off_the_sixteenth_grid_is_refused_like_any_content_length() {
    let rig = Rig::new();
    let track = rig.tracks.add_track("Lead", Some("synth"), 60).unwrap();
    let triplets = NewStepClip { step_ticks: 160, ..step_clip(track, "C4 E4 G4 C5") };
    assert!(rig.clips.add_step_clip(triplets).unwrap_err().is(codes::notes::LOOP_LENGTH_OUT_OF_RANGE));
    let whole = NewStepClip { step_ticks: 160, ..step_clip(track, "C4 E4 G4") };
    assert_eq!(content(&rig, rig.clips.add_step_clip(whole).unwrap().id).loop_ticks, 480);
}
