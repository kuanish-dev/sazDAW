//! Monitoring input: the host opens it at the engine request (`LiveInput::set_wanted`) and closes it without one.

use daw_contracts::LiveInput;

use super::*;

fn opened(rig: &Rig) -> usize {
    rig.fake.events().iter().filter(|e| e.starts_with("open-in")).count()
}

#[test]
fn the_engine_request_opens_the_input_and_frames_reach_the_reader() {
    let mut rig = rig(None);
    rig.start().unwrap();
    let mut reader = rig.shared.take_live_reader().unwrap();
    assert!(rig.shared.take_live_reader().is_none(), "the reading side is given out once");
    rig.host.follow_monitoring();
    assert_eq!(opened(&rig), 0, "without a request the input is closed");

    reader.set_wanted(true);
    rig.host.follow_monitoring();
    rig.host.follow_monitoring();
    assert_eq!(opened(&rig), 1);
    assert_eq!(reader.sample_rate(), Some(48_000));
    rig.fake.feed.lock().unwrap().as_mut().unwrap().feed(&[0.5, 0.25]);
    let mut out = [0.0; 2];
    assert_eq!(reader.read(&mut out), 1);
    assert_eq!(out, [0.5, 0.25]);

    reader.set_wanted(false);
    rig.host.follow_monitoring();
    assert!(rig.fake.events().contains(&"close in None".to_string()));
    assert_eq!(reader.sample_rate(), None, "the input is closed");
}

#[test]
fn monitoring_shares_the_input_with_capture_and_needs_running_output() {
    let mut rig = rig(None);
    let mut reader = rig.shared.take_live_reader().unwrap();
    reader.set_wanted(true);
    rig.host.follow_monitoring();
    assert_eq!(opened(&rig), 0, "output is not started: the engine does not play, the input is not needed");

    rig.start().unwrap();
    rig.host.follow_monitoring();
    rig.host.capture_start().unwrap();
    assert_eq!(opened(&rig), 1, "one stream for both");
    reader.set_wanted(false);
    rig.host.follow_monitoring();
    assert!(!rig.fake.events().iter().any(|e| e.starts_with("close in")), "recording still holds the input");
}

#[test]
fn a_failing_input_is_logged_once_per_request() {
    let mut rig = rig(Some(AudioSettings { input: Some("Line".into()), ..Default::default() }));
    rig.fake.broken.lock().unwrap().push("Line".into());
    rig.start().unwrap();
    let mut reader = rig.shared.take_live_reader().unwrap();
    reader.set_wanted(true);
    rig.host.follow_monitoring();
    rig.host.follow_monitoring();
    let warnings = || rig.log.0.lock().unwrap().iter().filter(|e| e.message.contains("monitoring")).count();
    assert_eq!(warnings(), 1);
    reader.set_wanted(false);
    rig.host.follow_monitoring();
    reader.set_wanted(true);
    rig.host.follow_monitoring();
    assert_eq!(warnings(), 2, "a new request is a new attempt");
}
