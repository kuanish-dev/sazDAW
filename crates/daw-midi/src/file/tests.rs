//! MIDI file tests: parsing, writing, importing back and exporting the project.

use std::path::Path;
use std::sync::{Arc, Mutex};

use daw_contracts::{
    BAR_TICKS, Bpm, ClipContent, ClipControl, MidiFileIo, PPQ, SignatureChange, TempoChange, TempoControl, TempoMap, TimeSignature,
};
use midly::num::{u4, u7, u15, u28};
use midly::{MetaMessage, MidiMessage, Smf, TrackEvent, TrackEventKind};

use super::MidiFiles;
use super::fakes::{Fake, FakePreferences, FakeTempo};
use super::read::{RawNote, extract_notes, imported_notes, ticks_per_beat, to_project_ticks};
use super::write::{melodic_channel, unrolled};

fn event(delta: u32, message: MidiMessage) -> TrackEvent<'static> {
    TrackEvent { delta: u28::new(delta), kind: TrackEventKind::Midi { channel: u4::new(0), message } }
}

fn on(key: u8, vel: u8) -> MidiMessage {
    MidiMessage::NoteOn { key: u7::new(key), vel: u7::new(vel) }
}

fn off(key: u8) -> MidiMessage {
    MidiMessage::NoteOff { key: u7::new(key), vel: u7::new(0) }
}

#[test]
fn pairs_note_on_and_off_including_velocity_zero() {
    let track = [event(0, on(60, 100)), event(100, on(64, 90)), event(100, off(60)), event(50, on(64, 0))];
    let notes = extract_notes(&track);
    assert_eq!(
        notes,
        [RawNote { pitch: 60, start: 0, length: 200, velocity: 100 }, RawNote { pitch: 64, start: 100, length: 150, velocity: 90 },]
    );
}

#[test]
fn unmatched_note_off_and_hanging_note_on_are_ignored() {
    let track = [event(0, off(60)), event(10, on(62, 80))];
    assert!(extract_notes(&track).is_empty());
}

#[test]
fn smpte_timing_is_rejected() {
    use midly::{Fps, Timing};
    assert!(ticks_per_beat(Timing::Timecode(Fps::Fps25, 40)).is_err_and(|e| e.is(daw_contracts::codes::midi::SMPTE_UNSUPPORTED)));
    assert_eq!(ticks_per_beat(Timing::Metrical(u15::new(480))).unwrap(), 480);
}

#[test]
fn huge_file_ticks_saturate_instead_of_wrapping() {
    let per_beat = u64::from(PPQ);
    // Wrapping would turn 2³² + 10 into 10 and put the note at the clip start.
    let huge = (1u64 << 32) + 10;
    assert_eq!(to_project_ticks(huge, per_beat), u32::MAX);
    assert_eq!(to_project_ticks(u64::MAX, 1), u32::MAX);
    let raw =
        [RawNote { pitch: 60, start: huge, length: 100, velocity: 100 }, RawNote { pitch: 62, start: 0, length: huge, velocity: 100 }];
    let notes = imported_notes(&raw, per_beat, BAR_TICKS);
    assert_eq!(notes.len(), 1, "the far note is dropped");
    assert_eq!(notes[0].pitch, 62);
    assert_eq!(notes[0].length_ticks, BAR_TICKS, "the length is cut at the limit");
}

#[test]
fn clip_far_to_the_right_does_not_wrap() {
    let fake = Fake::default();
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, u32::MAX - 100, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 200, 480)]);
    let clip = fake.clips().remove(0);
    let ClipContent::Notes(content) = &clip.content else { panic!() };
    let notes = unrolled(&clip, content);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].start, u32::MAX, "saturates rather than going to the project start");
    assert_eq!(notes[0].length, 480);
}

#[test]
fn melodic_tracks_skip_the_drum_channel() {
    let channels: Vec<u8> = (0..17).map(|i| melodic_channel(i).as_int()).collect();
    assert_eq!(channels, [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 0, 1]);
}

fn files(fake: &Arc<Fake>) -> MidiFiles {
    files_by(fake, Arc::new(FakePreferences::default()))
}

fn files_by(fake: &Arc<Fake>, preferences: Arc<FakePreferences>) -> MidiFiles {
    MidiFiles::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), Arc::new(FakeTempo::default()), preferences)
}

/// The author from the Copyright event of the meta track, if any.
fn copyright(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).unwrap();
    let smf = Smf::parse(&bytes).unwrap();
    smf.tracks[0].iter().find_map(|e| match e.kind {
        TrackEventKind::Meta(MetaMessage::Copyright(text)) => Some(String::from_utf8_lossy(text).into_owned()),
        _ => None,
    })
}

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("daw-midi-{}-{name}.mid", std::process::id()))
}

type ReadTrack = (Option<String>, Vec<(u8, u32, u32)>);

/// The file tracks: (name, notes (pitch, start, length)).
fn read_back(path: &Path) -> Vec<ReadTrack> {
    let bytes = std::fs::read(path).unwrap();
    let smf = Smf::parse(&bytes).unwrap();
    smf.tracks
        .iter()
        .map(|track| {
            let name = track.iter().find_map(|e| match e.kind {
                TrackEventKind::Meta(MetaMessage::TrackName(n)) => Some(String::from_utf8_lossy(n).into_owned()),
                _ => None,
            });
            let mut notes: Vec<_> = extract_notes(track).iter().map(|n| (n.pitch, n.start as u32, n.length as u32)).collect();
            notes.sort_unstable();
            (name, notes)
        })
        .collect()
}

#[test]
fn clip_round_trips_through_a_file() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.note_clip(11, 1, BAR_TICKS, BAR_TICKS, 0, BAR_TICKS);
    let original = [(36, 0, 480), (43, 960, 240), (48, 1_920, 960)];
    fake.set_notes(10, &original);
    let path = temp_path("round");
    let midi = files(&fake);
    midi.export_clip(10, &path).unwrap();
    assert_eq!(midi.import_into(11, &path).unwrap(), 3);
    std::fs::remove_file(&path).ok();
    assert_eq!(fake.notes_of(11), original);
}

#[test]
fn import_replaces_notes_and_grows_content_only_by_auto_length() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Lead", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.note_clip(11, 1, BAR_TICKS, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(60, 5 * BAR_TICKS / 2, 100)]);
    fake.set_notes(11, &[(30, 0, 100)]);
    let path = temp_path("grow");
    let midi = files(&fake);
    midi.export_clip(10, &path).unwrap();
    midi.import_into(11, &path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(fake.notes_of(11), [(60, 5 * BAR_TICKS / 2, 100)], "the old notes are replaced");
    let clips = fake.clips();
    assert_eq!(clips[1].length_ticks, BAR_TICKS, "the clip length does not change");
    let ClipContent::Notes(content) = &clips[1].content else { panic!() };
    assert_eq!(content.loop_ticks, 3 * BAR_TICKS, "the content grew with auto length");
}

#[test]
fn import_into_manual_length_drops_notes_beyond_it() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Lead", true);
    fake.note_clip(10, 1, 0, 2 * BAR_TICKS, 0, 2 * BAR_TICKS);
    fake.note_clip(11, 1, 2 * BAR_TICKS, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(60, 0, 100), (62, BAR_TICKS + 100, 100)]);
    if let ClipContent::Notes(content) = &mut fake.clips.lock().unwrap()[1].content {
        content.auto_length = false;
    }
    let path = temp_path("manual");
    let midi = files(&fake);
    midi.export_clip(10, &path).unwrap();
    assert_eq!(midi.import_into(11, &path).unwrap(), 1);
    std::fs::remove_file(&path).ok();
    assert_eq!(fake.notes_of(11), [(60, 0, 100)]);
}

#[test]
fn project_export_has_one_midi_track_per_instrument_track_with_notes() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.track(2, "Lead", true);
    fake.track(3, "Empty", true);
    fake.track(4, "Voice", false);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.note_clip(11, 2, BAR_TICKS, BAR_TICKS, 0, BAR_TICKS);
    fake.note_clip(12, 3, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 480)]);
    fake.set_notes(11, &[(72, 240, 240)]);
    let path = temp_path("project");
    files(&fake).export_project(&path).unwrap();
    let tracks = read_back(&path);
    std::fs::remove_file(&path).ok();
    assert_eq!(tracks.len(), 3, "the meta track + two note tracks");
    assert_eq!(tracks[1], (Some("Bass".into()), vec![(36, 0, 480)]));
    assert_eq!(tracks[2], (Some("Lead".into()), vec![(72, BAR_TICKS + 240, 240)]), "absolute time");
}

#[test]
fn project_export_unrolls_repeats_and_cuts_at_clip_end() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    // Content of one bar, clip of a bar and a half: the second repeat is cut in the middle.
    fake.note_clip(10, 1, BAR_TICKS, BAR_TICKS * 3 / 2, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 480), (40, 3_000, 840)]);
    let path = temp_path("unroll");
    files(&fake).export_project(&path).unwrap();
    let tracks = read_back(&path);
    std::fs::remove_file(&path).ok();
    let bar = BAR_TICKS;
    assert_eq!(
        tracks[1].1,
        [(36, bar, 480), (36, 2 * bar, 480), (40, bar + 3_000, 840)],
        "the second note of the repeat is outside the clip"
    );
}

#[test]
fn project_export_respects_clip_offset() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 960, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 100), (40, 960, 100)]);
    let path = temp_path("offset");
    files(&fake).export_project(&path).unwrap();
    let tracks = read_back(&path);
    std::fs::remove_file(&path).ok();
    // Offset 960: the note at 960 plays at the clip start, the note at 0 at the start of the next repeat (2880).
    assert_eq!(tracks[1].1, [(36, 2_880, 100), (40, 0, 100)]);
}

#[test]
fn export_writes_tempo_and_rejects_unknown_clips() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 480)]);
    let path = temp_path("tempo");
    let midi = files(&fake);
    midi.export_clip(10, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let smf = Smf::parse(&bytes).unwrap();
    assert!(smf.tracks[0].iter().any(|e| matches!(e.kind, TrackEventKind::Meta(MetaMessage::Tempo(t)) if t.as_int() == 500_000)));
    assert!(midi.export_clip(99, &path).is_err_and(|e| e.is(daw_contracts::codes::clips::NOT_FOUND)));
    assert!(midi.import_into(99, &path).is_err_and(|e| e.is(daw_contracts::codes::clips::NOT_FOUND)));
}

#[test]
fn the_author_from_preferences_goes_into_the_copyright_event_of_both_exports() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 480)]);
    let preferences = Arc::new(FakePreferences::with_author("Аня К."));
    let midi = files_by(&fake, preferences.clone());
    let (clip, project) = (temp_path("author-clip"), temp_path("author-project"));
    midi.export_clip(10, &clip).unwrap();
    midi.export_project(&project).unwrap();
    assert_eq!(copyright(&clip).as_deref(), Some("Аня К."));
    assert_eq!(copyright(&project).as_deref(), Some("Аня К."));

    // No author means no event; the setting is read on every export.
    preferences.0.lock().unwrap().profile.author = String::new();
    midi.export_clip(10, &clip).unwrap();
    assert_eq!(copyright(&clip), None);
    std::fs::remove_file(&clip).ok();
    std::fs::remove_file(&project).ok();
}

fn tempo(tick: u32, bpm: f64, ramp: bool) -> TempoChange {
    TempoChange { tick, bpm: Bpm::new(bpm).unwrap(), ramp }
}

fn signature(tick: u32, numerator: u8, denominator: u8) -> SignatureChange {
    SignatureChange { tick, signature: TimeSignature::new(numerator, denominator).unwrap() }
}

#[test]
fn the_tempo_map_round_trips_through_a_project_export() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 480)]);
    let map = TempoMap::new(
        vec![tempo(0, 128.0, false), tempo(7680, 140.0, false), tempo(15_360, 97.5, false)],
        vec![signature(0, 4, 4), signature(3840, 7, 8), signature(3840 + 3 * 3360, 3, 4)],
    )
    .unwrap();
    let (source, target) = (Arc::new(FakeTempo(Mutex::new(map.clone()))), Arc::new(FakeTempo::default()));
    let path = temp_path("tempo-map");
    MidiFiles::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), source, Arc::new(FakePreferences::default()))
        .export_project(&path)
        .unwrap();
    let reader =
        MidiFiles::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), target.clone(), Arc::new(FakePreferences::default()));
    reader.import_tempo(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(target.tempo_map(), map);
}

#[test]
fn a_ramp_is_written_as_steps_that_keep_its_timing() {
    let fake = Arc::new(Fake::default());
    let map = TempoMap::new(vec![tempo(0, 100.0, false), tempo(3840, 160.0, true)], vec![signature(0, 4, 4)]).unwrap();
    let (source, target) = (Arc::new(FakeTempo(Mutex::new(map.clone()))), Arc::new(FakeTempo::default()));
    let path = temp_path("tempo-ramp");
    MidiFiles::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), source, Arc::new(FakePreferences::default()))
        .export_project(&path)
        .unwrap();
    MidiFiles::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), target.clone(), Arc::new(FakePreferences::default()))
        .import_tempo(&path)
        .unwrap();
    std::fs::remove_file(&path).ok();
    let read = target.tempo_map();
    assert_eq!(read.tempos().len(), 1 + 16, "a step for every sixteenth of the ramp, then the final tempo");
    assert!(read.tempos().iter().all(|change| !change.ramp));
    for tick in [960.0, 3840.0, 10_000.0] {
        assert!((read.seconds_at(tick) - map.seconds_at(tick)).abs() < 1e-4, "time at tick {tick}");
    }
}

#[test]
fn a_file_without_tempo_events_gives_the_midi_default() {
    let fake = Arc::new(Fake::default());
    fake.track(1, "Bass", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(36, 0, 480)]);
    let smf = Smf { header: midly::Header::new(midly::Format::Parallel, midly::Timing::Metrical(96.into())), tracks: vec![vec![]] };
    let path = temp_path("no-tempo");
    smf.save(&path).unwrap();
    let target = Arc::new(FakeTempo(Mutex::new(TempoMap::default())));
    MidiFiles::new(fake.clone(), fake.clone(), fake.clone(), fake.clone(), target.clone(), Arc::new(FakePreferences::default()))
        .import_tempo(&path)
        .unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(target.tempo_map(), TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR));
}

fn lanes_of(fake: &Fake, clip: daw_contracts::ClipId) -> Vec<daw_contracts::ControllerLane> {
    match &fake.clips().into_iter().find(|c| c.id == clip).unwrap().content {
        ClipContent::Notes(content) => {
            let mut lanes = content.controllers.clone();
            lanes.sort_by_key(|lane| format!("{:?}", lane.kind));
            lanes
        }
        ClipContent::Audio { .. } => unreachable!(),
    }
}

fn set_lane(fake: &Fake, clip: daw_contracts::ClipId, kind: daw_contracts::ControllerKind, points: &[(u32, f32)]) {
    use daw_contracts::{ControllerPoint, NoteControl};
    let points = points.iter().map(|&(tick, value)| ControllerPoint { tick, value }).collect();
    fake.set_controller_lane(clip, kind, points).unwrap();
}

#[test]
fn controller_lanes_round_trip_through_a_file_in_one_undo_step() {
    use daw_contracts::ControllerKind;
    let fake = Arc::new(Fake::default());
    fake.track(1, "Lead", true);
    fake.note_clip(10, 1, 0, BAR_TICKS, 0, BAR_TICKS);
    fake.note_clip(11, 1, BAR_TICKS, BAR_TICKS, 0, BAR_TICKS);
    fake.set_notes(10, &[(60, 0, 480)]);
    // The wheel edges and center are exact; CC and pressure within 1/127.
    set_lane(&fake, 10, ControllerKind::PitchBend, &[(0, -1.0), (240, 0.0), (480, 1.0)]);
    set_lane(&fake, 10, ControllerKind::Cc { number: 64 }, &[(0, 1.0), (900, 0.0)]);
    set_lane(&fake, 10, ControllerKind::Aftertouch, &[(100, 64.0 / 127.0)]);
    // A lane missing in the file goes away on import.
    set_lane(&fake, 11, ControllerKind::Cc { number: 1 }, &[(0, 0.5)]);

    let path = temp_path("controllers");
    let midi = files(&fake);
    midi.export_clip(10, &path).unwrap();
    midi.import_into(11, &path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(lanes_of(&fake, 11), lanes_of(&fake, 10));
    assert_eq!(*fake.groups.lock().unwrap(), (1, 1, 0), "notes and lanes are one undo step");
}

#[test]
fn project_export_unrolls_controller_points_with_the_content() {
    use daw_contracts::ControllerKind;
    let fake = Arc::new(Fake::default());
    fake.track(1, "Lead", true);
    fake.note_clip(10, 1, BAR_TICKS, 2 * BAR_TICKS, 0, BAR_TICKS);
    set_lane(&fake, 10, ControllerKind::Cc { number: 1 }, &[(100, 1.0)]);
    let path = temp_path("project-controllers");
    files(&fake).export_project(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let smf = Smf::parse(&bytes).unwrap();
    let ticks: Vec<u64> = super::read::extract_controllers(&smf.tracks[1]).iter().map(|c| c.time).collect();
    let first = u64::from(BAR_TICKS) + 100;
    assert_eq!(ticks, [first, first + u64::from(BAR_TICKS)], "a track with controllers but no notes is in the file too");
}
