//! Full session and the tempo map: a tempo change sounds at the right frame, the metronome follows a meter change,
//! an export with a smooth tempo ramp has the right length.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

fn tempo(tick: u32, bpm: f64, ramp: bool) -> TempoChange {
    TempoChange { tick, bpm: Bpm::new(bpm).unwrap(), ramp }
}

fn signature(tick: u32, numerator: u8, denominator: u8) -> SignatureChange {
    SignatureChange { tick, signature: TimeSignature::new(numerator, denominator).unwrap() }
}

#[test]
fn a_note_after_a_tempo_change_starts_on_the_frame_of_the_map() {
    let mut session = start(false);
    let controls = session.controls.clone();
    controls.tempo.set_tempo_changes(vec![tempo(0, 120.0, false), tempo(1920, 60.0, false)]).unwrap();
    synth_track_with_note(&controls, note(60, 2880, 480));
    controls.transport.play();
    let out = render(&mut session, 100_000);
    let first = out.chunks(2).position(|frame| frame[0].abs() > 1e-4).expect("the note sounds");
    assert!((96_000..96_000 + 64).contains(&first), "the first sound at frame {first}");
}

/// The metronome click frequency in a window after a beat: the accent is 1500 Hz, a regular beat 1000 Hz.
fn is_accent(out: &[f32], frame: usize) -> bool {
    let window: Vec<f32> = out[frame * 2..(frame + 1_900) * 2].chunks(2).map(|f| f[0]).collect();
    let crossings = window.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    assert!(crossings > 20, "there is a click at frame {frame}");
    crossings > 50
}

#[test]
fn metronome_accents_follow_a_three_four_to_four_four_change() {
    let mut session = start(false);
    let controls = session.controls.clone();
    controls.metronome.set_enabled(true);
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    controls.tempo.set_signature_changes(vec![signature(0, 3, 4), signature(2880, 4, 4)]).unwrap();
    controls.transport.play();
    let out = render(&mut session, 8 * 24_000 + 2_000);
    let accents: Vec<bool> = (0..8).map(|beat| is_accent(&out, beat * 24_000)).collect();
    assert_eq!(accents, [true, false, false, true, false, false, false, true]);
}

#[test]
fn export_length_follows_a_tempo_ramp() {
    let dir = temp_dir("export-ramp");
    let session = start(false);
    let controls = session.controls.clone();
    synth_track_with_note(&controls, note(60, 0, 480));
    controls.tempo.set_tempo_changes(vec![tempo(0, 60.0, false), tempo(3840, 120.0, true)]).unwrap();
    let request = ExportRequest {
        sample_rate: 48_000,
        bit_depth: BitDepth::Int16,
        stems: false,
        tail_seconds: 0.0,
        ..ExportRequest::new(dir.join("ramp.wav"))
    };
    let report = controls.exporter.export(&request, &|_| {}).unwrap();
    assert_eq!(report.frames, (4.0 * 2f64.ln() * 48_000.0).ceil() as u64);
    std::fs::remove_dir_all(dir).ok();
}
