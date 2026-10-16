//! Encoded files are read back with the symphonia decoder (the one the project reads samples with): FLAC bit-exact,
//! MP3 audible with the right duration, WAV with its bit depth; no unfinished file is left behind.

use std::fs::File;
use std::path::{Path, PathBuf};

use daw_contracts::codes::{export, general};
use daw_contracts::{BitDepth, ContractError, ExportFormat, ExportRequest};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use super::pcm::Quantizer;
use super::{check, write_file};

fn temp(name: &str, file: &str) -> PathBuf {
    std::env::temp_dir().join(format!("daw-render-encode-{name}-{}", std::process::id())).join(file)
}

fn cleanup(path: &Path) {
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// A stereo signal: different sines per channel and a little "noise", so FLAC has prediction errors to encode.
fn signal(frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let t = i as f32 / 44_100.0;
            let left = 0.6 * (std::f32::consts::TAU * 440.0 * t).sin() + 0.01 * ((i * 7919) % 101) as f32 / 101.0;
            let right = 0.4 * (std::f32::consts::TAU * 660.0 * t).sin();
            [left, right]
        })
        .collect()
}

fn request(file: &Path, format: ExportFormat, bit_depth: BitDepth) -> ExportRequest {
    ExportRequest { format, bit_depth, sample_rate: 44_100, ..ExportRequest::new(file.to_path_buf()) }
}

/// Writes `audio` in chunks of uneven length (as the render delivers it).
fn encode(request: &ExportRequest, audio: &[f32]) -> Result<(), ContractError> {
    write_file(&request.path, request, |encoder| {
        for chunk in audio.chunks(4096 * 2 + 6) {
            encoder.write(chunk)?;
        }
        Ok(())
    })
}

/// Reads a file back: rate, channels and interleaved float samples.
fn decode(path: &Path) -> (u32, usize, Vec<f32>) {
    let stream = MediaSourceStream::new(Box::new(File::open(path).unwrap()), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(path.extension().unwrap().to_str().unwrap());
    let mut format = symphonia::default::get_probe().probe(&hint, stream, FormatOptions::default(), MetadataOptions::default()).unwrap();
    let track = format.default_track(TrackType::Audio).unwrap();
    let track_id = track.id;
    let params = track.codec_params.as_ref().unwrap().audio().unwrap().clone();
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).unwrap();
    let (mut rate, mut channels, mut samples) = (0, 0, Vec::new());
    while let Some(packet) = format.next_packet().unwrap() {
        if packet.track_id != track_id {
            continue;
        }
        let buffer = decoder.decode(&packet).unwrap();
        rate = buffer.spec().rate();
        channels = buffer.spec().channels().count();
        let mut chunk = vec![0.0f32; buffer.samples_interleaved()];
        buffer.copy_to_slice_interleaved(&mut chunk);
        samples.extend_from_slice(&chunk);
    }
    (rate, channels, samples)
}

/// What an integer format must contain: the same rounding (and the same noise, which has a fixed seed).
fn expected_ints(audio: &[f32], bits: u32, dither: bool) -> Vec<i32> {
    let mut quantizer = Quantizer::new(bits, dither);
    audio.iter().map(|&s| quantizer.quantize(s)).collect()
}

#[test]
fn flac_decodes_back_bit_exact_at_16_and_24_bits() {
    let audio = signal(44_100 + 123);
    for (depth, bits, dither) in [(BitDepth::Int16, 16, None), (BitDepth::Int24, 24, None), (BitDepth::Int24, 24, Some(true))] {
        let path = temp(&format!("flac{bits}"), "out.flac");
        let request = ExportRequest { dither, ..request(&path, ExportFormat::Flac, depth) };
        encode(&request, &audio).unwrap();
        let (rate, channels, decoded) = decode(&path);
        assert_eq!((rate, channels, decoded.len()), (44_100, 2, audio.len()));
        // symphonia divides integers by 2^(bits-1), so multiplying restores them exactly.
        let scale = f64::from(1u32 << (bits - 1));
        let ints: Vec<i32> = decoded.iter().map(|&s| (f64::from(s) * scale) as i32).collect();
        assert!(ints == expected_ints(&audio, bits, request.dithers()), "{bits} bits, dither {:?}: not bit-exact", request.dithers());
        cleanup(&path);
    }
}

#[test]
fn flac_refuses_float_samples() {
    let request = request(Path::new("x.flac"), ExportFormat::Flac, BitDepth::Float32);
    assert!(matches!(check(&request), Err(e) if e.is(export::FLAC_FLOAT_UNSUPPORTED)));
}

#[test]
fn mp3_decodes_with_about_the_right_duration_and_level() {
    let audio = signal(44_100 * 2);
    for bitrate in ExportRequest::MP3_BITRATES {
        let path = temp(&format!("mp3-{bitrate}"), "out.mp3");
        let request = ExportRequest { mp3_bitrate: Some(bitrate), ..request(&path, ExportFormat::Mp3, BitDepth::Int16) };
        encode(&request, &audio).unwrap();
        let bytes = std::fs::metadata(&path).unwrap().len();
        // Constant bitrate: 2 s ≈ bitrate·2/8 KB (plus the encoder delay frames).
        let expected = u64::from(bitrate) * 1000 / 8 * 2;
        assert!(bytes > expected * 9 / 10 && bytes < expected * 12 / 10, "{bitrate} kbps: {bytes} bytes");
        let (rate, channels, decoded) = decode(&path);
        assert_eq!((rate, channels), (44_100, 2));
        let seconds = decoded.len() as f32 / 2.0 / 44_100.0;
        // Without the LAME tag the decoder hears the encoder delay and last-frame padding: up to ~0.08 s extra.
        assert!((2.0..2.1).contains(&seconds), "{bitrate} kbps: {seconds} s");
        let peak = decoded.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!((0.5..0.75).contains(&peak), "{bitrate} kbps: peak {peak}");
        cleanup(&path);
    }
}

#[test]
fn mp3_resamples_rates_above_48k_and_refuses_odd_bitrates() {
    let path = temp("mp3-96k", "out.mp3");
    let request96 = ExportRequest { sample_rate: 96_000, ..request(&path, ExportFormat::Mp3, BitDepth::Int16) };
    encode(&request96, &signal(96_000)).unwrap();
    let (rate, _, decoded) = decode(&path);
    assert!(rate <= 48_000, "MP3 has no rates above 48 kHz: {rate}");
    let seconds = decoded.len() as f32 / 2.0 / rate as f32;
    assert!((1.0..1.1).contains(&seconds), "{seconds} s");
    cleanup(&path);

    let odd = ExportRequest { mp3_bitrate: Some(100), ..request(&path, ExportFormat::Mp3, BitDepth::Int16) };
    assert!(matches!(check(&odd), Err(e) if e.is(export::MP3_BITRATE_UNSUPPORTED)));
}

#[test]
fn wav_keeps_the_bit_depth_and_the_old_rounding() {
    let path = temp("wav24", "out.wav");
    encode(&request(&path, ExportFormat::Wav, BitDepth::Int24), &[-2.0, -2.0, 0.5, 0.5]).unwrap();
    let reader = hound::WavReader::open(&path).unwrap();
    assert_eq!(reader.spec().bits_per_sample, 24);
    assert_eq!(reader.into_samples::<i32>().map(Result::unwrap).collect::<Vec<_>>(), [-8_388_607, -8_388_607, 4_194_304, 4_194_304]);
    cleanup(&path);
}

/// 16 bits are dithered by default: silence becomes ±1 LSB noise instead of staying zeros.
#[test]
fn wav_16_bit_is_dithered_by_default() {
    let path = temp("wav16", "out.wav");
    encode(&request(&path, ExportFormat::Wav, BitDepth::Int16), &vec![0.0; 20_000]).unwrap();
    let samples: Vec<i16> = hound::WavReader::open(&path).unwrap().into_samples::<i16>().map(Result::unwrap).collect();
    assert!(samples.iter().all(|s| s.abs() <= 1));
    assert!(samples.iter().any(|&s| s != 0));
    let mean = samples.iter().map(|&s| f64::from(s)).sum::<f64>() / samples.len() as f64;
    assert!(mean.abs() < 0.02, "{mean}");
    cleanup(&path);
}

#[test]
fn a_failed_render_leaves_no_partial_file() {
    for format in [ExportFormat::Wav, ExportFormat::Flac, ExportFormat::Mp3] {
        let path = temp("partial", &format!("out.{}", format.extension()));
        let result = write_file(&path, &request(&path, format, BitDepth::Int16), |encoder| {
            encoder.write(&vec![0.5; 20_000])?;
            Err::<(), _>(general::INTERNAL.with("details", "failure mid-render"))
        });
        assert!(result.is_err());
        assert!(!path.exists(), "unfinished {format:?} removed");
        cleanup(&path);
    }
}
