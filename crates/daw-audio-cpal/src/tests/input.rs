//! The input for the level meter: open only on request, reopened when the device changes.

use super::*;

#[test]
fn input_is_opened_only_while_watched_and_levels_drop_to_zero_after() {
    let mut rig = rig(None);
    rig.start().unwrap();
    assert!(!rig.fake.events().iter().any(|e| e.starts_with("open-in")), "the input is untouched without a request");

    rig.host.watch_input(true).unwrap();
    rig.host.watch_input(true).unwrap();
    assert_eq!(rig.fake.events().iter().filter(|e| e.starts_with("open-in")).count(), 1, "a repeat does not open it twice");
    rig.fake.feed.lock().unwrap().as_mut().unwrap().feed(&[0.5, 0.25]);
    assert_eq!(rig.shared.tap().level().left, 0.5);

    rig.host.watch_input(false).unwrap();
    assert!(rig.fake.events().contains(&"close in None".to_string()));
    assert_eq!(rig.shared.tap().level().left, 0.0);
}

#[test]
fn input_that_cannot_be_opened_reports_the_reason() {
    let mut rig = rig(Some(AudioSettings { input: Some("Line".into()), ..Default::default() }));
    rig.fake.broken.lock().unwrap().push("Line".into());
    let error = rig.host.watch_input(true).unwrap_err();
    assert!(error.is(audio::INPUT_FAILED), "{error}");
}

#[test]
fn changing_the_input_device_reopens_a_watched_input() {
    let mut rig = rig(None);
    rig.start().unwrap();
    rig.host.watch_input(true).unwrap();
    rig.host.apply(AudioSettings { input: Some("Line".into()), ..Default::default() }).unwrap();
    let events = rig.fake.events();
    assert_eq!(events.last().unwrap(), "open-in Some(\"Line\")");
    assert!(events.contains(&"close in None".to_string()));
}
