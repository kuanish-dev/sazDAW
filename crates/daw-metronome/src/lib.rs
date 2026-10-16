//! Metronome: a short sine click on every beat, accented on the first beat of a bar. Beats and bars count from the
//! segment's signature change (`BlockTiming::signature_start`): the engine cuts segments at signature changes, so after
//! "3/4 → 4/4" the accent follows the new bar right away. The switch starts as the user left it in the last launch and
//! asks `MetronomeSettingsStore` to remember every change (off on the first launch).

use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AudioBlock, AudioProcessor, BlockTiming, LogEntry, MetronomeControl, MetronomeSettings, MetronomeSettingsStore, StreamConfig,
};

const ACCENT_HZ: f32 = 1_500.0;
const BEAT_HZ: f32 = 1_000.0;
const CLICK_SECONDS: f32 = 0.04;
const DECAY_SECONDS: f32 = 0.008;
const GAIN: f32 = 0.4;

struct Control {
    enabled: Arc<AtomicBool>,
    store: Arc<dyn MetronomeSettingsStore>,
    log: Arc<dyn AppLog>,
    /// Switches from two threads (the window and the AI server) take turns, so the file ends up with the last one.
    switching: Mutex<()>,
}

impl MetronomeControl for Control {
    fn set_enabled(&self, enabled: bool) {
        let _turn = lock(&self.switching);
        self.enabled.store(enabled, Ordering::Release);
        if let Err(error) = self.store.save(&MetronomeSettings { enabled }) {
            self.log.record(LogEntry::warn("metronome", format!("the metronome switch was not remembered: {error}")));
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy)]
struct Click {
    phase: f32,
    step: f32,
    age: u32,
}

struct Metronome {
    enabled: Arc<AtomicBool>,
    sample_rate: f32,
    click: Option<Click>,
}

impl Metronome {
    fn start_click(&mut self, accent: bool) {
        let hz = if accent { ACCENT_HZ } else { BEAT_HZ };
        self.click = Some(Click { phase: 0.0, step: TAU * hz / self.sample_rate, age: 0 });
    }

    fn next_sample(&mut self) -> f32 {
        let Some(click) = self.click.as_mut() else {
            return 0.0;
        };
        let seconds = click.age as f32 / self.sample_rate;
        if seconds >= CLICK_SECONDS {
            self.click = None;
            return 0.0;
        }
        let value = click.phase.sin() * (-seconds / DECAY_SECONDS).exp() * GAIN;
        click.phase = (click.phase + click.step) % TAU;
        click.age += 1;
        value
    }
}

impl AudioProcessor for Metronome {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.click = None;
    }

    fn process(&mut self, block: &mut AudioBlock, timing: &BlockTiming) {
        let frames = block.frames();
        let triggering = timing.playing && self.enabled.load(Ordering::Acquire);

        let ticks_per_beat = timing.time_signature.ticks_per_beat();
        let beats_per_bar = i64::from(timing.time_signature.numerator());
        let origin = timing.signature_start;
        // A beat sounds from frame `BlockTiming::frame_at`, the same frame as the engine's notes, so the click matches
        // a note on the beat. A segment takes the beats whose frame falls into it; a beat between the last frame of the
        // previous segment and the start of this one (its `frame_at` there would be past the end) sounds here from
        // frame 0, just like a note.
        let after_previous = timing.start_tick - timing.ticks_per_frame * (1.0 - 1e-6);
        // Beats count from the signature start; there are no negative ticks: the engine with latency compensation
        // gives the metronome the segments "before zero" (see `daw-engine`).
        let first_beat = (-origin / ticks_per_beat).ceil();
        let mut beat = (((after_previous - origin) / ticks_per_beat).floor() + 1.0).max(first_beat);

        for frame in 0..frames {
            while triggering && timing.frame_at(origin + beat * ticks_per_beat) <= frame {
                self.start_click((beat as i64).rem_euclid(beats_per_bar) == 0);
                beat += 1.0;
            }
            let sample = self.next_sample();
            if sample != 0.0 {
                block.add_to_frame(frame, sample);
            }
        }
    }
}

/// Creates a pair: the control for the UI and the processor for the audio engine. The switch starts as `store` remembers
/// it.
pub fn create_metronome(
    store: Arc<dyn MetronomeSettingsStore>,
    log: Arc<dyn AppLog>,
) -> (Arc<dyn MetronomeControl>, Box<dyn AudioProcessor>) {
    let enabled = Arc::new(AtomicBool::new(store.load().enabled));
    let control = Arc::new(Control { enabled: Arc::clone(&enabled), store, log, switching: Mutex::new(()) });
    let processor = Box::new(Metronome { enabled, sample_rate: StreamConfig::DEFAULT.sample_rate as f32, click: None });
    (control, processor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::codes::settings;
    use daw_contracts::{Bpm, ContractError, TimeSignature};
    use std::path::PathBuf;

    /// Remembers the settings in memory; `fails` makes every save fail.
    #[derive(Default)]
    struct MemoryStore {
        saved: Mutex<MetronomeSettings>,
        fails: bool,
    }

    impl MetronomeSettingsStore for MemoryStore {
        fn load(&self) -> MetronomeSettings {
            *self.saved.lock().unwrap()
        }
        fn save(&self, settings: &MetronomeSettings) -> Result<(), ContractError> {
            if self.fails {
                return Err(settings::STATE_NOT_WRITTEN.with("path", "metronome.json").with("details", "disk full"));
            }
            *self.saved.lock().unwrap() = *settings;
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryLog(Mutex<Vec<LogEntry>>);

    impl AppLog for MemoryLog {
        fn record(&self, entry: LogEntry) {
            self.0.lock().unwrap().push(entry);
        }
        fn recent(&self, limit: usize) -> Vec<LogEntry> {
            self.0.lock().unwrap().iter().rev().take(limit).cloned().collect()
        }
        fn file(&self) -> Option<PathBuf> {
            None
        }
    }

    const SR: u32 = 48_000;
    const TICKS_PER_FRAME: f64 = 1920.0 / SR as f64;

    fn timing(start_tick: f64, playing: bool) -> BlockTiming {
        // 120 BPM: 2 beats per second → 1920 ticks per second.
        BlockTiming {
            frames: 0,
            playing,
            start_tick,
            ticks_per_frame: TICKS_PER_FRAME,
            tempo: Bpm::new(120.0).unwrap(),
            time_signature: TimeSignature::FOUR_FOUR,
            signature_start: 0.0,
            loop_region: None,
        }
    }

    fn render(processor: &mut dyn AudioProcessor, timing: &BlockTiming, frames: usize) -> Vec<f32> {
        let timing = &BlockTiming { frames, ..*timing };
        let mut buffer = vec![0.0; frames * 2];
        let mut block = AudioBlock::new(&mut buffer, 2);
        processor.process(&mut block, timing);
        buffer
    }

    fn first_nonzero_frame(buffer: &[f32]) -> Option<usize> {
        buffer.chunks(2).position(|f| f[0] != 0.0)
    }

    /// A metronome that is on, ready to play at `SR`.
    fn prepared() -> (Arc<dyn MetronomeControl>, Box<dyn AudioProcessor>) {
        let (control, mut processor) = create_metronome(Arc::new(MemoryStore::default()), Arc::new(MemoryLog::default()));
        control.set_enabled(true);
        processor.prepare(StreamConfig { sample_rate: SR, channels: 2 });
        (control, processor)
    }

    #[test]
    fn the_switch_starts_as_remembered_off_at_first_and_remembers_every_change() {
        let store = Arc::new(MemoryStore::default());
        let (control, _) = create_metronome(store.clone(), Arc::new(MemoryLog::default()));
        assert!(!control.is_enabled(), "off on the first launch");
        control.set_enabled(true);
        assert_eq!(store.load(), MetronomeSettings { enabled: true });
        let (next_launch, _) = create_metronome(store.clone(), Arc::new(MemoryLog::default()));
        assert!(next_launch.is_enabled(), "the next launch starts as the user left it");
        next_launch.set_enabled(false);
        assert_eq!(store.load(), MetronomeSettings { enabled: false });
    }

    #[test]
    fn a_failed_save_keeps_the_switch_and_is_logged() {
        let log = Arc::new(MemoryLog::default());
        let (control, _) = create_metronome(Arc::new(MemoryStore { fails: true, ..MemoryStore::default() }), log.clone());
        control.set_enabled(true);
        assert!(control.is_enabled(), "the click follows the user now");
        let entries = log.recent(10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "metronome");
    }

    #[test]
    fn click_starts_at_beat_position_inside_block() {
        let (_c, mut m) = prepared();
        // The next beat is at tick 960, exactly 100 frames away.
        let start = 960.0 - 100.0 * TICKS_PER_FRAME;
        let out = render(m.as_mut(), &timing(start, true), 512);
        let frame = first_nonzero_frame(&out).expect("click expected");
        assert!((i64::try_from(frame).unwrap() - 100).abs() <= 1, "frame = {frame}");
    }

    /// A beat between frames: the click sounds from the first frame not before it, where the engine starts a note.
    #[test]
    fn click_starts_on_the_same_frame_as_a_note_on_the_beat() {
        let (_c, mut m) = prepared();
        let start = 960.0 - 100.5 * TICKS_PER_FRAME;
        let timing = BlockTiming { frames: 512, ..timing(start, true) };
        let out = render(m.as_mut(), &timing, 512);
        assert_eq!(timing.frame_at(960.0), 101);
        // The first click sample is sin(0) = 0; it becomes audible from the next frame.
        assert_eq!(first_nonzero_frame(&out), Some(102));
    }

    #[test]
    fn silent_when_stopped_or_disabled() {
        let (control, mut m) = prepared();
        let out = render(m.as_mut(), &timing(0.0, false), 512);
        assert!(first_nonzero_frame(&out).is_none());

        control.set_enabled(false);
        let out = render(m.as_mut(), &timing(0.0, true), 512);
        assert!(first_nonzero_frame(&out).is_none());
    }

    #[test]
    fn beats_are_not_doubled_across_blocks() {
        let (_c, mut m) = prepared();
        let frames = 480;
        let mut start = 0.0;
        let mut samples = Vec::new();
        for _ in 0..200 {
            // 200 blocks of 10 ms = 2 s = 4 beats at 120 BPM.
            let t = timing(start, true);
            samples.extend(render(m.as_mut(), &t, frames).chunks(2).map(|f| f[0]));
            start = BlockTiming { frames, ..t }.end_tick();
        }
        // A click is a continuous stretch of sound; there is long silence between clicks.
        let mut clicks = 0;
        let mut silence = usize::MAX;
        for sample in samples {
            if sample == 0.0 {
                silence = silence.saturating_add(1);
            } else {
                if silence >= 100 {
                    clicks += 1;
                }
                silence = 0;
            }
        }
        assert_eq!(clicks, 4);
    }

    /// The frequency of a click that started in the buffer (by zero crossings over the click length).
    fn click_crossings(buffer: &[f32]) -> usize {
        let samples: Vec<f32> = buffer.chunks(2).map(|f| f[0]).collect();
        samples.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
    }

    #[test]
    fn accents_follow_a_signature_change() {
        // A 3/4 bar from tick 0 to 2880, then 4/4: accents on beats 0 and 3 (2880), then 4 beats later (6720).
        let three_four = BlockTiming { time_signature: TimeSignature::new(3, 4).unwrap(), ..timing(0.0, true) };
        let four_four = BlockTiming { signature_start: 2880.0, ..timing(2880.0, true) };
        let accents = |segment: BlockTiming, beats: &[f64]| -> Vec<bool> {
            beats
                .iter()
                .map(|&tick| {
                    let (_c, mut m) = prepared();
                    let at = BlockTiming { start_tick: tick, ..segment };
                    let accent_crossings = click_crossings(&render(prepared().1.as_mut(), &timing(0.0, true), 256));
                    click_crossings(&render(m.as_mut(), &at, 256)) == accent_crossings
                })
                .collect()
        };
        assert_eq!(accents(three_four, &[0.0, 960.0, 1920.0]), [true, false, false]);
        assert_eq!(accents(four_four, &[2880.0, 3840.0, 4800.0, 5760.0, 6720.0]), [true, false, false, false, true]);
    }

    #[test]
    fn accent_is_higher_pitch_than_regular_beat() {
        let (_c, mut m) = prepared();
        let accent = render(m.as_mut(), &timing(0.0, true), 256);
        let (_c2, mut m2) = prepared();
        let beat = render(m2.as_mut(), &timing(960.0, true), 256);
        let crossings = |b: &[f32]| b.chunks(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!(crossings(&accent) > crossings(&beat));
    }
}
