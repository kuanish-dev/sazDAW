//! `PluginClock`: "what to tell the plugin about time" — the `ProcessContext` of each `process` call from the engine
//! segment time (`BlockTiming`). Without it, tempo-synced plugins (arpeggiators, synced delays and LFOs) do not know the tempo.
//!
//! Pure code without allocations: fit for the audio thread. The position is the transport (sources) position, not shifted
//! by latency compensation (PDC), same as notes; an instrument may play a segment in pieces — the position advances by what was played.
//! Time in samples is computed from ticks at the current tempo (there are no tempo changes inside the project).

use daw_contracts::{BlockTiming, PPQ};
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_::{
    kBarPositionValid, kContTimeValid, kCycleActive, kCycleValid, kPlaying, kProjectTimeMusicValid, kTempoValid, kTimeSigValid,
};
use vst3::Steinberg::Vst::{Chord, FrameRate, ProcessContext};

#[derive(Default)]
pub struct PluginClock {
    /// Time of the current segment; `None` when the engine did not report it (e.g. the plugin plays outside the engine).
    timing: Option<BlockTiming>,
    /// Frames of the segment already played.
    offset: usize,
    sample_rate: f64,
    /// Frames through the plugin since start (`continousTimeSamples`): runs even while the transport is stopped.
    continuous: i64,
}

/// An empty context: its memory is allocated once, afterwards only the numbers are rewritten.
pub fn empty_context() -> ProcessContext {
    ProcessContext {
        state: 0,
        sampleRate: 0.0,
        projectTimeSamples: 0,
        systemTime: 0,
        continousTimeSamples: 0,
        projectTimeMusic: 0.0,
        barPositionMusic: 0.0,
        cycleStartMusic: 0.0,
        cycleEndMusic: 0.0,
        tempo: 0.0,
        timeSigNumerator: 0,
        timeSigDenominator: 0,
        chord: Chord { keyNote: 0, rootNote: 0, chordMask: 0 },
        smpteOffsetSubframes: 0,
        frameRate: FrameRate { framesPerSecond: 0, flags: 0 },
        samplesToNextClock: 0,
    }
}

fn quarters(tick: f64) -> f64 {
    tick / f64::from(PPQ)
}

impl PluginClock {
    /// Starting the plugin at `sample_rate`: continuous time starts over.
    pub fn start(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.continuous = 0;
        self.offset = 0;
    }

    /// Time of a new segment (`Instrument::set_timing` / `Effect::set_timing`).
    pub fn set_timing(&mut self, timing: &BlockTiming) {
        self.timing = Some(*timing);
        self.offset = 0;
    }

    /// Fills `context` for the next `frames` frames and advances the position by them.
    pub fn fill(&mut self, context: &mut ProcessContext, frames: usize) {
        *context = empty_context();
        context.sampleRate = self.sample_rate;
        context.continousTimeSamples = self.continuous;
        context.state = kContTimeValid;
        self.continuous = self.continuous.saturating_add(frames as i64);
        let Some(timing) = self.timing else { return };
        let moved = if timing.playing { self.offset as f64 } else { 0.0 };
        let tick = timing.start_tick + moved * timing.ticks_per_frame;
        context.projectTimeMusic = quarters(tick);
        context.barPositionMusic = quarters(timing.bar_start(tick));
        context.projectTimeSamples = if timing.ticks_per_frame > 0.0 { (tick / timing.ticks_per_frame).round() as i64 } else { 0 };
        context.tempo = timing.tempo.get();
        context.timeSigNumerator = i32::from(timing.time_signature.numerator());
        context.timeSigDenominator = i32::from(timing.time_signature.denominator());
        context.state |= kTempoValid | kTimeSigValid | kProjectTimeMusicValid | kBarPositionValid;
        if timing.playing {
            context.state |= kPlaying;
        }
        if let Some((start, end)) = timing.loop_region {
            context.cycleStartMusic = quarters(start as f64);
            context.cycleEndMusic = quarters(end as f64);
            context.state |= kCycleActive | kCycleValid;
        }
        self.offset += frames;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{Bpm, TimeSignature};

    const SR: u32 = 48_000;

    fn timing(start_tick: f64, playing: bool) -> BlockTiming {
        let tempo = Bpm::new(120.0).unwrap();
        BlockTiming {
            frames: 512,
            playing,
            start_tick,
            ticks_per_frame: tempo.ticks_per_frame(SR),
            tempo,
            time_signature: TimeSignature::new(3, 4).unwrap(),
            signature_start: 0.0,
            loop_region: None,
        }
    }

    #[test]
    fn bar_position_counts_from_the_signature_change() {
        let mut clock = PluginClock::default();
        clock.start(f64::from(SR));
        let after_change = BlockTiming { time_signature: TimeSignature::FOUR_FOUR, signature_start: 2880.0, ..timing(7000.0, true) };
        clock.set_timing(&after_change);
        let mut context = empty_context();
        clock.fill(&mut context, 64);
        assert_eq!(context.barPositionMusic, 7.0);
        assert_eq!((context.timeSigNumerator, context.timeSigDenominator), (4, 4));
    }

    fn has(context: &ProcessContext, flag: u32) -> bool {
        context.state & flag != 0
    }

    #[test]
    fn playing_segment_gives_tempo_signature_position_and_bar() {
        let mut clock = PluginClock::default();
        clock.start(f64::from(SR));
        clock.set_timing(&timing(4000.0, true));
        let mut context = empty_context();
        clock.fill(&mut context, 256);
        assert_eq!(context.tempo, 120.0);
        assert_eq!((context.timeSigNumerator, context.timeSigDenominator), (3, 4));
        assert!((context.projectTimeMusic - 4000.0 / 960.0).abs() < 1e-9);
        assert_eq!(context.barPositionMusic, 3.0);
        assert_eq!(context.projectTimeSamples, 100_000, "120 BPM, 48 kHz: 25 frames per tick");
        assert_eq!(context.sampleRate, 48_000.0);
        for flag in [kPlaying, kTempoValid, kTimeSigValid, kProjectTimeMusicValid, kBarPositionValid, kContTimeValid] {
            assert!(has(&context, flag), "flag {flag}");
        }
        assert!(!has(&context, kCycleActive), "no cycle");
    }

    #[test]
    fn pieces_of_one_segment_continue_the_position() {
        let mut clock = PluginClock::default();
        clock.start(f64::from(SR));
        clock.set_timing(&timing(0.0, true));
        let mut context = empty_context();
        clock.fill(&mut context, 100);
        clock.fill(&mut context, 50);
        assert_eq!(context.projectTimeSamples, 100, "the second piece starts after the first");
        assert_eq!(context.continousTimeSamples, 100);
        clock.set_timing(&timing(0.0, true));
        clock.fill(&mut context, 10);
        assert_eq!(context.projectTimeSamples, 0, "a new segment starts from its beginning");
        assert_eq!(context.continousTimeSamples, 150, "continuous time is not reset");
    }

    #[test]
    fn stopped_transport_stands_still_and_says_so() {
        let mut clock = PluginClock::default();
        clock.start(f64::from(SR));
        clock.set_timing(&timing(960.0, false));
        let mut context = empty_context();
        clock.fill(&mut context, 256);
        clock.fill(&mut context, 256);
        assert!(!has(&context, kPlaying));
        assert!(has(&context, kTempoValid));
        assert_eq!(context.projectTimeMusic, 1.0);
    }

    #[test]
    fn loop_region_is_the_active_cycle() {
        let mut clock = PluginClock::default();
        clock.start(f64::from(SR));
        clock.set_timing(&BlockTiming { loop_region: Some((1920, 5760)), ..timing(2000.0, true) });
        let mut context = empty_context();
        clock.fill(&mut context, 64);
        assert!(has(&context, kCycleActive) && has(&context, kCycleValid));
        assert_eq!((context.cycleStartMusic, context.cycleEndMusic), (2.0, 6.0));
    }

    #[test]
    fn without_engine_time_only_continuous_time_is_valid() {
        let mut clock = PluginClock::default();
        clock.start(44_100.0);
        let mut context = empty_context();
        clock.fill(&mut context, 32);
        assert_eq!(context.state, kContTimeValid);
        assert_eq!(context.sampleRate, 44_100.0);
    }
}
