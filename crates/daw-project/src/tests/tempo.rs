//! The `TempoControl` contract: the tempo map is part of the project; edits are undoable and reach the engine in the
//! snapshot.

use super::*;

fn bpm(value: f64) -> Bpm {
    Bpm::new(value).unwrap()
}

fn tempo(tick: u32, value: f64, ramp: bool) -> TempoChange {
    TempoChange { tick, bpm: bpm(value), ramp }
}

fn signature(tick: u32, numerator: u8, denominator: u8) -> SignatureChange {
    SignatureChange { tick, signature: TimeSignature::new(numerator, denominator).unwrap() }
}

#[test]
fn tempo_changes_are_undoable_edits_that_reach_the_engine() {
    let rig = Rig::new();
    let changes = vec![tempo(0, 120.0, false), tempo(3840, 140.0, true)];
    rig.tempo.set_tempo_changes(changes.clone()).unwrap();
    assert_eq!(rig.tempo.tempo_map().tempos(), changes.as_slice());
    assert_eq!(rig.last_snapshot().project.tempo_map.tempos(), changes.as_slice(), "the engine got the map in the snapshot");
    assert!(rig.files.is_dirty());

    assert!(rig.service.undo());
    assert_eq!(rig.tempo.tempo_map(), TempoMap::default());
    assert!(rig.service.redo());
    assert_eq!(rig.tempo.tempo_map().tempos().len(), 2);
}

#[test]
fn base_tempo_and_signature_keep_the_other_points() {
    let rig = Rig::new();
    rig.tempo.set_tempo_changes(vec![tempo(0, 120.0, false), tempo(7680, 90.0, false)]).unwrap();
    rig.tempo.set_tempo(bpm(100.0)).unwrap();
    assert_eq!(rig.tempo.tempo_map().tempos(), [tempo(0, 100.0, false), tempo(7680, 90.0, false)]);

    rig.tempo.set_signature_changes(vec![signature(0, 4, 4), signature(7680, 7, 8)]).unwrap();
    // 3/4 does not divide 7680 into whole bars (2880): the change would land mid-bar.
    let off_bar = rig.tempo.set_time_signature(TimeSignature::new(3, 4).unwrap());
    assert!(matches!(off_bar, Err(e) if e.is(codes::tempo::SIGNATURE_OFF_BAR)));
    rig.tempo.set_time_signature(TimeSignature::new(2, 4).unwrap()).unwrap();
    assert_eq!(rig.tempo.tempo_map().signatures(), [signature(0, 2, 4), signature(7680, 7, 8)]);
}

#[test]
fn invalid_maps_leave_the_project_unchanged() {
    let rig = Rig::new();
    let before = rig.service.revision();
    let late = rig.tempo.set_tempo_changes(vec![tempo(960, 120.0, false)]);
    assert!(matches!(late, Err(e) if e.is(codes::tempo::TEMPO_MAP_FIRST_NOT_AT_ZERO)));
    assert!(rig.tempo.set_signature_changes(vec![signature(0, 4, 4), signature(100, 3, 4)]).is_err());
    assert_eq!(rig.service.revision(), before);
    assert!(!rig.service.can_undo());
}

#[test]
fn audio_clip_length_follows_the_map_from_its_start() {
    let rig = Rig::new();
    // One second of sound: up to bar 2 at 120 BPM (1920 ticks per second), then 60 BPM (960).
    rig.tempo.set_tempo_changes(vec![tempo(0, 120.0, false), tempo(3840, 60.0, false)]).unwrap();
    let sample = rig.catalog.import(Path::new("/music/hit.wav")).unwrap();
    let track = rig.tracks.add_track("Sound", None, 60).unwrap();
    assert_eq!(rig.clips.add_audio_clip(track, 0, sample.id).unwrap().length_ticks, 1920);
    assert_eq!(rig.clips.add_audio_clip(track, 3840, sample.id).unwrap().length_ticks, 960);
    // Half a second at 120 and half a second at 60.
    let other = rig.tracks.add_track("Sound 2", None, 60).unwrap();
    assert_eq!(rig.clips.add_audio_clip(other, 2880, sample.id).unwrap().length_ticks, 960 + 480);
}

#[test]
fn a_note_clip_counts_bars_by_the_signature_at_its_start() {
    let rig = Rig::new();
    // Two bars of 4/4, then 7/8 (a 3360-tick bar).
    rig.tempo.set_signature_changes(vec![signature(0, 4, 4), signature(7680, 7, 8)]).unwrap();
    let track = rig.tracks.add_track("Notes", Some("synth"), 60).unwrap();
    let clip = rig.clips.add_note_clip(NewNoteClip::empty(track, 7680, None)).unwrap();
    assert_eq!(clip.length_ticks, 3360, "\"one bar clip\" is a 7/8 bar");
    let loop_ticks = |id| match rig.clips.clips().into_iter().find(|c| c.id == id).unwrap().content {
        ClipContent::Notes(content) => content.loop_ticks,
        ClipContent::Audio { .. } => unreachable!(),
    };
    assert_eq!(loop_ticks(clip.id), 3360);
    rig.notes.add_note(clip.id, super::note(60, 3360, 240)).unwrap();
    assert_eq!(loop_ticks(clip.id), 2 * 3360, "the automatic length grows in 7/8 bars");
    // A clip in 4/4 counts its bars as 3840.
    let early = rig.clips.add_note_clip(NewNoteClip::empty(track, 0, None)).unwrap();
    assert_eq!(early.length_ticks, 3840);
}

#[test]
fn the_map_is_saved_and_loaded_with_the_project() {
    let rig = Rig::new();
    rig.tempo.set_signature_changes(vec![signature(0, 3, 4), signature(5760, 4, 4)]).unwrap();
    rig.files.save(Path::new("/p/map.daw")).unwrap();
    rig.files.new_project();
    assert_eq!(rig.tempo.tempo_map(), TempoMap::default());
    rig.files.load(Path::new("/p/map.daw")).unwrap();
    assert_eq!(rig.tempo.tempo_map().signatures(), [signature(0, 3, 4), signature(5760, 4, 4)]);
}
