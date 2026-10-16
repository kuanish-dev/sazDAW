//! `Exporter for OfflineExporter`: "write the project to audio files". What to play is in `range.rs`, which format to
//! write in `crate::encode`, the loudness of the result and its normalization in `loudness.rs` (audio between
//! measuring and writing in `scratch.rs`), bus stems in `crate::stems`. The offline engine plays
//! (`OfflineExporter::load_engine`).

mod loudness;
mod range;
mod scratch;

use std::path::{Path, PathBuf};

use daw_contracts::codes::export;
use daw_contracts::{ContractError, ExportFormat, ExportReport, ExportRequest, Exporter, Normalize, Project};

use crate::{CHUNK_FRAMES, OfflineExporter, encode, stems};
use loudness::{Levels, OutputLevels};
use range::Span;
use scratch::Scratch;

const SAMPLE_RATES: std::ops::RangeInclusive<u32> = 8_000..=192_000;

impl Exporter for OfflineExporter {
    fn export(&self, request: &ExportRequest, progress: &dyn Fn(f32)) -> Result<ExportReport, ContractError> {
        check(request)?;
        let project = self.project.snapshot();
        let span = range::span(&project, request)?;
        let path = file_for_format(&request.path, request.format);
        let stems = if request.stems { stems::stem_buses(&span.project.graph) } else { Vec::new() };
        // Progress parts: the mix, the stems and, when normalizing, writing the mix from the temporary file.
        let parts = (stems.len() + 1 + usize::from(request.normalize.is_some())) as f32;
        let part = |index: usize| move |p: f32| progress((index as f32 + p) / parts);

        let (levels, gain) = match &request.normalize {
            None => (self.render_file(&span.project, &span, request, &path, 1.0, &part(0))?, 1.0),
            Some(target) => {
                let scratch = Scratch::path_for(&path);
                let result = self.render_normalized(&span, request, &path, target, &scratch, [&part(0), &part(1)]);
                scratch::remove(&scratch, result)?
            }
        };
        let mut files = vec![path.clone()];
        let first_stem = 1 + usize::from(request.normalize.is_some());
        // Stems get the same gain as the mix, so together they still add up to it.
        for (index, (bus, name)) in stems.iter().enumerate() {
            let mut reduced = span.project.clone();
            reduced.graph = stems::stem_graph(&span.project.graph, *bus);
            let stem_path = stems::stem_path(&path, *bus, name);
            self.render_file(&reduced, &span, request, &stem_path, gain, &part(first_stem + index))?;
            files.push(stem_path);
        }
        progress(1.0);
        Ok(ExportReport { files, frames: span.frames, loudness_lufs: levels.lufs, peak_db: levels.peak_rounded() })
    }
}

impl OfflineExporter {
    /// Plays `project` from the start of `span` and hands the audio to `each` in chunks. After every chunk the control
    /// side frees what the engine released (finished voices, replaced objects): otherwise the queue would overflow on
    /// a long export.
    fn play(
        &self,
        project: &Project,
        span: &Span,
        sample_rate: u32,
        mut each: impl FnMut(&mut [f32]) -> Result<(), ContractError>,
        progress: &dyn Fn(f32),
    ) -> Result<(), ContractError> {
        let mut engine = self.load_engine(project, sample_rate)?;
        engine.transport.seek(u64::from(span.from_tick));
        engine.transport.play();
        let mut buffer = vec![0.0f32; CHUNK_FRAMES * 2];
        let mut done = 0u64;
        while done < span.frames {
            let frames = (span.frames - done).min(CHUNK_FRAMES as u64) as usize;
            let chunk = &mut buffer[..frames * 2];
            engine.renderer.render(chunk);
            engine.dsp.collect_garbage();
            each(chunk)?;
            done += frames as u64;
            progress(done as f32 / span.frames as f32);
        }
        Ok(())
    }

    /// Renders `project` to `path` in the requested format with `gain` applied, and measures what was written.
    fn render_file(
        &self,
        project: &Project,
        span: &Span,
        request: &ExportRequest,
        path: &Path,
        gain: f32,
        progress: &dyn Fn(f32),
    ) -> Result<Levels, ContractError> {
        encode::write_file(path, request, |encoder| {
            let mut levels = OutputLevels::new(request.sample_rate);
            let write = |chunk: &mut [f32]| {
                apply_gain(chunk, gain);
                levels.push(chunk);
                encoder.write(chunk)
            };
            self.play(project, span, request.sample_rate, write, progress)?;
            Ok(levels.finish())
        })
    }

    /// Loudness normalization: render to a temporary file while measuring, gain from `loudness::gain_db`, then write
    /// the export file from the temporary one. Returns the levels of the written file and the linear gain (for stems).
    fn render_normalized(
        &self,
        span: &Span,
        request: &ExportRequest,
        path: &Path,
        target: &Normalize,
        scratch_path: &Path,
        progress: [&dyn Fn(f32); 2],
    ) -> Result<(Levels, f32), ContractError> {
        let mut scratch = Scratch::create(scratch_path)?;
        let mut before = OutputLevels::new(request.sample_rate);
        let measure = |chunk: &mut [f32]| {
            before.push(chunk);
            scratch.write(chunk)
        };
        self.play(&span.project, span, request.sample_rate, measure, progress[0])?;
        let gain = 10f64.powf(loudness::gain_db(target, before.finish())? / 20.0) as f32;
        let levels = encode::write_file(path, request, |encoder| {
            let mut after = OutputLevels::new(request.sample_rate);
            let write = |chunk: &mut [f32]| {
                apply_gain(chunk, gain);
                after.push(chunk);
                encoder.write(chunk)
            };
            scratch.read(write, progress[1])?;
            Ok(after.finish())
        })?;
        Ok((levels, gain))
    }
}

/// Everything that can be checked before rendering: rate, format, loudness target.
fn check(request: &ExportRequest) -> Result<(), ContractError> {
    if !SAMPLE_RATES.contains(&request.sample_rate) {
        return Err(export::SAMPLE_RATE_OUT_OF_RANGE
            .with("rate", request.sample_rate)
            .with("min", SAMPLE_RATES.start())
            .with("max", SAMPLE_RATES.end()));
    }
    encode::check(request)?;
    request.normalize.as_ref().map_or(Ok(()), loudness::check)
}

fn apply_gain(chunk: &mut [f32], gain: f32) {
    if gain != 1.0 {
        for sample in chunk.iter_mut() {
            *sample *= gain;
        }
    }
}

/// The file extension follows the format: an audio extension (`.wav`, `.flac`, `.mp3`) is replaced, any other is
/// appended to (`song.v2` becomes `song.v2.flac`, not `song.flac`).
fn file_for_format(path: &Path, format: ExportFormat) -> PathBuf {
    let audio = [ExportFormat::Wav, ExportFormat::Flac, ExportFormat::Mp3].map(ExportFormat::extension);
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if audio.iter().any(|a| a.eq_ignore_ascii_case(ext)) => path.with_extension(format.extension()),
        _ => {
            let mut name = path.as_os_str().to_owned();
            name.push(".");
            name.push(format.extension());
            PathBuf::from(name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_follows_the_format() {
        let flac = |p: &str| file_for_format(Path::new(p), ExportFormat::Flac);
        assert_eq!(flac("/a/song.wav"), Path::new("/a/song.flac"));
        assert_eq!(flac("/a/song.MP3"), Path::new("/a/song.flac"));
        assert_eq!(flac("/a/song.flac"), Path::new("/a/song.flac"));
        assert_eq!(flac("/a/song"), Path::new("/a/song.flac"));
        assert_eq!(flac("/a/song.v2"), Path::new("/a/song.v2.flac"));
        assert_eq!(file_for_format(Path::new("mix.wav"), ExportFormat::Wav), Path::new("mix.wav"));
    }
}
