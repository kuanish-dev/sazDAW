//! Input capture: frames are copied while recording, the stream is shared with the meter, refusals.

use super::*;

fn feed(rig: &Rig, frames: &[f32]) {
    rig.fake.feed.lock().unwrap().as_mut().unwrap().feed(frames);
}

fn drain(rig: &Rig) -> Vec<f32> {
    let mut out = Vec::new();
    rig.shared.drain_capture(&mut out);
    out
}

#[test]
fn capture_copies_frames_only_while_it_runs_and_reports_the_stream_format() {
    let mut rig = rig(None);
    rig.start().unwrap();
    let format = rig.host.capture_start().unwrap();
    assert_eq!((format.sample_rate, format.channels), (48_000, 2));
    feed(&rig, &[0.1, 0.2, 0.3, 0.4]);
    assert_eq!(rig.shared.tap().level().right, 0.4, "the level is measured while recording too");
    rig.host.capture_stop();
    feed(&rig, &[0.9, 0.9]);
    assert_eq!(drain(&rig), [0.1, 0.2, 0.3, 0.4], "the rest is drained after stopping, nothing new is queued");
}

#[test]
fn input_stays_open_until_both_watching_and_capture_are_done() {
    let mut rig = rig(None);
    rig.host.watch_input(true).unwrap();
    rig.host.capture_start().unwrap();
    assert_eq!(rig.fake.events().iter().filter(|e| e.starts_with("open-in")).count(), 1, "one stream for both");
    rig.host.watch_input(false).unwrap();
    assert!(!rig.fake.events().iter().any(|e| e.starts_with("close in")), "recording still holds the input");
    rig.host.capture_stop();
    assert!(rig.fake.events().contains(&"close in None".to_string()));

    rig.host.capture_start().unwrap();
    rig.host.watch_input(true).unwrap();
    rig.host.capture_stop();
    assert_eq!(rig.fake.events().iter().filter(|e| e.starts_with("close in")).count(), 1, "the meter still holds the input");
}

#[test]
fn capture_start_clears_the_previous_take_and_the_loss_counter() {
    let mut rig = rig(None);
    rig.host.capture_start().unwrap();
    feed(&rig, &vec![0.5; 2 * 48_000 * 6]);
    assert_eq!(rig.shared.tap().dropped(), 48_000);
    rig.host.capture_stop();
    rig.host.capture_start().unwrap();
    assert_eq!(rig.shared.tap().dropped(), 0);
    assert!(drain(&rig).is_empty());
}

#[test]
fn second_capture_start_and_input_change_during_capture_are_refused() {
    let mut rig = rig(None);
    rig.host.capture_start().unwrap();
    assert!(matches!(rig.host.capture_start(), Err(e) if e.is(audio::CAPTURE_ALREADY_RUNNING)));
    let error = rig.host.apply(AudioSettings { input: Some("Line".into()), ..Default::default() }).unwrap_err();
    assert!(error.is(audio::INPUT_CHANGE_WHILE_RECORDING), "{error}");
    assert!(rig.host.apply(Rig::choose(Some("Interface"), None)).is_ok(), "the output may change");
}

#[test]
fn capture_that_cannot_open_the_input_reports_the_reason_and_stays_off() {
    let mut rig = rig(Some(AudioSettings { input: Some("Line".into()), ..Default::default() }));
    rig.fake.broken.lock().unwrap().push("Line".into());
    let error = rig.host.capture_start().unwrap_err();
    assert!(error.is(audio::INPUT_FAILED), "{error}");
    rig.fake.broken.lock().unwrap().clear();
    assert!(rig.host.capture_start().is_ok(), "after an error capture can start again");
}
