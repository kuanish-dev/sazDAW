//! Full session and MIDI effects: an arpeggiator on a track sounds the same as the same notes written by hand, both in
//! playback and in the export (the offline engine builds the same chain).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

/// A bar at 120 BPM and 48 kHz is 96 000 frames; it is rendered with room for the tail.
const FRAMES: usize = 110_000;

/// A session with a synth track and a bar of notes: with `arp`, C and E for the whole bar through the arpeggiator
/// (1/16, 50 % length), otherwise the same arpeggio written as notes.
fn session(arp: bool) -> Session {
    let session = start(false);
    let controls = &session.controls;
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    // The tails after the bar are compared, not the song looping back to its start.
    controls.transport.set_song_loop(false);
    let track = add_synth_track(controls, "Synth");
    let clip = note_clip(controls, track, 0, 1).id;
    if arp {
        controls.notes.add_notes(clip, vec![note(60, 0, BAR_TICKS), note(64, 0, BAR_TICKS)]).unwrap();
        controls.note_effects.add(track, "arp", Params::new()).unwrap();
    } else {
        let steps = (0..16).map(|step| note(if step % 2 == 0 { 60 } else { 64 }, step * 240, 120)).collect();
        controls.notes.add_notes(clip, steps).unwrap();
    }
    session
}

fn max_difference(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

#[test]
fn arpeggiated_playback_sounds_like_the_written_out_notes() {
    let mut results = Vec::new();
    for arp in [true, false] {
        let mut session = session(arp);
        session.controls.transport.play();
        results.push(render(&mut session, FRAMES));
    }
    assert!(peak(&results[0]) > 0.01, "the arpeggio sounds");
    assert!(max_difference(&results[0], &results[1]) < 1e-4, "playback with the arpeggiator = the written notes");
}

#[test]
fn export_uses_the_same_chain_as_playback() {
    let dir = temp_dir("export-note-effects");
    let mut files = Vec::new();
    for (arp, name) in [(true, "arp.wav"), (false, "notes.wav")] {
        let session = session(arp);
        let request = ExportRequest { sample_rate: 48_000, bit_depth: BitDepth::Float32, ..ExportRequest::new(dir.join(name)) };
        let report = session.controls.exporter.export(&request, &|_| {}).unwrap();
        files.push(read_wav(&report.files[0]).1);
    }
    assert!(peak(&files[0]) > 0.01, "the arpeggio sounds in the export");
    assert!(max_difference(&files[0], &files[1]) < 1e-4, "export with the arpeggiator = the written notes");
    std::fs::remove_dir_all(dir).ok();
}
