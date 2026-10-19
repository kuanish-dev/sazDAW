//! `LiveMeter` honours `MasterMeter`: started by a reading and stopped a lease after the last one, the BS.1770 readings of what the tap captured, the reset and
//! the playback restart mark, phase, the vectorscope frames, and lost frames in the log. The tap is a fake that holds the
//! frames the test writes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use daw_contracts::sync::lock;
use daw_contracts::{AppLog, LogEntry, MASTER_TAP_FRAMES, MasterFrames, MasterMeter, MasterTap, VECTOR_FRAMES};

use super::LiveMeter;
use crate::loudness::tests::{RATE, sine};

#[derive(Default)]
struct Captured {
    open: bool,
    /// Every frame since the tap was opened, interleaved.
    samples: Vec<f32>,
    restart: Option<u64>,
}

/// A tap that hands out the frames the test wrote, at most `held` of the newest.
struct FakeTap {
    captured: Mutex<Captured>,
    held: usize,
}

impl FakeTap {
    fn new() -> Arc<Self> {
        Self::holding(MASTER_TAP_FRAMES)
    }

    fn holding(held: usize) -> Arc<Self> {
        Arc::new(Self { captured: Mutex::new(Captured::default()), held })
    }

    fn play(&self, samples: &[f32]) {
        let mut captured = lock(&self.captured);
        assert!(captured.open, "the meter keeps the tap open while it measures");
        captured.samples.extend_from_slice(samples);
    }

    fn frames(&self) -> u64 {
        (lock(&self.captured).samples.len() / 2) as u64
    }

    fn mark_restart(&self, at: u64) {
        lock(&self.captured).restart = Some(at);
    }

    fn is_open(&self) -> bool {
        lock(&self.captured).open
    }
}

impl MasterTap for FakeTap {
    fn open_tap(&self, on: bool) {
        *lock(&self.captured) = Captured { open: on, ..Captured::default() };
    }

    fn frames_after(&self, end: u64) -> MasterFrames {
        let captured = lock(&self.captured);
        let total = (captured.samples.len() / 2) as u64;
        let start = end.max(total.saturating_sub(self.held as u64)).min(total);
        let samples = captured.samples[2 * start as usize..].to_vec();
        MasterFrames { start, samples, restart: captured.restart, sample_rate: RATE }
    }
}

#[derive(Default)]
struct MemoryLog(Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        lock(&self.0).push(entry);
    }

    fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let entries = lock(&self.0);
        entries[entries.len().saturating_sub(limit)..].to_vec()
    }

    fn file(&self) -> Option<PathBuf> {
        None
    }
}

fn meter_on(tap: &Arc<FakeTap>) -> (LiveMeter, Arc<MemoryLog>) {
    let log = Arc::new(MemoryLog::default());
    let meter = LiveMeter::new(tap.clone(), log.clone());
    meter.reading();
    (meter, log)
}

/// A meter whose readings keep it measuring for only a tenth of a second.
fn short_lease(tap: &Arc<FakeTap>) -> LiveMeter {
    LiveMeter::with_lease(tap.clone(), Arc::new(MemoryLog::default()), Duration::from_millis(100))
}

/// Waits (a few seconds at most) for the meter's thread to close the tap.
fn wait_closed(tap: &FakeTap) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while tap.is_open() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
}

fn close(actual: Option<f32>, expected: f32, tolerance: f32) -> bool {
    actual.is_some_and(|a| (a - expected).abs() <= tolerance)
}

#[test]
fn a_reading_starts_the_measuring_and_without_readings_the_tap_closes() {
    let tap = FakeTap::new();
    let meter = short_lease(&tap);
    assert!(!tap.is_open(), "closed at creation");

    let empty = meter.reading();
    assert!(tap.is_open(), "the reading started the measuring");
    assert_eq!((empty.integrated_lufs, empty.seconds, empty.vector.len()), (None, 0.0, 0), "nothing played yet");

    wait_closed(&tap);
    assert!(!tap.is_open(), "no reading for a whole lease: the meter's thread closed the tap");
}

/// EBU Tech 3341's reference: a 1 kHz sine at −23 dBFS in both channels reads −23 LUFS momentary, short-term and
/// integrated, no range, a −23 dBTP peak and full correlation.
#[test]
fn the_reference_sine_reads_its_loudness_peak_and_phase() {
    let tap = FakeTap::new();
    let (meter, log) = meter_on(&tap);
    let tone = sine(1000.0, -23.0, 10.0, true, true);
    tap.play(&tone);
    let reading = meter.reading();
    for (name, value) in
        [("momentary", reading.momentary_lufs), ("short-term", reading.short_term_lufs), ("integrated", reading.integrated_lufs)]
    {
        assert!(close(value, -23.0, 0.1), "{name}: {value:?}");
    }
    assert_eq!(reading.range_lu, Some(0.0));
    assert!(close(reading.true_peak_db, -23.0, 0.1) && close(reading.max_true_peak_db, -23.0, 0.1), "{reading:?}");
    assert!(!reading.clipped);
    assert!(close(reading.correlation, 1.0, 1e-3), "{:?}", reading.correlation);
    assert!((reading.seconds - 10.0).abs() < 1e-9);
    assert_eq!(reading.sample_rate, RATE);
    assert_eq!(reading.vector, tone[tone.len() - 2 * VECTOR_FRAMES..], "the newest frames, oldest first");
    assert!(lock(&log.0).is_empty(), "nothing was lost");
}

#[test]
fn opposite_channels_read_minus_one_and_a_lone_channel_reads_zero() {
    let correlation_of = |samples: Vec<f32>| {
        let tap = FakeTap::new();
        let (meter, _) = meter_on(&tap);
        tap.play(&samples);
        meter.reading().correlation
    };
    let inverted: Vec<f32> = sine(1000.0, -20.0, 1.0, true, true).chunks_exact(2).flat_map(|f| [f[0], -f[1]]).collect();
    assert!(close(correlation_of(inverted), -1.0, 1e-3));
    assert_eq!(correlation_of(sine(1000.0, -20.0, 1.0, true, false)), Some(0.0), "nothing cancels in mono");
    assert_eq!(correlation_of(vec![0.0; 2 * RATE as usize]), None, "silence");
}

#[test]
fn a_true_peak_over_full_scale_lights_the_clip_until_the_reset() {
    let tap = FakeTap::new();
    let (meter, _) = meter_on(&tap);
    tap.play(&sine(997.0, -1.0, 1.0, true, true));
    assert!(!meter.reading().clipped, "−1 dBTP is below the clip");
    tap.play(&sine(997.0, 1.0, 1.0, true, true));
    let over = meter.reading();
    assert!(over.clipped && close(over.max_true_peak_db, 1.0, 0.1), "{over:?}");

    meter.reset();
    tap.play(&sine(997.0, -6.0, 1.0, true, true));
    let after = meter.reading();
    assert!(!after.clipped && close(after.max_true_peak_db, -6.0, 0.1), "{after:?}");
}

/// A reset forgets the integration and the held peak, the momentary loudness runs on.
#[test]
fn a_reset_starts_the_integration_over() {
    let tap = FakeTap::new();
    let (meter, _) = meter_on(&tap);
    tap.play(&sine(1000.0, -10.0, 5.0, true, true));
    meter.reset();
    let right_after = meter.reading();
    assert_eq!((right_after.integrated_lufs, right_after.max_true_peak_db, right_after.seconds), (None, None, 0.0));
    assert!(close(right_after.momentary_lufs, -10.0, 0.1), "the momentary window runs on");

    tap.play(&sine(1000.0, -30.0, 5.0, true, true));
    let reading = meter.reading();
    assert!(close(reading.integrated_lufs, -30.0, 0.1), "{:?}", reading.integrated_lufs);
    assert!(close(reading.max_true_peak_db, -30.0, 0.1));
}

/// Playback from the song start restarts the integration at the marked frame, even when it lands between readings; a
/// mark the frames have not reached yet waits for its frame.
#[test]
fn playback_from_the_start_restarts_at_its_frame() {
    let tap = FakeTap::new();
    let (meter, _) = meter_on(&tap);
    tap.play(&sine(1000.0, -10.0, 5.0, true, true));
    let mark = tap.frames();
    tap.mark_restart(mark);
    tap.play(&sine(1000.0, -30.0, 5.0, true, true));
    let reading = meter.reading();
    assert!(close(reading.integrated_lufs, -30.0, 0.1), "only after the mark: {:?}", reading.integrated_lufs);
    assert!((reading.seconds - 5.0).abs() < 1e-9);

    let ahead = tap.frames() + u64::from(RATE);
    tap.mark_restart(ahead);
    let waiting = meter.reading();
    assert!((waiting.seconds - 5.0).abs() < 1e-9, "the mark ahead waits");
    tap.play(&sine(1000.0, -20.0, 2.0, true, true));
    let reached = meter.reading();
    assert!((reached.seconds - 1.0).abs() < 1e-9, "restarted one second into the last stretch: {}", reached.seconds);
    assert!(close(reached.integrated_lufs, -20.0, 0.1));
}

#[test]
fn frames_the_meter_could_not_read_in_time_are_logged() {
    let tap = FakeTap::holding(1_000);
    let (meter, log) = meter_on(&tap);
    tap.play(&sine(1000.0, -20.0, 1.0, true, true));
    meter.reading();
    let entries = lock(&log.0);
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].source == "meter" && entries[0].message.contains("missed"), "{:?}", entries[0]);
}

#[test]
fn a_reading_after_the_lease_ran_out_starts_a_fresh_measurement() {
    let tap = FakeTap::new();
    let meter = short_lease(&tap);
    meter.reading();
    tap.play(&sine(1000.0, -10.0, 2.0, true, true));
    assert!(meter.reading().integrated_lufs.is_some(), "a reading within the lease keeps measuring");

    wait_closed(&tap);
    let fresh = meter.reading();
    assert_eq!((fresh.integrated_lufs, fresh.seconds), (None, 0.0));
}
