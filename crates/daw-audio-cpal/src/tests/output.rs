//! Output: start with the saved choice, device and buffer changes with rollback, stream faults, stop.

use super::*;

#[test]
fn start_opens_the_system_device_when_nothing_is_saved() {
    let mut rig = rig(None);
    assert_eq!(rig.start().unwrap(), StreamConfig { sample_rate: 48_000, channels: 2 });
    let status = rig.shared.status();
    assert!(status.running && status.output_name.as_deref() == Some("Speakers"));
    assert_eq!(status.buffer_range, Some((32, 2048)));
    assert_eq!(rig.fake.events(), ["open Speakers None"]);
}

#[test]
fn start_uses_the_saved_device_and_buffer() {
    let mut rig = rig(Some(Rig::choose(Some("Interface"), Some(128))));
    assert_eq!(rig.start().unwrap().sample_rate, 96_000);
    assert_eq!(rig.fake.events(), ["open Interface Some(128)"]);
}

#[test]
fn start_falls_back_to_the_system_device_and_warns_when_the_saved_one_is_gone() {
    let mut rig = rig(Some(Rig::choose(Some("Unplugged"), None)));
    assert_eq!(rig.start().unwrap().sample_rate, 48_000, "there is sound, the app did not fail");
    let entries = rig.log.recent(10);
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].level, entries[0].source.as_str()), (LogLevel::Warn, "audio"));
    assert!(entries[0].message.contains("Unplugged"));
    assert_eq!(rig.store.load().output.as_deref(), Some("Unplugged"), "the user choice is kept: the device may come back");
}

#[test]
fn start_fails_with_a_reason_when_even_the_system_device_does_not_open() {
    let mut rig = rig(None);
    rig.fake.broken.lock().unwrap().push("Speakers".into());
    assert!(rig.start().is_err());
    let status = rig.shared.status();
    assert!(!status.running && status.error.unwrap().is(audio::OUTPUT_FAILED));
}

#[test]
fn apply_closes_the_old_stream_before_opening_the_new_one_and_reprepares_the_engine() {
    let mut rig = rig(None);
    rig.start().unwrap();
    let status = rig.host.apply(Rig::choose(Some("Interface"), Some(256))).unwrap();
    assert_eq!((status.sample_rate, status.output_name.as_deref()), (96_000, Some("Interface")));
    assert_eq!(rig.fake.events(), ["open Speakers None", "close Speakers", "open Interface Some(256)"]);
    let prepared = rig.prepared.lock().unwrap().clone();
    assert_eq!(prepared.last().unwrap().sample_rate, 96_000, "the engine learned the new rate");
    assert_eq!(rig.store.load(), Rig::choose(Some("Interface"), Some(256)), "the choice is saved");
    assert_eq!(rig.shared.settings().output.as_deref(), Some("Interface"));
}

#[test]
fn failed_apply_returns_to_the_previous_device_and_does_not_save() {
    let mut rig = rig(Some(Rig::choose(Some("Interface"), None)));
    rig.start().unwrap();
    rig.fake.broken.lock().unwrap().push("Broken".into());
    let error = rig.host.apply(Rig::choose(Some("Broken"), None)).unwrap_err();
    assert!(error.is(audio::OUTPUT_FAILED), "{error}");
    let status = rig.shared.status();
    assert!(status.running && status.output_name.as_deref() == Some("Interface"), "audio came back: {status:?}");
    assert_eq!(rig.store.load().output.as_deref(), Some("Interface"));
    assert_eq!(rig.shared.settings().output.as_deref(), Some("Interface"));
}

#[test]
fn failed_apply_falls_back_to_the_system_device_when_the_previous_one_is_also_gone() {
    let mut rig = rig(Some(Rig::choose(Some("Interface"), None)));
    rig.start().unwrap();
    rig.fake.broken.lock().unwrap().extend(["Broken".into(), "Interface".into()]);
    assert!(rig.host.apply(Rig::choose(Some("Broken"), None)).is_err());
    assert_eq!(rig.shared.status().output_name.as_deref(), Some("Speakers"));
}

#[test]
fn unknown_device_or_bad_buffer_is_refused_without_touching_the_running_stream() {
    let mut rig = rig(None);
    rig.start().unwrap();
    let before = rig.fake.events();

    let unknown = rig.host.apply(Rig::choose(Some("Missing"), None)).unwrap_err();
    assert!(unknown.is(audio::OUTPUT_DEVICE_NOT_FOUND), "{unknown}");
    let too_small = rig.host.apply(Rig::choose(None, Some(8))).unwrap_err();
    assert!(too_small.is(audio::BUFFER_SIZE_OUT_OF_RANGE), "{too_small}");
    assert!(too_small.message().contains("32") && too_small.message().contains("2048"), "{too_small}");
    assert!(rig.host.apply(Rig::choose(None, Some(100_000))).is_err());
    let no_input = rig.host.apply(AudioSettings { input: Some("No mic".into()), ..Default::default() }).unwrap_err();
    assert!(no_input.is(audio::INPUT_DEVICE_NOT_FOUND), "{no_input}");

    assert_eq!(rig.fake.events(), before, "the stream was neither closed nor opened");
    assert!(rig.shared.status().running);
    assert_eq!(rig.store.load(), AudioSettings::default());
}

#[test]
fn a_choice_that_cannot_be_saved_is_rolled_back() {
    let mut rig = rig(None);
    rig.start().unwrap();
    *rig.store.fail.lock().unwrap() = true;
    assert!(matches!(rig.host.apply(Rig::choose(Some("Interface"), None)), Err(e) if e.is(general::INTERNAL)));
    assert_eq!(rig.shared.status().output_name.as_deref(), Some("Speakers"));
    assert_eq!(rig.shared.settings().output, None);
}

#[test]
fn apply_before_start_only_validates_and_remembers() {
    let mut rig = rig(None);
    let status = rig.host.apply(Rig::choose(Some("Interface"), Some(64))).unwrap();
    assert!(!status.running, "audio has not been started yet");
    assert!(rig.fake.events().is_empty(), "nothing is open");
    assert_eq!(rig.start().unwrap().sample_rate, 96_000, "start takes the remembered choice");
}

#[test]
fn stream_faults_go_to_the_log_and_the_status() {
    let mut rig = rig(None);
    rig.start().unwrap();
    let faults = rig.fake.faults.lock().unwrap().clone().unwrap();
    faults.record(Fault::DeviceGone);
    assert!(rig.shared.status().running, "the error callback writes nothing itself: it may be the audio thread");
    rig.host.report_faults();
    let status = rig.shared.status();
    assert!(!status.running && status.error == Some(Fault::DeviceGone.error()));
    let entries = rig.log.recent(10);
    assert_eq!((entries[0].level, entries[0].source.as_str()), (LogLevel::Error, "audio"));
    assert!(entries[0].message.contains("output") && entries[0].message.contains(&Fault::DeviceGone.error().message()));
}

#[test]
fn overloads_are_logged_at_most_once_per_interval_with_their_count() {
    let mut rig = rig(None);
    rig.start().unwrap();
    let faults = rig.fake.faults.lock().unwrap().clone().unwrap();
    for _ in 0..3 {
        faults.record(Fault::Xrun);
    }
    rig.host.report_faults();
    faults.record(Fault::Xrun);
    rig.host.report_faults();
    let entries = rig.log.recent(10);
    assert_eq!(entries.len(), 1, "the second overload waits for the next window: {entries:?}");
    assert!(entries[0].message.contains("×3"), "{}", entries[0].message);
    assert!(rig.shared.status().running, "an overload does not stop the stream");
}

/// Regression: losing both the previous and the system device after a failed change used to go unlogged.
#[test]
fn losing_every_output_after_a_failed_apply_is_logged() {
    let mut rig = rig(Some(Rig::choose(Some("Interface"), None)));
    rig.start().unwrap();
    rig.fake.broken.lock().unwrap().extend(["Broken".into(), "Interface".into(), "Speakers".into()]);
    assert!(rig.host.apply(Rig::choose(Some("Broken"), None)).is_err());
    let entries = rig.log.recent(10);
    assert!(entries.iter().any(|e| e.level == LogLevel::Error && e.message.contains("no sound")), "{entries:?}");
}

#[test]
fn stop_closes_everything_and_allows_a_new_start() {
    let mut rig = rig(None);
    rig.start().unwrap();
    rig.host.watch_input(true).unwrap();
    rig.host.stop();
    assert!(!rig.shared.status().running);
    assert_eq!(rig.fake.events().iter().filter(|e| e.starts_with("close")).count(), 2);
    assert!(rig.start().is_ok());
}

#[test]
fn render_block_is_silent_instead_of_waiting_when_the_engine_is_busy() {
    let renderer: SharedRenderer = Arc::new(Mutex::new(Box::new(Prepared(Arc::default()))));
    let mut data = [1.0f32; 4];
    render_block(&renderer, &mut data);
    assert_eq!(data, [0.25; 4]);

    let _busy = renderer.lock().unwrap();
    let mut data = [1.0f32; 4];
    render_block(&renderer, &mut data);
    assert_eq!(data, [0.0; 4], "streams are being swapped: the block is silent, the callback does not wait");
}
