//! Previewing a part: notes rendered offline into a temporary buffer with the part's instruments, the tail folded onto
//! the loop start (so the seam is inaudible) and the buffer looped on the engine's existing preview voice. Shared by the
//! drum and the chord part previews.

use std::sync::Arc;

use daw_contracts::{
    AudioBlock, Bpm, ContractError, DspCommand, DspSink, Instrument, InstrumentCatalog, InstrumentSpec, SampleData, StreamConfig,
};

/// Sample rate of the temporary buffer; the engine converts it to the device rate itself.
const RATE: u32 = 48_000;
/// How many frames an instrument renders at a time.
const CHUNK: usize = 512;
/// The tail after the last hit (a crash rings up to 1.6 s) that is folded onto the loop start.
const TAIL_SECONDS: f64 = 1.5;
/// No need to listen longer: a preview checks the rhythm, not the whole song.
pub(crate) const MAX_PREVIEW_BARS: u32 = 16;
/// The buffer is never louder than this: a too loud one is scaled down so it does not crackle.
const CEILING: f32 = 0.95;

pub(crate) struct Event {
    pub(crate) frame: usize,
    pub(crate) on: bool,
    pub(crate) pitch: u8,
    pub(crate) velocity: f32,
}

/// One instrument and its notes.
pub(crate) struct Voice {
    pub(crate) sound: InstrumentSpec,
    pub(crate) events: Vec<Event>,
}

/// Frames per tick of the preview buffer at `tempo` (a part's preview has no tempo changes inside).
pub(crate) fn frames_per_tick(tempo: Bpm) -> f64 {
    1.0 / tempo.ticks_per_frame(RATE)
}

pub(crate) fn frame_at(tick: u32, frames_per_tick: f64) -> usize {
    (f64::from(tick) * frames_per_tick).round() as usize
}

/// Renders frames `[from, to)` of the stereo buffer in chunks: instruments need not handle large blocks.
fn render_range(instrument: &mut dyn Instrument, buffer: &mut [f32], from: usize, to: usize) {
    let mut cursor = from;
    while cursor < to {
        let end = (cursor + CHUNK).min(to);
        instrument.render(&mut AudioBlock::new(&mut buffer[cursor * 2..end * 2], 2));
        cursor = end;
    }
}

/// Plays one instrument's schedule over the whole buffer; instruments add their sound, so voices simply layer.
fn render_voice(instrument: &mut dyn Instrument, events: &mut [Event], buffer: &mut [f32]) {
    let total = buffer.len() / 2;
    // On one frame the release goes first, then the new press.
    events.sort_by_key(|e| (e.frame, e.on));
    let mut cursor = 0;
    for event in events.iter() {
        let frame = event.frame.min(total);
        render_range(instrument, buffer, cursor, frame);
        cursor = frame;
        if event.on {
            instrument.note_on(event.pitch, event.velocity);
        } else {
            instrument.note_off(event.pitch);
        }
    }
    render_range(instrument, buffer, cursor, total);
}

/// The tail past the loop end is added to its start and the buffer is cut to the loop; a too loud buffer is scaled down.
fn close_loop(mut buffer: Vec<f32>, loop_frames: usize) -> Vec<f32> {
    let tail = buffer.len() / 2 - loop_frames;
    for i in 0..(tail.min(loop_frames) * 2) {
        buffer[i] += buffer[loop_frames * 2 + i];
    }
    buffer.truncate(loop_frames * 2);
    let peak = buffer.iter().fold(0.0_f32, |a, v| a.max(v.abs()));
    if peak > CEILING {
        let scale = CEILING / peak;
        for value in &mut buffer {
            *value *= scale;
        }
    }
    buffer
}

/// Renders the voices into a buffer `loop_frames` long (their tails folded onto its start) and loops it on the
/// engine's preview voice.
pub(crate) fn play_loop(
    instruments: &dyn InstrumentCatalog,
    dsp: &dyn DspSink,
    voices: &mut [Voice],
    loop_frames: usize,
) -> Result<(), ContractError> {
    let tail = (f64::from(RATE) * TAIL_SECONDS) as usize;
    let mut buffer = vec![0.0_f32; (loop_frames + tail) * 2];
    for voice in voices {
        let mut instrument = instruments.create(&voice.sound.kind, &voice.sound.params, voice.sound.state.as_deref())?;
        instrument.prepare(StreamConfig { sample_rate: RATE, ..StreamConfig::DEFAULT });
        render_voice(&mut *instrument, &mut voice.events, &mut buffer);
    }
    let frames = close_loop(buffer, loop_frames);
    let count = frames.len() / 2;
    let data = Arc::new(SampleData { sample_rate: RATE, channels: 2, frames });
    dsp.send(DspCommand::PreviewPlay { data, from: 0, to: count, looping: true })
}

pub(crate) fn stop(dsp: &dyn DspSink) {
    // The engine may have stopped together with the app: then there is nothing to stop.
    let _ = dsp.send(DspCommand::PreviewStop);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A "counter" instrument: while a note is held it writes its pitch/100 into the buffer.
    struct Held {
        pitch: Option<u8>,
    }

    impl Instrument for Held {
        fn prepare(&mut self, _: StreamConfig) {}
        fn note_on(&mut self, note: u8, _: f32) {
            self.pitch = Some(note);
        }
        fn note_off(&mut self, _: u8) {
            self.pitch = None;
        }
        fn all_notes_off(&mut self) {
            self.pitch = None;
        }
        fn controller(&mut self, _: daw_contracts::ControllerKind, _: f32) {}
        fn set_param(&mut self, _: &str, _: f32) {}
        fn render(&mut self, block: &mut AudioBlock) {
            for frame in 0..block.frames() {
                block.add_to_frame(frame, self.pitch.map_or(0.0, |p| f32::from(p) / 100.0));
            }
        }
    }

    fn event(frame: usize, on: bool, pitch: u8) -> Event {
        Event { frame, on, pitch, velocity: 1.0 }
    }

    #[test]
    fn events_fire_exactly_at_their_frame_even_across_chunk_edges() {
        let mut buffer = vec![0.0_f32; 2_000 * 2];
        // Note 50 from frame 700 to 1100 (across the chunk edge at 1024), then note 30 from frame 1100 with no gap.
        let mut events = vec![event(1100, true, 30), event(700, true, 50), event(1100, false, 50), event(1500, false, 30)];
        render_voice(&mut Held { pitch: None }, &mut events, &mut buffer);
        let at = |frame: usize| buffer[frame * 2];
        assert_eq!((at(699), at(700), at(1099)), (0.0, 0.5, 0.5));
        assert_eq!((at(1100), at(1499), at(1500)), (0.3, 0.3, 0.0), "at frame 1100 the old note is released before the new one starts");
    }

    #[test]
    fn the_tail_after_the_loop_is_folded_onto_its_start_and_loud_buffers_are_scaled_down() {
        // A 4-frame loop with a 2-frame tail: frames 4 and 5 are added to 0 and 1.
        let mut buffer = vec![0.0_f32; 6 * 2];
        buffer[0] = 0.25;
        buffer[8] = 0.5;
        buffer[10] = 0.25;
        let folded = close_loop(buffer, 4);
        assert_eq!(folded.len(), 8);
        assert_eq!((folded[0], folded[2]), (0.75, 0.25));
        let loud = close_loop(vec![2.0, 2.0, 0.0, 0.0], 2);
        assert!((loud[0] - CEILING).abs() < 1e-6);
    }

    #[test]
    fn frames_follow_the_tempo() {
        let frames_per_tick = frames_per_tick(Bpm::new(120.0).unwrap());
        assert_eq!(frame_at(daw_contracts::BAR_TICKS, frames_per_tick), 96_000, "a bar at 120 BPM = 2 s");
    }
}
