//! Plugin delay compensation (PDC): parallel paths meet in step, chain latencies add up, the sidechain is aligned, a
//! latency change is picked up by a snapshot, the limit is cut and counted.

use std::sync::atomic::{AtomicU32, Ordering};

use daw_contracts::BlockTiming;

use super::*;
use crate::mixer::{MAX_DELAY_LINES, MAX_PDC_FRAMES};

/// An effect with a real latency: the output is the input `latency` frames ago; the latency can be changed from outside.
pub(super) struct Latent {
    latency: Arc<AtomicU32>,
    ring: Vec<f32>,
    write: usize,
}

const LATENT_CAPACITY: usize = 4_096;

impl Latent {
    pub(super) fn boxed(frames: u32) -> Box<dyn Effect> {
        Self::shared(Arc::new(AtomicU32::new(frames)))
    }

    fn shared(latency: Arc<AtomicU32>) -> Box<dyn Effect> {
        Box::new(Self { latency, ring: vec![0.0; LATENT_CAPACITY * 2], write: 0 })
    }
}

impl Effect for Latent {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, buffer: &mut [f32], _: Option<&[f32]>) {
        let delay = (self.latency.load(Ordering::Relaxed) as usize).min(LATENT_CAPACITY - 1);
        for frame in buffer.chunks_exact_mut(2) {
            let read = (self.write + LATENT_CAPACITY - delay) % LATENT_CAPACITY * 2;
            self.ring[self.write * 2] = frame[0];
            self.ring[self.write * 2 + 1] = frame[1];
            frame[0] = self.ring[read];
            frame[1] = self.ring[read + 1];
            self.write = (self.write + 1) % LATENT_CAPACITY;
        }
    }
    fn latency_samples(&self) -> u32 {
        self.latency.load(Ordering::Relaxed)
    }
}

/// An "output = key" effect: shows when the sidechain signal arrived.
struct KeyProbe;

impl Effect for KeyProbe {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, buffer: &mut [f32], sidechain: Option<&[f32]>) {
        for (index, sample) in buffer.iter_mut().enumerate() {
            *sample = sidechain.map_or(0.0, |key| key[index]);
        }
    }
}

/// An effect that only declares a latency (does not touch the sound).
struct Claims(u32);

impl Effect for Claims {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, _: &mut [f32], _: Option<&[f32]>) {}
    fn latency_samples(&self) -> u32 {
        self.0
    }
}

/// The first frame where the left channel is noticeably not zero.
pub(super) fn first_sound(out: &[f32]) -> Option<usize> {
    out.chunks_exact(2).position(|frame| frame[0].abs() > 1e-6)
}

#[test]
fn a_latent_path_and_a_dry_path_meet_aligned_at_master() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Latent::boxed(100));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(first_sound(&out), Some(100), "the dry path waits for the delayed one");
    assert!(near(Rig::left(&out, 100), 1.0), "both paths sound from the same frame: 0.5 + 0.5");
    assert_eq!(rig.link.latency().frames, 100);
    assert_eq!(rig.link.latency().sample_rate, SR);
}

#[test]
fn latencies_along_a_chain_add_up() {
    let mut rig = Rig::new();
    rig.add_track(10, MASTER);
    let first = rig.add_fx(false, Latent::boxed(100));
    let second = rig.add_fx(false, Latent::boxed(50));
    rig.connect(first, second, 1.0, false);
    rig.connect(second, MASTER, 1.0, false);
    rig.add_track(11, first);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(first_sound(&out), Some(150));
    assert!(near(Rig::left(&out, 150), 1.0));
    assert_eq!(rig.link.latency().frames, 150);
}

#[test]
fn a_bus_aligns_its_mixed_inputs_and_the_master_aligns_the_bus_with_a_dry_track() {
    let mut rig = Rig::new();
    let bus = rig.add_bus(1.0);
    let fx = rig.add_fx(false, Latent::boxed(100));
    rig.connect(fx, bus, 1.0, false);
    rig.add_track(10, fx);
    rig.add_track(11, bus);
    rig.add_track(12, MASTER);
    for track in [10, 11, 12] {
        rig.note(track, 60, 0, 960);
    }
    *rig.node_kind(MASTER) = NodeKind::Master { volume: 0.5 };
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(first_sound(&out), Some(100), "all three paths wait for the longest one");
    assert!(near(Rig::left(&out, 100), 0.75), "(0.5 + 0.5 + 0.5) · 0.5");
}

#[test]
fn the_sidechain_key_is_aligned_with_the_main_input() {
    let mut rig = Rig::new();
    let latent = rig.add_fx(false, Latent::boxed(100));
    let probe = rig.add_fx(false, Box::new(KeyProbe));
    rig.connect(latent, probe, 1.0, false);
    rig.connect(probe, MASTER, 1.0, false);
    rig.add_track(10, latent);
    let key = rig.add_track(11, MASTER);
    rig.connect(key, probe, 1.0, true);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(first_sound(&out), Some(100));
    assert!(near(Rig::left(&out, 100), 1.0), "key 0.5 + track 0.5 from the same frame");
}

#[test]
fn a_bypassed_effect_adds_no_latency() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(true, Latent::boxed(100));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert_eq!(first_sound(&out), Some(0));
    assert_eq!(rig.link.latency().frames, 0);
}

#[test]
fn a_changed_latency_is_picked_up_by_the_next_snapshot() {
    let mut rig = Rig::new();
    let latency = Arc::new(AtomicU32::new(100));
    let fx = rig.add_fx(false, Latent::shared(Arc::clone(&latency)));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    rig.render(10_000);
    assert_eq!(rig.link.latency().frames, 100);

    latency.store(300, Ordering::Relaxed);
    rig.publish();
    let out = rig.render(15_000);
    assert_eq!(rig.link.latency().frames, 300);
    let end = 24_000 + 300 - 10_000;
    assert!(near(Rig::left(&out, end - 1), 1.0), "both paths up to the end: {}", Rig::left(&out, end - 1));
    assert!(near(Rig::left(&out, end), 0.0), "silence after the end, not half: {}", Rig::left(&out, end));
    assert_eq!(rig.link.losses().latency_clamped, 0);
}

#[test]
fn a_latency_above_the_limit_is_clamped_and_counted() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(Claims(MAX_PDC_FRAMES as u32 + 10)));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.add_track(11, MASTER);
    rig.note(10, 60, 0, 960);
    rig.note(11, 62, 0, 960);
    rig.publish();
    rig.transport.play();
    let out = rig.render(1_000);
    assert!(rig.link.losses().latency_clamped >= 1);
    assert_eq!(rig.link.latency().frames, MAX_PDC_FRAMES as u32 + 10, "the total latency is honest");
    assert!(near(Rig::left(&out, 500), 0.5));
}

#[test]
fn running_out_of_delay_lines_is_counted() {
    let mut rig = Rig::new();
    let fx = rig.add_fx(false, Box::new(Claims(10)));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(1, fx);
    for track in 0..=MAX_DELAY_LINES as TrackId {
        rig.add_track(100 + track, MASTER);
    }
    rig.publish();
    rig.render(100);
    assert!(rig.link.losses().latency_clamped >= 1);
}

/// A post-processor that remembers the segment start.
struct TimingProbe {
    starts: Arc<Mutex<Vec<f64>>>,
}

impl AudioProcessor for TimingProbe {
    fn prepare(&mut self, _: StreamConfig) {}
    fn process(&mut self, _: &mut AudioBlock, timing: &BlockTiming) {
        self.starts.lock().unwrap().push(timing.start_tick);
    }
}

#[test]
fn post_processors_hear_the_timeline_delayed_by_the_total_latency() {
    let starts = Arc::new(Mutex::new(Vec::new()));
    let mut rig = Rig::with_post_processors(vec![Box::new(TimingProbe { starts: Arc::clone(&starts) })]);

    let fx = rig.add_fx(false, Latent::boxed(100));
    rig.connect(fx, MASTER, 1.0, false);
    rig.add_track(10, fx);
    rig.publish();
    rig.transport.play();
    rig.render(256);
    let first = starts.lock().unwrap()[0];
    assert!((first - -(100.0 / FRAMES_PER_TICK as f64)).abs() < 1e-9, "the metronome lags together with the music: {first}");
}
