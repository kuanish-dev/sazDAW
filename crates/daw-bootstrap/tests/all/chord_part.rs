//! A chord part on a real session: a new string track and its clip in one undo step, the guitar's voicing and the
//! bass's roots, occupied space, refusals without traces; the preview sounds without changing the project, and so does
//! the built part.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test: failing with a clear message is the check itself"
)]

use crate::common::*;

fn request(target: ChordTarget, strum: &str, bars: &[&[&str]]) -> ChordPartRequest {
    ChordPartRequest {
        target,
        start_bar: 0,
        strum: strum.into(),
        bars: bars.iter().map(|bar| bar.iter().map(|symbol| (*symbol).to_string()).collect()).collect(),
        repeat: 1,
        replace: false,
    }
}

fn new(kind: &str) -> ChordTarget {
    ChordTarget::New { kind: kind.into() }
}

fn notes_of(controls: &Controls, clip: ClipId) -> Vec<Note> {
    controls.notes.notes(clip).unwrap()
}

#[test]
fn a_part_on_a_new_track_is_one_undo_step() {
    let session = start(false);
    let c = &session.controls;
    let report = c.chord_part.build(ChordPartRequest { repeat: 2, ..request(new(GUITAR_KIND), "six", &[&["C"], &["G"]]) }).unwrap();
    assert_eq!((report.bars, report.created_track.as_deref()), (4, Some("Acoustic guitar")));

    let tracks = c.tracks.tracks();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].instrument.as_ref().map(|spec| spec.kind.as_str()), Some(GUITAR_KIND));
    let clip = c.clips.clips().into_iter().next().unwrap();
    assert_eq!((clip.id, clip.length_ticks), (report.clip, 4 * BAR_TICKS));
    let ClipContent::Notes(content) = &clip.content else { panic!("a note clip") };
    assert_eq!(content.loop_ticks, 2 * BAR_TICKS, "the progression loops inside the clip");

    assert_eq!(c.history.undo_label().as_deref(), Some("Chord part"));
    assert!(c.history.undo());
    assert!(c.tracks.tracks().is_empty() && c.clips.clips().is_empty(), "one undo takes back the track and the clip");
}

#[test]
fn the_guitar_strums_open_chords_and_the_bass_plays_roots() {
    let session = start(false);
    let c = &session.controls;
    c.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let guitar = c.chord_part.build(request(new(GUITAR_KIND), "six", &[&["C"]])).unwrap();
    let mut first: Vec<(u32, u8)> =
        notes_of(c, guitar.clip).iter().filter(|n| n.start_tick < STRUM_STEP_TICKS).map(|n| (n.start_tick, n.pitch)).collect();
    first.sort_unstable();
    assert_eq!(first.iter().map(|(_, pitch)| *pitch).collect::<Vec<_>>(), [48, 52, 55, 60, 64], "x32010 from the A string up");
    assert!(first.windows(2).all(|pair| pair[1].0 > pair[0].0), "the strings are hit one after another");

    let bass = c.chord_part.build(request(new(BASS_GUITAR_KIND), "six", &[&["A"]])).unwrap();
    let mut roots: Vec<_> = notes_of(c, bass.clip).iter().map(|n| (n.start_tick, n.pitch)).collect();
    roots.sort_unstable();
    assert_eq!(roots, [(0, 33), (960, 33), (2880, 33)], "A1 on the down strokes");
}

#[test]
fn occupied_space_is_refused_or_replaced() {
    let session = start(false);
    let c = &session.controls;
    let track = add_synth_track(c, "Keys");
    note_clip(c, track, 1, 1);
    let on_keys = || request(ChordTarget::Track { id: track }, "four", &[&["C"], &["F"]]);

    let error = c.chord_part.build(on_keys()).unwrap_err();
    assert!(error.is(codes::patterns::PLACE_TAKEN), "{error}");
    assert_eq!(c.clips.clips().len(), 1, "nothing changed");

    let report = c.chord_part.build(ChordPartRequest { replace: true, ..on_keys() }).unwrap();
    let clips = c.clips.clips();
    assert_eq!(clips.iter().map(|clip| clip.id).collect::<Vec<_>>(), [report.clip], "the old clip is replaced");
    assert_eq!((report.track, report.created_track), (track, None));
}

#[test]
fn bad_requests_are_refused_without_traces() {
    let session = start(false);
    let c = &session.controls;
    let audio = c.tracks.add_track("Vox", None, 60).unwrap();
    let cases = [
        (request(new(GUITAR_KIND), "seven", &[&["C"]]), codes::patterns::STRUM_NOT_FOUND),
        (request(new(GUITAR_KIND), "six", &[]), codes::patterns::NO_CHORDS),
        (request(new(GUITAR_KIND), "six", &[&["C", "F", "G"]]), codes::patterns::BAR_CHORD_COUNT),
        (request(new(GUITAR_KIND), "six", &[&["Cmin"]]), codes::patterns::BAD_CHORD),
        (request(new("theremin"), "six", &[&["C"]]), codes::tracks::UNKNOWN_INSTRUMENT),
        (request(ChordTarget::Track { id: audio }, "six", &[&["C"]]), codes::patterns::CHORDS_ON_AUDIO_TRACK),
        (request(ChordTarget::Track { id: 999 }, "six", &[&["C"]]), codes::tracks::NOT_FOUND),
        (ChordPartRequest { repeat: 0, ..request(new(GUITAR_KIND), "six", &[&["C"]]) }, codes::patterns::REPEAT_OUT_OF_RANGE),
    ];
    for (request, code) in cases {
        let error = c.chord_part.build(request).unwrap_err();
        assert!(error.is(code), "expected {}: {error}", code.code);
    }
    assert_eq!(c.tracks.tracks().len(), 1, "only the audio track");
    assert!(c.clips.clips().is_empty());
}

#[test]
fn the_preview_sounds_without_changing_the_project_and_the_built_part_plays() {
    let mut session = start(false);
    let c = session.controls.clone();
    c.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    c.history.mark_baseline();
    c.chord_preview.play(request(new(UKULELE_KIND), "eight", &[&["C"], &["Am"]])).unwrap();

    let bar = 96_000;
    let (first, second) = (peak(&render(&mut session, bar)), peak(&render(&mut session, bar)));
    assert!(first > 0.05 && second > 0.05, "both bars sound: {first}, {second}");
    assert!(c.tracks.tracks().is_empty() && !c.history.can_undo(), "the preview changes nothing");
    c.chord_preview.stop();
    render(&mut session, 4_800);
    assert!(peak(&render(&mut session, bar)) < 0.01, "silence after stop");

    let error = c.chord_preview.play(ChordPartRequest { repeat: 9, ..request(new(GUITAR_KIND), "six", &[&["C"], &["G"]]) }).unwrap_err();
    assert!(error.is(codes::patterns::PREVIEW_TOO_LONG), "18 bars: {error}");

    c.chord_part.build(request(new(GUITAR_KIND), "six", &[&["G"]])).unwrap();
    c.transport.play();
    assert!(peak(&render(&mut session, bar)) > 0.05, "the built part plays");
}
