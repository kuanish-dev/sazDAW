//! Offline export of the project to audio files (`Exporter`): format, loudness, song range.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ContractError;
use crate::codes::export;

/// Longest tail after the rendered range (reverb, release), in seconds: for export and for bouncing a track.
pub const MAX_TAIL_SECONDS: f32 = 30.0;

/// The one tail rule of export and bounce: `0..=MAX_TAIL_SECONDS` seconds, as `f64` for frame math; outside it (or NaN)
/// is `export::TAIL_OUT_OF_RANGE`.
pub fn check_tail(tail_seconds: f32) -> Result<f64, ContractError> {
    if !(0.0..=MAX_TAIL_SECONDS).contains(&tail_seconds) {
        return Err(export::TAIL_OUT_OF_RANGE.with("seconds", tail_seconds).with("max", MAX_TAIL_SECONDS));
    }
    Ok(f64::from(tail_seconds))
}

/// Bit depth of integer formats (WAV, FLAC). FLAC supports only 16 and 24 bits; MP3 does not use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BitDepth {
    Int16,
    Int24,
    Float32,
}

/// File format; the extension of exported files (and stems) follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Wav,
    /// Lossless, 16 or 24 bits.
    Flac,
    /// Lossy, constant bitrate (`ExportRequest::mp3_bitrate`).
    Mp3,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Mp3 => "mp3",
        }
    }
}

/// Loudness matching: integrated loudness (BS.1770) of `lufs`, true peak no higher than `true_peak_db`.
/// If there is not enough headroom to reach the target, loudness is raised only up to the ceiling (no limiter: it would
/// distort the sound); the value reached comes back in `ExportReport::loudness_lufs`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Normalize {
    pub lufs: f64,
    /// True peak ceiling, dBTP; [`Normalize::DEFAULT_TRUE_PEAK_DB`] by default.
    #[serde(default)]
    pub true_peak_db: Option<f64>,
}

impl Normalize {
    /// Default ceiling: −1 dBTP, the headroom streaming services ask for (codecs raise peaks slightly).
    pub const DEFAULT_TRUE_PEAK_DB: f64 = -1.0;
    pub const LUFS_RANGE: std::ops::RangeInclusive<f64> = -40.0..=0.0;
    pub const TRUE_PEAK_RANGE: std::ops::RangeInclusive<f64> = -12.0..=0.0;

    pub fn ceiling_db(&self) -> f64 {
        self.true_peak_db.unwrap_or(Self::DEFAULT_TRUE_PEAK_DB)
    }
}

/// Which part of the song: `"song"` is from the start to the end of the last clip, `"loop"` the loop region,
/// `{ fromTick, toTick }` a range. The tail (`tail_seconds`) is added after the range end; clips past the range end do
/// not sound in the tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ExportRange {
    #[default]
    Song,
    Loop,
    #[serde(untagged)]
    Ticks {
        from_tick: u32,
        to_tick: u32,
    },
}

/// What to export and where. JSON (RPC `project.export`):
/// `{ path, sampleRate, bitDepth, stems, tailSeconds, format?, mp3Bitrate?, normalize?, dither?, range? }`.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub bit_depth: BitDepth,
    /// Also write each mixer track as a separate file next to it.
    pub stems: bool,
    /// Seconds of tail after the end of the range (reverb, release): [`check_tail`].
    pub tail_seconds: f32,
    #[serde(default)]
    pub format: ExportFormat,
    /// MP3 bitrate, kbit/s: one of [`ExportRequest::MP3_BITRATES`]; [`ExportRequest::DEFAULT_MP3_BITRATE`] by
    /// default.
    #[serde(default)]
    pub mp3_bitrate: Option<u32>,
    #[serde(default)]
    pub normalize: Option<Normalize>,
    /// Triangular (TPDF) noise when rounding to integers; by default only for 16 bits (see
    /// [`ExportRequest::dithers`]).
    #[serde(default)]
    pub dither: Option<bool>,
    #[serde(default)]
    pub range: ExportRange,
}

impl ExportRequest {
    /// Default sample rate (as on a CD).
    pub const DEFAULT_SAMPLE_RATE: u32 = 44_100;
    /// Default tail: enough for a reverb to decay.
    pub const DEFAULT_TAIL_SECONDS: f32 = 2.0;
    pub const MP3_BITRATES: [u32; 4] = [128, 192, 256, 320];
    pub const DEFAULT_MP3_BITRATE: u32 = 320;

    /// The whole song as 24-bit WAV with the default sample rate and tail, no stems and no loudness matching.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            sample_rate: Self::DEFAULT_SAMPLE_RATE,
            bit_depth: BitDepth::Int24,
            stems: false,
            tail_seconds: Self::DEFAULT_TAIL_SECONDS,
            format: ExportFormat::Wav,
            mp3_bitrate: None,
            normalize: None,
            dither: None,
            range: ExportRange::Song,
        }
    }

    /// Whether to add noise when rounding: only for integer samples (WAV/FLAC 16 and 24 bits). Without an explicit
    /// `dither`: yes for 16 bits (the rounding error is audible on quiet tails), no for 24 bits (it is below the noise
    /// of any hardware).
    pub fn dithers(&self) -> bool {
        match (self.format, self.bit_depth) {
            (ExportFormat::Mp3, _) | (_, BitDepth::Float32) => false,
            (_, BitDepth::Int16) => self.dither.unwrap_or(true),
            (_, BitDepth::Int24) => self.dither.unwrap_or(false),
        }
    }
}

/// Export result. Loudness and peak are of the main file (the mix), after loudness matching and before MP3
/// compression; `null` means silence. JSON: `{ files, frames, loudnessLufs, peakDb }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub files: Vec<PathBuf>,
    pub frames: u64,
    /// Integrated BS.1770 loudness, LUFS.
    pub loudness_lufs: Option<f32>,
    /// True peak (with 4x oversampling), dBTP.
    pub peak_db: Option<f32>,
}

/// Offline render of the project to a file, faster than real time.
pub trait Exporter: Send + Sync {
    fn export(&self, request: &ExportRequest, progress: &dyn Fn(f32)) -> Result<ExportReport, ContractError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: serde_json::Value) -> ExportRequest {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn old_requests_keep_todays_behaviour() {
        let request = parse(serde_json::json!({
            "path": "a.wav", "sampleRate": 48000, "bitDepth": "int16", "stems": false, "tailSeconds": 1.0
        }));
        assert_eq!(
            (request.format, request.range, request.normalize, request.mp3_bitrate),
            (ExportFormat::Wav, ExportRange::Song, None, None)
        );
        assert!(request.dithers(), "16 bits dither by default");
        assert!(!ExportRequest { bit_depth: BitDepth::Int24, ..request.clone() }.dithers());
        assert!(!ExportRequest { bit_depth: BitDepth::Float32, dither: Some(true), ..request.clone() }.dithers());
        assert!(!ExportRequest { format: ExportFormat::Mp3, ..request }.dithers());
    }

    #[test]
    fn new_fields_parse_from_camel_case() {
        let request = parse(serde_json::json!({
            "path": "a.flac", "sampleRate": 44100, "bitDepth": "int24", "stems": true, "tailSeconds": 0.0,
            "format": "flac", "normalize": { "lufs": -14.0 }, "dither": true, "range": { "fromTick": 960, "toTick": 3840 }
        }));
        assert_eq!(request.format, ExportFormat::Flac);
        assert_eq!(request.normalize, Some(Normalize { lufs: -14.0, true_peak_db: None }));
        assert_eq!(request.normalize.unwrap().ceiling_db(), -1.0);
        assert_eq!(request.range, ExportRange::Ticks { from_tick: 960, to_tick: 3840 });
        assert!(request.dithers());
        let looped = parse(serde_json::json!({
            "path": "a.mp3", "sampleRate": 44100, "bitDepth": "int16", "stems": false, "tailSeconds": 0.0,
            "format": "mp3", "mp3Bitrate": 192, "range": "loop", "normalize": { "lufs": -9, "truePeakDb": -0.3 }
        }));
        assert_eq!((looped.format, looped.mp3_bitrate, looped.range), (ExportFormat::Mp3, Some(192), ExportRange::Loop));
        assert_eq!(looped.normalize.unwrap().ceiling_db(), -0.3);
    }

    #[test]
    fn report_is_camel_case() {
        let report = ExportReport { files: vec!["a.wav".into()], frames: 10, loudness_lufs: Some(-14.0), peak_db: None };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["loudnessLufs"], -14.0);
        assert!(json["peakDb"].is_null());
    }
}
