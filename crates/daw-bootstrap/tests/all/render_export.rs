//! Full session: export with loudness fitting (a LUFS target and a true peak ceiling), a song span and the loop,
//! FLAC/MP3 formats with stems (the extension follows the format).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

/// 120 BPM, 48 kHz: a 4/4 bar is 2 s.
const RATE: u32 = 48_000;

/// A session with an audio track: a 1 kHz sine with peak 0.1 (−20 dBFS) for 4 seconds from the song start.
/// For a 1 kHz sine in both channels the loudness in LUFS equals the peak in dBFS (EBU Tech 3341), so the file peak checks it.
fn tone_session(dir: &Path) -> Session {
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let path = dir.join("tone.wav");
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..RATE as usize * 4 {
        writer.write_sample(0.1 * (std::f32::consts::TAU * 1000.0 * i as f32 / RATE as f32).sin()).unwrap();
    }
    writer.finalize().unwrap();
    let session = start(false);
    let controls = &session.controls;
    controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    let sample = controls.samples.import(&path).unwrap();
    let track = controls.tracks.add_track("Tone", None, 60).unwrap();
    controls.clips.add_audio_clip(track, 0, sample.id).unwrap();
    session
}

fn float_wav(path: PathBuf) -> ExportRequest {
    ExportRequest { sample_rate: RATE, bit_depth: BitDepth::Float32, tail_seconds: 0.0, ..ExportRequest::new(path) }
}

fn db(value: f32) -> f32 {
    20.0 * value.log10()
}

#[test]
fn normalization_hits_the_target_loudness() {
    let dir = temp_dir("export-normalize");
    let session = tone_session(&dir);
    let request = ExportRequest { normalize: Some(Normalize { lufs: -14.0, true_peak_db: None }), ..float_wav(dir.join("mix.wav")) };
    let report = session.controls.exporter.export(&request, &|_| {}).unwrap();

    let lufs = report.loudness_lufs.unwrap();
    assert!((lufs + 14.0).abs() <= 0.5, "loudness {lufs} LUFS");
    let (_, mix) = read_wav(&report.files[0]);
    let file_peak = db(peak(&mix));
    assert!((file_peak + 14.0).abs() <= 0.5, "file peak {file_peak} dBFS");
    assert!((report.peak_db.unwrap() - file_peak).abs() <= 0.2, "report {:?} ≈ file {file_peak}", report.peak_db);
    assert!(!dir.join("mix.wav.render-tmp").exists(), "the temporary file is removed");

    let plain = session.controls.exporter.export(&float_wav(dir.join("plain.wav")), &|_| {}).unwrap();
    let before = db(peak(&read_wav(&plain.files[0]).1));
    assert!((plain.loudness_lufs.unwrap() - before).abs() <= 0.5, "{:?} vs {before}", plain.loudness_lufs);
    std::fs::remove_dir_all(dir).ok();
}

/// A −0.5 LUFS target needs a −0.5 dB peak, above the −1 dBTP ceiling: the loudness rises only up to the ceiling, and
/// the report says an honest −1 LUFS, not the target.
#[test]
fn normalization_stops_at_the_true_peak_ceiling_and_reports_what_it_reached() {
    let dir = temp_dir("export-ceiling");
    let session = tone_session(&dir);
    let request = ExportRequest { normalize: Some(Normalize { lufs: -0.5, true_peak_db: None }), ..float_wav(dir.join("mix.wav")) };
    let report = session.controls.exporter.export(&request, &|_| {}).unwrap();
    let (_, mix) = read_wav(&report.files[0]);
    assert!(db(peak(&mix)) <= -0.95, "file peak {}", db(peak(&mix)));
    assert!(report.peak_db.unwrap() <= -0.95, "{:?}", report.peak_db);
    let lufs = report.loudness_lufs.unwrap();
    assert!((lufs + 1.0).abs() <= 0.3, "reached {lufs} LUFS instead of −0.5");

    let silent = ExportRequest { range: ExportRange::Ticks { from_tick: 3840 * 3, to_tick: 3840 * 4 }, ..request };
    assert!(matches!(session.controls.exporter.export(&silent, &|_| {}), Err(e) if e.is(codes::export::TOO_QUIET_TO_NORMALIZE)));
    assert!(!dir.join("mix.wav.render-tmp").exists());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_range_and_the_loop_export_only_their_part_plus_the_tail() {
    let dir = temp_dir("export-range");
    let session = tone_session(&dir);
    let controls = &session.controls;
    let ranged = ExportRequest {
        range: ExportRange::Ticks { from_tick: 960, to_tick: 4800 },
        tail_seconds: 0.5,
        ..float_wav(dir.join("range.wav"))
    };
    let report = controls.exporter.export(&ranged, &|_| {}).unwrap();
    assert_eq!(report.frames, u64::from(RATE) * 5 / 2);
    let (_, audio) = read_wav(&report.files[0]);
    assert_eq!(audio.len() as u64, report.frames * 2);
    let second = RATE as usize * 2;
    assert!(rms(&audio[..second * 2]) > 0.03, "the span sounds from start to end");
    assert!(rms(&audio[second * 2 + 200..]) < 1e-4, "the clip does not sound in the tail: it is cut at the span end");

    controls.clips.set_loop_region(Some((3840, 5760))).unwrap();
    let looped = controls.exporter.export(&ExportRequest { range: ExportRange::Loop, ..float_wav(dir.join("loop.wav")) }, &|_| {}).unwrap();
    assert_eq!(looped.frames, u64::from(RATE));
    std::fs::remove_dir_all(dir).ok();
}

/// FLAC and MP3 with stems: every file has the format's extension, and the project sample decoder reads them.
#[test]
fn flac_and_mp3_exports_with_stems_follow_the_format() {
    let dir = temp_dir("export-formats");
    let session = start(true);
    let controls = &session.controls;
    let one_bar = ExportRange::Ticks { from_tick: 0, to_tick: BAR_TICKS };
    for (format, depth) in [(ExportFormat::Flac, BitDepth::Int24), (ExportFormat::Mp3, BitDepth::Int16)] {
        let request = ExportRequest {
            format,
            bit_depth: depth,
            stems: true,
            range: one_bar,
            tail_seconds: 0.0,
            ..ExportRequest::new(dir.join("song.wav"))
        };
        let report = controls.exporter.export(&request, &|_| {}).unwrap();
        assert_eq!(report.files.len(), 4, "mix + 3 buses");
        for file in &report.files {
            assert_eq!(file.extension().unwrap(), format.extension(), "{}", file.display());
            let imported = controls.samples.import(file).unwrap();
            assert!(imported.frame_count > 0, "{} is readable", file.display());
        }
        assert!(report.loudness_lufs.is_some());
    }
    assert!(controls.log.recent(50).iter().all(|e| e.source != "samples"), "the files were read whole, without warnings");
    std::fs::remove_dir_all(dir).ok();
}
