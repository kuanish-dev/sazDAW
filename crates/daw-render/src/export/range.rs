//! Which part of the song is exported (`ExportRange`): the tick to play from, how many frames to write (the range by
//! the tempo map plus the tail) and a project in which clips past the range end are silent, so the tail holds only
//! decaying effects and released notes, not the start of the next part of the song.

use daw_contracts::codes::export;
use daw_contracts::{ContractError, ExportRange, ExportRequest, Project, check_tail};

/// The longest export: an hour.
const MAX_SECONDS: u32 = 60 * 60;

pub(crate) struct Span {
    pub from_tick: u32,
    /// Frames to write: the range and the tail.
    pub frames: u64,
    /// The project to render, with clips cut at the range end.
    pub project: Project,
}

pub(crate) fn span(project: &Project, request: &ExportRequest) -> Result<Span, ContractError> {
    let (from, to) = ticks(project, request.range)?;
    // The length follows the tempo map (changes and ramps); the engine plays by the same map from the snapshot.
    let body = project.tempo_map.frames_between(f64::from(from), f64::from(to), request.sample_rate).ceil() as u64;
    let tail = (check_tail(request.tail_seconds)? * f64::from(request.sample_rate)) as u64;
    let frames = body + tail;
    if frames > u64::from(MAX_SECONDS) * u64::from(request.sample_rate) {
        return Err(export::EXPORT_TOO_LONG.with("seconds", frames / u64::from(request.sample_rate)).with("max", MAX_SECONDS));
    }
    Ok(Span { from_tick: from, frames, project: cut_at(project, to) })
}

/// The range `[from, to)` in ticks; an empty one is an error that names the reason.
fn ticks(project: &Project, range: ExportRange) -> Result<(u32, u32), ContractError> {
    let (from, to) = match range {
        ExportRange::Song => {
            let end = project.song_end();
            if end == 0 {
                return Err(export::NOTHING_TO_EXPORT.err());
            }
            (0, u32::try_from(end).unwrap_or(u32::MAX))
        }
        ExportRange::Loop => project.loop_region.ok_or(export::NO_LOOP_REGION)?,
        ExportRange::Ticks { from_tick, to_tick } => (from_tick, to_tick),
    };
    if from >= to {
        return Err(export::RANGE_BACKWARDS.with("from", from).with("to", to));
    }
    Ok((from, to))
}

/// The project without clip audio after `to`: clips starting there are removed, clips crossing `to` are shortened to it.
/// A shortened clip loses its fade-out: otherwise the fade would move into the range and change its sound.
fn cut_at(project: &Project, to: u32) -> Project {
    let mut cut = project.clone();
    cut.clips.retain(|clip| clip.start_tick < to);
    for clip in &mut cut.clips {
        if clip.end_tick() > u64::from(to) {
            clip.length_ticks = to - clip.start_tick;
            clip.fade_out_ticks = 0;
            clip.fade_in_ticks = clip.fade_in_ticks.min(clip.length_ticks);
        }
    }
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{Bpm, Clip, ClipContent, MAX_TAIL_SECONDS, NoteContent, TempoChange, TempoMap, TimeSignature};

    fn clip(id: u32, start: u32, length: u32) -> Clip {
        Clip {
            id,
            track: 1,
            start_tick: start,
            length_ticks: length,
            content: ClipContent::Notes(NoteContent::empty()),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        }
    }

    fn request(sample_rate: u32, tail_seconds: f32) -> ExportRequest {
        ExportRequest { sample_rate, tail_seconds, ..ExportRequest::new("mix.wav".into()) }
    }

    fn frames(project: &Project, request: &ExportRequest) -> u64 {
        span(project, request).unwrap().frames
    }

    #[test]
    fn length_follows_the_tempo_and_adds_the_tail() {
        let mut project = Project::empty();
        project.clips.push(clip(1, 0, 960));
        // 960 ticks = one beat = 0.5 s at 120 BPM.
        project.tempo_map = TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::FOUR_FOUR);
        assert_eq!(frames(&project, &request(48_000, 0.0)), 24_000);
        project.tempo_map = TempoMap::constant(Bpm::new(60.0).unwrap(), TimeSignature::FOUR_FOUR);
        assert_eq!(frames(&project, &request(44_100, 1.0)), 2 * 44_100);
        project.clips.push(clip(2, 3840, 960));
        project.clips.push(clip(3, 0, 1920));
        assert_eq!(frames(&project, &request(48_000, 0.0)), 240_000, "up to the end of the last clip: 4800 ticks");
    }

    #[test]
    fn length_follows_a_tempo_ramp() {
        let mut project = Project::empty();
        project.clips.push(clip(1, 0, 3840));
        // 60 → 120 BPM ramped over a bar: 4·ln2 ≈ 2.7726 s.
        let changes = vec![
            TempoChange { tick: 0, bpm: Bpm::new(60.0).unwrap(), ramp: false },
            TempoChange { tick: 3840, bpm: Bpm::new(120.0).unwrap(), ramp: true },
        ];
        project.tempo_map = project.tempo_map.with_tempos(changes).unwrap();
        let expected = (4.0 * 2f64.ln() * 48_000.0).ceil() as u64;
        assert_eq!(frames(&project, &request(48_000, 0.0)), expected);
    }

    #[test]
    fn empty_and_too_long_exports_are_refused() {
        let mut project = Project::empty();
        assert!(matches!(span(&project, &request(48_000, 0.0)), Err(e) if e.is(export::NOTHING_TO_EXPORT)));
        project.clips.push(clip(1, 0, 960));
        // 8,000,000 ticks at 120 BPM is over an hour.
        let too_long = ExportRequest { range: ExportRange::Ticks { from_tick: 0, to_tick: 8_000_000 }, ..request(48_000, 0.0) };
        assert!(matches!(span(&project, &too_long), Err(e) if e.is(export::EXPORT_TOO_LONG)));
    }

    #[test]
    fn the_tail_follows_the_same_rule_as_the_bounce_tail() {
        let mut project = Project::empty();
        project.clips.push(clip(1, 0, 960));
        assert!(span(&project, &request(48_000, MAX_TAIL_SECONDS)).is_ok());
        // A huge tail used to overflow the frame count instead of being refused.
        for tail in [-1.0, MAX_TAIL_SECONDS + 1.0, 1e30, f32::NAN] {
            assert!(matches!(span(&project, &request(48_000, tail)), Err(e) if e.is(export::TAIL_OUT_OF_RANGE)), "{tail}");
        }
    }

    /// A range and the loop: the start, the length by the tempo map (60 BPM is a beat per second) and clips cut at the
    /// range end.
    #[test]
    fn a_range_or_the_loop_plays_only_its_part() {
        let mut project = Project::empty();
        project.tempo_map = TempoMap::constant(Bpm::new(60.0).unwrap(), TimeSignature::FOUR_FOUR);
        project.clips.push(Clip { fade_out_ticks: 480, ..clip(1, 0, 3840) });
        project.clips.push(clip(2, 1920, 960));
        project.clips.push(clip(3, 2880, 960));
        let ranged = ExportRequest { range: ExportRange::Ticks { from_tick: 960, to_tick: 2880 }, ..request(48_000, 0.5) };
        let span = span(&project, &ranged).unwrap();
        assert_eq!((span.from_tick, span.frames), (960, 2 * 48_000 + 24_000));
        let lengths: Vec<_> = span.project.clips.iter().map(|c| (c.id, c.length_ticks, c.fade_out_ticks)).collect();
        assert_eq!(lengths, [(1, 2880, 0), (2, 960, 0)], "clip 3 starts at the range end and is silent");

        assert!(matches!(
            super::span(&project, &ExportRequest { range: ExportRange::Loop, ..request(48_000, 0.0) }),
            Err(e) if e.is(export::NO_LOOP_REGION)
        ));
        project.loop_region = Some((0, 1920));
        let looped = super::span(&project, &ExportRequest { range: ExportRange::Loop, ..request(48_000, 0.0) }).unwrap();
        assert_eq!((looped.from_tick, looped.frames), (0, 2 * 48_000));
        let backwards = ExportRequest { range: ExportRange::Ticks { from_tick: 960, to_tick: 960 }, ..request(48_000, 0.0) };
        assert!(matches!(super::span(&project, &backwards), Err(e) if e.is(export::RANGE_BACKWARDS)));
    }
}
