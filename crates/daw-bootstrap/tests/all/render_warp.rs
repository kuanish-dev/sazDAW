//! Full session: fitting an audio clip to the tempo and shifting its pitch are audible live (the sound is computed in
//! the background, the watch refreshes the snapshot) and in the export; overlapping clips play with a crossfade.
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

/// Listens to the song start until the sound in frames `[from, to)` sounds whole (processing runs in the background) and
/// returns the `to` rendered frames. Every piece of the window is checked: the ready sound may arrive mid-render, and
/// then the window start is still quiet; such an attempt is repeated, otherwise the window frequency would come out low.
fn listen_until_sound(session: &mut Session, from: usize, to: usize) -> Vec<f32> {
    // A time limit, not an attempt count: under load (the whole test suite in parallel, a debug build) background
    // stretching takes many times longer than in a single run.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        session.controls.transport.stop();
        render(session, 512);
        session.controls.transport.play();
        let out = render(session, to);
        if out[from * 2..].chunks(2_000).all(|piece| rms(piece) > 0.1) {
            return out;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the processed sound never started sounding");
}

#[test]
fn a_pitched_and_warped_clip_sounds_shifted_live_and_in_the_export() {
    let dir = temp_dir("warp");
    write_wav(&dir.join("tone 60bpm.wav"), 1.0, 440.0);
    let mut session = start(false);
    let controls = session.controls.clone();
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let info = controls.samples.import(&dir.join("tone 60bpm.wav")).unwrap();
    let track = controls.tracks.add_track("Sound", None, 60).unwrap();
    let clip = controls.clips.add_audio_clip(track, 0, info.id).unwrap();

    let edit = ClipEdit { warp: Some(Some(WarpEdit { source_bpm: None })), pitch_semitones: Some(12.0), ..Default::default() };
    let warped = controls.clips.edit_clip(clip.id, edit).unwrap();
    assert_eq!(warped.warp, Some(Warp { source_bpm: 60.0 }), "the tempo from the file name");
    assert_eq!(warped.length_ticks, 960, "a second at 60 BPM is one beat");

    // At 120 BPM a beat is half a second: the sound is squeezed twice, and the pitch is an octave up (880 Hz), not doubled twice (1760).
    let out = listen_until_sound(&mut session, 4_000, 20_000);
    let freq = frequency(&out[4_000 * 2..18_000 * 2]);
    assert!((freq - 880.0).abs() < 25.0, "frequency {freq}");
    let after = render(&mut session, 10_000);
    assert!(rms(&after[6_000 * 2..]) < 1e-3, "the clip ended on the beat");

    // A new tempo: the watch refreshes the snapshot, the sound stretches (a beat at 60 BPM is a second; while the new
    // sound is computed, the old one plays for half a second).
    controls.tempo.set_tempo(Bpm::new(60.0).unwrap()).unwrap();
    let out = listen_until_sound(&mut session, 30_000, 40_000);
    let freq = frequency(&out[30_000 * 2..40_000 * 2]);
    assert!((freq - 880.0).abs() < 25.0, "at 60 BPM the clip sounds a whole second and an octave up: {freq}");

    let path = dir.join("mix.wav");
    let request = ExportRequest {
        sample_rate: 48_000,
        bit_depth: BitDepth::Float32,
        stems: false,
        tail_seconds: 0.0,
        ..ExportRequest::new(path.clone())
    };
    let report = controls.exporter.export(&request, &|_| {}).unwrap();
    assert_eq!(report.frames, 48_000, "one beat at 60 BPM");
    let (_, samples) = read_wav(&path);
    let freq = frequency(&samples[4_000 * 2..44_000 * 2]);
    assert!((freq - 880.0).abs() < 25.0, "export: frequency {freq}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn overlapping_audio_clips_play_a_crossfade() {
    let dir = temp_dir("crossfade");
    write_wav(&dir.join("tone.wav"), 1.0, 220.0);
    let mut session = start(false);
    let controls = session.controls.clone();
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let info = controls.samples.import(&dir.join("tone.wav")).unwrap();
    let track = controls.tracks.add_track("Sound", None, 60).unwrap();
    controls.clips.add_audio_clip(track, 0, info.id).unwrap(); // 0..1920
    let second = controls.clips.add_audio_clip(track, 960, info.id).unwrap();
    assert_eq!(second.crossfades(&controls.clips.clips()), (960, 0));
    controls.transport.play();
    let out = render(&mut session, 48_000 * 3 / 2);
    // The second half of the first second is a crossfade: the sound neither breaks off nor doubles.
    let fade = rms(&out[30_000 * 2..42_000 * 2]);
    let solo = rms(&out[5_000 * 2..20_000 * 2]);
    assert!(fade > solo * 0.5 && fade < solo * 1.5, "crossfade {fade}, one clip {solo}");
    std::fs::remove_dir_all(dir).ok();
}
