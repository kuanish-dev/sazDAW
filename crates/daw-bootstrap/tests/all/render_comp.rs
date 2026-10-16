//! Full session: a comp of takes sounds the same live as in the export, and the right take plays at the seam.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

/// The frequency by zero crossings in a span of a 48 kHz stereo buffer.
fn frequency(buffer: &[f32]) -> f32 {
    zero_crossings(buffer) as f32 * 48_000.0 / (buffer.len() / 2) as f32
}

#[test]
fn a_comped_clip_sounds_the_same_live_and_in_the_export() {
    let dir = temp_dir("comp");
    write_wav(&dir.join("take 1.wav"), 2.0, 220.0);
    write_wav(&dir.join("take 2.wav"), 2.0, 440.0);
    let mut session = start(false);
    let controls = session.controls.clone();
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let one = controls.samples.import(&dir.join("take 1.wav")).unwrap().id;
    let two = controls.samples.import(&dir.join("take 2.wav")).unwrap().id;
    let track = controls.tracks.add_track("Voice", None, 60).unwrap();
    let clip = controls.clips.add_audio_clip(track, 0, one).unwrap(); // 2 s = 3840 ticks
    controls.takes.add_take(clip.id, two).unwrap();
    let comped = controls.takes.choose_take_range(clip.id, 0, 1920, one).unwrap();
    assert_eq!(comped.comp().len(), 2, "the first second is the first take, the second one the second");

    controls.transport.play();
    let live = render(&mut session, 96_000);
    assert!((frequency(&live[4_000 * 2..44_000 * 2]) - 220.0).abs() < 10.0, "the first second is 220 Hz");
    assert!((frequency(&live[52_000 * 2..92_000 * 2]) - 440.0).abs() < 10.0, "the second is 440 Hz");

    let path = dir.join("mix.wav");
    let request = ExportRequest {
        sample_rate: 48_000,
        bit_depth: BitDepth::Float32,
        stems: false,
        tail_seconds: 0.0,
        ..ExportRequest::new(path.clone())
    };
    let report = controls.exporter.export(&request, &|_| {}).unwrap();
    assert_eq!(report.frames, 96_000);
    let (_, exported) = read_wav(&path);
    let difference = live.iter().zip(&exported).fold(0.0f32, |worst, (a, b)| worst.max((a - b).abs()));
    assert!(difference < 1e-5, "export = live sound, difference {difference}");
    std::fs::remove_dir_all(dir).ok();
}
