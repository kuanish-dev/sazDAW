//! The transport implementation: the control side and the audio thread side talk through atomics, without locks. The
//! clock takes the tempo from the tempo map the engine passes to every `advance` (it travels in the project snapshot).
//! It wraps around the loop region, or around the whole song while the song loop is on; both are one rule,
//! `Shared::wrap_region`, read by the clock and by the control side alike.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use daw_contracts::{BlockTiming, RAMP_SEGMENT_FRAMES, TempoMap, TransportClock, TransportControl, TransportState};

const NO_SEEK: u64 = u64::MAX;
/// No cue set; a cue tick is clamped below it, like a seek.
const NO_CUE: u64 = u64::MAX;
/// How close (in ticks) a segment end must come to a map point to snap exactly onto it.
const SNAP_TICKS: f64 = 1e-3;
/// The loop is off. The loop end is always greater than its start, so a packed loop is never zero.
const NO_LOOP: u64 = 0;
/// No song: an empty arrangement has nothing to loop.
const NO_SONG: u64 = 0;

struct Shared {
    state: AtomicU8,
    seek: AtomicU64,
    position: AtomicU64,
    /// The start position `play` (from pause/stop) and `stop` return to; only the control side reads it.
    cue: AtomicU64,
    /// Loop start and end in one atomic (`start << 32 | end`): the audio thread reads them in one load and never sees
    /// the start of one loop with the end of another.
    loop_region: AtomicU64,
    /// The song loop mode (on by default).
    song_loop: AtomicBool,
    /// Where the song loop jumps back to 0; `NO_SONG` when there is nothing to loop.
    song_end: AtomicU64,
    /// Recordings that asked playback to run past the song end; the song loop waits while any is left.
    song_loop_holds: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(encode_state(TransportState::Stopped)),
            seek: AtomicU64::new(NO_SEEK),
            position: AtomicU64::new(0),
            cue: AtomicU64::new(NO_CUE),
            loop_region: AtomicU64::new(NO_LOOP),
            song_loop: AtomicBool::new(true),
            song_end: AtomicU64::new(NO_SONG),
            song_loop_holds: AtomicU32::new(0),
        }
    }

    fn state(&self) -> TransportState {
        decode_state(self.state.load(Ordering::Acquire))
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(encode_state(state), Ordering::Release);
    }

    fn loop_region(&self) -> Option<(u64, u64)> {
        unpack_loop(self.loop_region.load(Ordering::Acquire))
    }

    /// The loop region wins; otherwise the whole song, while the song loop is on, not held, and there is a song.
    fn wrap_region(&self) -> Option<(u64, u64)> {
        self.loop_region().or_else(|| {
            let end = self.song_end.load(Ordering::Acquire);
            let on = self.song_loop.load(Ordering::Acquire) && self.song_loop_holds.load(Ordering::Acquire) == 0;
            (on && end != NO_SONG).then_some((0, end))
        })
    }
}

/// Loop ticks are stored in 32 bits: over 4 million quarter notes, further than any project goes.
fn pack_loop(region: Option<(u64, u64)>) -> u64 {
    let clamp = |tick: u64| tick.min(u64::from(u32::MAX));
    match region {
        Some((start, end)) if clamp(end) > clamp(start) => clamp(start) << 32 | clamp(end),
        _ => NO_LOOP,
    }
}

fn unpack_loop(bits: u64) -> Option<(u64, u64)> {
    (bits != NO_LOOP).then_some((bits >> 32, bits & u64::from(u32::MAX)))
}

fn encode_state(state: TransportState) -> u8 {
    match state {
        TransportState::Stopped => 0,
        TransportState::Playing => 1,
        TransportState::Paused => 2,
    }
}

fn decode_state(raw: u8) -> TransportState {
    match raw {
        1 => TransportState::Playing,
        2 => TransportState::Paused,
        _ => TransportState::Stopped,
    }
}

struct Control {
    shared: Arc<Shared>,
}

impl TransportControl for Control {
    fn play(&self) {
        if self.shared.state() != TransportState::Playing
            && let Some(cue) = self.cue()
        {
            self.seek(cue);
        }
        self.shared.set_state(TransportState::Playing);
    }

    fn pause(&self) {
        self.shared.set_state(TransportState::Paused);
    }

    fn stop(&self) {
        self.seek(self.cue().unwrap_or(0));
        self.shared.set_state(TransportState::Stopped);
    }

    fn seek(&self, tick: u64) {
        self.shared.seek.store(tick.min(NO_SEEK - 1), Ordering::Release);
    }

    fn set_cue(&self, tick: Option<u64>) {
        self.shared.cue.store(tick.map_or(NO_CUE, |tick| tick.min(NO_CUE - 1)), Ordering::Release);
    }

    fn cue(&self) -> Option<u64> {
        let cue = self.shared.cue.load(Ordering::Acquire);
        (cue != NO_CUE).then_some(cue)
    }

    fn set_loop(&self, region: Option<(u64, u64)>) {
        self.shared.loop_region.store(pack_loop(region), Ordering::Release);
    }

    fn loop_region(&self) -> Option<(u64, u64)> {
        self.shared.loop_region()
    }

    fn set_song_loop(&self, on: bool) {
        self.shared.song_loop.store(on, Ordering::Release);
    }

    fn song_loop(&self) -> bool {
        self.shared.song_loop.load(Ordering::Acquire)
    }

    fn set_song_end(&self, end: Option<u64>) {
        let end = end.map_or(NO_SONG, |end| end.min(u64::from(u32::MAX)));
        self.shared.song_end.store(end, Ordering::Release);
    }

    fn hold_song_loop(&self, hold: bool) {
        let holds = &self.shared.song_loop_holds;
        if hold {
            holds.fetch_add(1, Ordering::AcqRel);
        } else {
            let _ = holds.try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1));
        }
    }

    fn wrap_region(&self) -> Option<(u64, u64)> {
        self.shared.wrap_region()
    }

    fn state(&self) -> TransportState {
        self.shared.state()
    }

    fn position_ticks(&self) -> u64 {
        self.shared.position.load(Ordering::Acquire)
    }
}

struct Clock {
    shared: Arc<Shared>,
    position: f64,
}

impl TransportClock for Clock {
    fn advance(&mut self, max_frames: usize, sample_rate: u32, map: &TempoMap) -> BlockTiming {
        let seek = self.shared.seek.swap(NO_SEEK, Ordering::AcqRel);
        if seek != NO_SEEK {
            self.position = seek as f64;
        }

        let playing = self.shared.state() == TransportState::Playing;
        let tempo = map.tempo_at(self.position);
        let signature = map.signature_at(self.position);
        let estimate = tempo.ticks_per_frame(sample_rate);
        let frames_to = |tick: f64| ((tick - self.position) / estimate - 1e-6).ceil().max(1.0) as usize;

        let mut frames = max_frames;
        if map.in_ramp(self.position) {
            frames = frames.min(RAMP_SEGMENT_FRAMES);
        }
        let change = map.next_change(self.position);
        if let Some(change) = change {
            frames = frames.min(frames_to(change));
        }
        let mut wrap_to = None;
        let loop_region = self.shared.wrap_region().filter(|_| playing);
        if let Some((start, end)) = loop_region {
            let end = end as f64;
            if self.position < end && frames_to(end) <= frames {
                frames = frames_to(end);
                wrap_to = Some(start as f64);
            }
        }
        let frames = frames.max(1);
        let end = map.tick_after_frames(self.position, frames as f64, sample_rate);
        let end = change.filter(|change| (end - change).abs() < SNAP_TICKS).unwrap_or(end);
        let ticks_per_frame = (end - self.position) / frames as f64;

        let timing = BlockTiming {
            frames,
            playing,
            start_tick: self.position,
            ticks_per_frame,
            tempo,
            time_signature: signature.signature,
            signature_start: f64::from(signature.tick),
            loop_region,
        };

        if playing {
            self.position = wrap_to.unwrap_or(end);
        }
        self.shared.position.store(self.position.max(0.0) as u64, Ordering::Release);
        timing
    }
}

/// Creates a pair: the control for the UI and the clock for the audio engine.
pub fn create_transport() -> (Arc<dyn TransportControl>, Box<dyn TransportClock>) {
    let shared = Arc::new(Shared::new());
    let control = Arc::new(Control { shared: Arc::clone(&shared) });
    let clock = Box::new(Clock { shared, position: 0.0 });
    (control, clock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{Bpm, PPQ, SignatureChange, TempoChange, TimeSignature};

    const SR: u32 = 48_000;

    fn constant(bpm: f64) -> TempoMap {
        TempoMap::constant(Bpm::new(bpm).unwrap(), TimeSignature::FOUR_FOUR)
    }

    fn map(tempos: &[(u32, f64, bool)], signatures: &[(u32, u8, u8)]) -> TempoMap {
        let tempos = tempos.iter().map(|&(tick, bpm, ramp)| TempoChange { tick, bpm: Bpm::new(bpm).unwrap(), ramp }).collect();
        let signatures =
            signatures.iter().map(|&(tick, n, d)| SignatureChange { tick, signature: TimeSignature::new(n, d).unwrap() }).collect();
        TempoMap::new(tempos, signatures).unwrap()
    }

    #[test]
    fn stopped_clock_does_not_move() {
        let (_control, mut clock) = create_transport();
        let map = constant(120.0);
        let timing = clock.advance(512, SR, &map);
        assert_eq!(timing.frames, 512);
        assert!(!timing.playing);
        assert_eq!(timing.start_tick, 0.0);
        assert_eq!(clock.advance(512, SR, &map).start_tick, 0.0);
    }

    #[test]
    fn one_second_at_120_bpm_is_two_beats() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.play();
        for _ in 0..100 {
            clock.advance((SR / 100) as usize, SR, &map);
        }
        let ticks = control.position_ticks() as f64;
        assert!((ticks - 2.0 * f64::from(PPQ)).abs() <= 1.0, "ticks = {ticks}");
    }

    #[test]
    fn pause_keeps_position_and_stop_resets_it() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.play();
        clock.advance(4800, SR, &map);
        let moved = control.position_ticks();
        assert!(moved > 0);

        control.pause();
        clock.advance(4800, SR, &map);
        assert_eq!(control.position_ticks(), moved);
        assert_eq!(control.state(), TransportState::Paused);

        control.stop();
        clock.advance(480, SR, &map);
        assert_eq!(control.position_ticks(), 0);
        assert_eq!(control.state(), TransportState::Stopped);
    }

    #[test]
    fn play_after_pause_starts_from_the_cue() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.set_cue(Some(1920));
        assert_eq!(control.cue(), Some(1920));
        control.play();
        assert_eq!(clock.advance(64, SR, &map).start_tick, 1920.0, "play from stop starts at the cue");
        clock.advance(4800, SR, &map);
        control.pause();
        clock.advance(64, SR, &map);
        assert!(control.position_ticks() > 1920);

        control.play();
        assert_eq!(clock.advance(64, SR, &map).start_tick, 1920.0, "unpausing starts from the cue again");
    }

    #[test]
    fn play_while_playing_does_not_jump_to_the_cue() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.play();
        clock.advance(4800, SR, &map);
        control.set_cue(Some(0));
        let before = control.position_ticks();
        control.play();
        assert!(clock.advance(64, SR, &map).start_tick >= before as f64);
    }

    #[test]
    fn stop_returns_to_the_cue_and_clearing_it_restores_the_old_behaviour() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.set_cue(Some(960));
        control.play();
        clock.advance(4800, SR, &map);
        control.stop();
        clock.advance(64, SR, &map);
        assert_eq!(control.position_ticks(), 960);

        control.set_cue(None);
        assert_eq!(control.cue(), None);
        control.play();
        clock.advance(4800, SR, &map);
        control.pause();
        clock.advance(64, SR, &map);
        let paused = control.position_ticks();
        control.play();
        assert_eq!(clock.advance(64, SR, &map).start_tick, paused as f64, "without a cue pause resumes in place");
        control.stop();
        clock.advance(64, SR, &map);
        assert_eq!(control.position_ticks(), 0, "without a cue stop returns to the start");
    }

    #[test]
    fn setting_the_cue_does_not_move_the_playhead() {
        let (control, mut clock) = create_transport();
        control.seek(480);
        control.set_cue(Some(3840));
        assert_eq!(clock.advance(64, SR, &constant(120.0)).start_tick, 480.0);
        control.set_cue(Some(u64::MAX));
        assert_eq!(control.cue(), Some(u64::MAX - 1), "the largest tick is clamped, never read as no cue");
    }

    #[test]
    fn seek_moves_playhead() {
        let (control, mut clock) = create_transport();
        control.seek(1920);
        assert_eq!(clock.advance(64, SR, &constant(120.0)).start_tick, 1920.0);
        assert_eq!(control.position_ticks(), 1920);
    }

    #[test]
    fn loop_wraps_sample_accurately() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.set_loop(Some((0, 960)));
        control.play();
        let first = clock.advance(30_000, SR, &map);
        assert_eq!(first.frames, 24_000);
        assert_eq!(first.start_tick, 0.0);
        assert_eq!(first.loop_region, Some((0, 960)), "plugins learn about the loop from the segment timing");
        let second = clock.advance(30_000 - first.frames, SR, &map);
        assert_eq!(second.start_tick, 0.0, "after the boundary the playhead is at the loop start");
        assert_eq!(control.loop_region(), Some((0, 960)));
        control.set_loop(None);
        assert_eq!(control.loop_region(), None);
        let free = clock.advance(100_000, SR, &map);
        assert_eq!(free.frames, 100_000);
        assert_eq!(free.loop_region, None);
    }

    #[test]
    fn loop_region_is_packed_whole() {
        let (control, _clock) = create_transport();
        control.set_loop(Some((3840, 7680)));
        assert_eq!(control.loop_region(), Some((3840, 7680)));
        control.set_loop(Some((7680, 3840)));
        assert_eq!(control.loop_region(), None, "an end before the start means no loop");
        control.set_loop(Some((0, u64::MAX)));
        assert_eq!(control.loop_region(), Some((0, u64::from(u32::MAX))));
    }

    #[test]
    fn the_song_loop_jumps_back_to_the_start_on_the_exact_frame() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        assert!(control.song_loop(), "the song loop is on by default");
        control.set_song_end(Some(3840));
        assert_eq!(control.wrap_region(), Some((0, 3840)));
        control.seek(1920);
        control.play();
        let last = clock.advance(60_000, SR, &map);
        assert_eq!(last.frames, 48_000, "the segment ends on the song end");
        assert_eq!(last.loop_region, Some((0, 3840)), "plugins see the song as the cycle");
        let first = clock.advance(512, SR, &map);
        assert_eq!(first.start_tick, 0.0, "playback goes on from the start");
        assert!(first.playing);
    }

    #[test]
    fn the_loop_region_wins_over_the_song_end() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.set_song_end(Some(7680));
        control.set_loop(Some((960, 1920)));
        assert_eq!(control.wrap_region(), Some((960, 1920)));
        control.seek(960);
        control.play();
        assert_eq!(clock.advance(100_000, SR, &map).frames, 24_000, "the loop end comes first");
        assert_eq!(clock.advance(512, SR, &map).start_tick, 960.0);
        control.set_loop(None);
        assert_eq!(control.wrap_region(), Some((0, 7680)), "without the loop region the song loops again");
    }

    #[test]
    fn without_the_song_loop_or_a_song_playback_runs_on() {
        let (control, mut clock) = create_transport();
        let map = constant(120.0);
        control.set_song_end(Some(960));
        control.set_song_loop(false);
        assert!(!control.song_loop());
        assert_eq!(control.wrap_region(), None);
        control.play();
        assert_eq!(clock.advance(100_000, SR, &map).frames, 100_000);
        control.set_song_loop(true);
        control.set_song_end(None);
        assert_eq!(control.wrap_region(), None, "an empty arrangement has nothing to loop");
        assert_eq!(clock.advance(100_000, SR, &map).frames, 100_000);
    }

    #[test]
    fn playback_already_past_the_song_end_runs_on() {
        let (control, mut clock) = create_transport();
        control.set_song_end(Some(960));
        control.seek(3840);
        control.play();
        let timing = clock.advance(100_000, SR, &constant(120.0));
        assert_eq!(timing.frames, 100_000);
        assert!(timing.end_tick() > 3840.0, "the playhead moves on, it does not jump back");
    }

    #[test]
    fn a_recording_hold_plays_past_the_song_end_until_every_hold_is_released() {
        let (control, mut clock) = create_transport();
        control.set_song_end(Some(960));
        control.hold_song_loop(true);
        control.hold_song_loop(true);
        assert_eq!(control.wrap_region(), None);
        control.set_loop(Some((0, 480)));
        assert_eq!(control.wrap_region(), Some((0, 480)), "a hold does not touch the loop region");
        control.set_loop(None);
        control.play();
        assert_eq!(clock.advance(48_000, SR, &constant(120.0)).frames, 48_000, "the take runs past the song end");
        control.hold_song_loop(false);
        assert_eq!(control.wrap_region(), None, "one recording still holds");
        control.hold_song_loop(false);
        control.hold_song_loop(false);
        assert_eq!(control.wrap_region(), Some((0, 960)), "an extra release does not go below zero");
        control.hold_song_loop(true);
        assert_eq!(control.wrap_region(), None, "...so the next hold still holds");
    }

    #[test]
    fn the_song_end_is_clamped_like_the_loop() {
        let (control, _clock) = create_transport();
        control.set_song_end(Some(u64::MAX));
        assert_eq!(control.wrap_region(), Some((0, u64::from(u32::MAX))));
    }

    #[test]
    fn a_tempo_change_starts_a_segment_at_its_frame() {
        let tempo_map = map(&[(0, 120.0, false), (3840, 60.0, false)], &[(0, 4, 4)]);
        let (control, mut clock) = create_transport();
        control.play();
        let mut done = 0;
        while done < 96_000 {
            let timing = clock.advance(512, SR, &tempo_map);
            assert_eq!(timing.tempo.get(), 120.0);
            done += timing.frames;
        }
        assert_eq!(done, 96_000, "the segment ended exactly at the tempo change");
        let after = clock.advance(48_000, SR, &tempo_map);
        assert_eq!(after.tempo.get(), 60.0);
        assert!((after.start_tick - 3840.0).abs() < 1e-6);
        assert!((after.end_tick() - 3840.0 - 960.0).abs() < 1e-6, "a second at 60 BPM is one beat");
    }

    #[test]
    fn a_ramp_is_played_in_short_segments_and_lands_exactly() {
        let tempo_map = map(&[(0, 60.0, false), (3840, 120.0, true)], &[(0, 4, 4)]);
        let (control, mut clock) = create_transport();
        control.play();
        let ramp_frames = (4.0 * 2f64.ln() * f64::from(SR)).round() as usize;
        let mut done = 0;
        let mut last_tempo = 0.0;
        while done < ramp_frames {
            let timing = clock.advance(4096.min(ramp_frames - done), SR, &tempo_map);
            assert!(timing.frames <= RAMP_SEGMENT_FRAMES);
            assert!(timing.tempo.get() >= last_tempo, "the tempo rises");
            last_tempo = timing.tempo.get();
            done += timing.frames;
        }
        let after = clock.advance(1, SR, &tempo_map);
        assert!((after.start_tick - 3840.0).abs() < 0.05, "the ramp end by the exact formula: {}", after.start_tick);
    }

    #[test]
    fn segments_carry_the_signature_of_their_bar() {
        let tempo_map = map(&[(0, 120.0, false)], &[(0, 3, 4), (2880, 4, 4)]);
        let (control, mut clock) = create_transport();
        control.play();
        let first = clock.advance(100_000, SR, &tempo_map);
        assert_eq!(first.time_signature, TimeSignature::new(3, 4).unwrap());
        assert_eq!(first.frames, 72_000, "the segment ends at the signature change");
        let second = clock.advance(512, SR, &tempo_map);
        assert_eq!(second.time_signature, TimeSignature::FOUR_FOUR);
        assert_eq!(second.signature_start, 2880.0);
        assert_eq!(second.bar_start(second.start_tick + 10.0), 2880.0);
    }
}
