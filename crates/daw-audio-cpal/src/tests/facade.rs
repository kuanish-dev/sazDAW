//! `CpalAudio` from outside: commands reach the host thread through the `AudioDevices` and `AudioCapture` contracts.

use super::*;

fn facade(fake: Fake) -> (CpalAudio, Arc<MemoryStore>) {
    let store = Arc::new(MemoryStore::default());
    (CpalAudio::with_backend(store.clone(), Arc::new(MemoryLog::default()), move || Box::new(fake)), store)
}

#[test]
fn facade_talks_to_the_host_thread_through_the_contract() {
    let fake = Fake::default();
    let (audio, store) = facade(fake.clone());
    assert_eq!(audio.outputs().len(), 3);
    assert!(audio.inputs()[0].is_default);
    assert!(!audio.status().running);

    assert_eq!(audio.start_output(Box::new(Prepared(Arc::default()))).unwrap().channels, 2);
    let status = audio.apply(Rig::choose(Some("Interface"), None)).unwrap();
    assert_eq!(status, audio.status());
    assert_eq!(audio.settings().output.as_deref(), Some("Interface"));
    assert_eq!(store.load().output.as_deref(), Some("Interface"));

    audio.watch_input(true).unwrap();
    assert_eq!(audio.input_level().left, 0.0);
    audio.stop_output();
    assert!(!audio.status().running);
    drop(audio);
    assert!(fake.events().iter().any(|e| e == "close Interface"), "streams are closed");
}

#[test]
fn host_without_devices_is_safe_for_tests() {
    let store = Arc::new(MemoryStore::default());
    let audio = CpalAudio::without_devices(store.clone(), Arc::new(MemoryLog::default()));
    assert!(audio.outputs().is_empty() && audio.inputs().is_empty());
    assert!(audio.start_output(Box::new(Prepared(Arc::default()))).is_err());
    assert!(audio.watch_input(true).is_err());
    assert!(audio.apply(Rig::choose(Some("Speakers"), None)).is_err());
    audio.apply(AudioSettings::default()).unwrap();
    assert!(store.saved.lock().unwrap().is_some());
}

#[test]
fn facade_implements_the_capture_contract() {
    use daw_contracts::AudioCapture;
    let fake = Fake::default();
    let (audio, _) = facade(fake.clone());
    let format = AudioCapture::start(&audio).unwrap();
    assert_eq!(format.channels, 2);
    let mut out = Vec::new();
    assert_eq!(AudioCapture::drain(&audio, &mut out), 0);
    assert_eq!(audio.dropped_frames(), 0);
    AudioCapture::stop(&audio);
}

#[test]
fn capture_without_devices_fails_cleanly() {
    use daw_contracts::AudioCapture;
    let audio = CpalAudio::without_devices(Arc::new(MemoryStore::default()), Arc::new(MemoryLog::default()));
    assert!(AudioCapture::start(&audio).is_err());
    assert_eq!(AudioCapture::drain(&audio, &mut Vec::new()), 0);
}
