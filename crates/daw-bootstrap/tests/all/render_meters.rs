//! The live master meter and the mono check wired to the real engine: the demo measured as it plays from the start, and
//! the mono check heard on the output.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests: failing with a clear message is the check itself"
)]

use crate::common::*;

#[test]
fn the_demo_is_measured_as_it_plays_from_the_start() {
    let mut session = start(true);
    let controls = session.controls.clone();
    let first = controls.master_meter.reading();
    assert_eq!((first.integrated_lufs, first.seconds), (None, 0.0), "the first reading starts the measuring");

    render(&mut session, 4_800);
    controls.transport.play();
    for _ in 0..8 {
        render(&mut session, 24_000);
        controls.master_meter.reading();
    }
    let reading = controls.master_meter.reading();
    assert!(reading.integrated_lufs.is_some_and(|lufs| (-40.0..0.0).contains(&lufs)), "{reading:?}");
    assert!(reading.short_term_lufs.is_some() && reading.max_true_peak_db.is_some(), "{reading:?}");
    assert!(reading.seconds > 3.9 && reading.seconds <= 4.0, "the silence before playing is left out: {}", reading.seconds);
    assert_eq!(reading.sample_rate, 48_000);
}

#[test]
fn the_mono_check_sends_the_same_sound_to_both_speakers() {
    let mut session = start(true);
    let controls = session.controls.clone();
    controls.mono.set_mono(true);
    controls.transport.play();
    let out = render(&mut session, 24_000);
    assert!(rms(&out) > 0.01, "the beat sounds");
    assert!(out.chunks_exact(2).all(|frame| frame[0] == frame[1]), "both channels the same");
    controls.mono.set_mono(false);
    assert!(!controls.mono.is_mono());
}
