//! Input monitoring: the input goes into the audio track node by its mode (`Off`/`On`/`Auto`) and channels, a frame
//! shortage counts as a loss, different input and output rates turn monitoring off.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

use daw_contracts::{InputChannels, InputMonitor, LiveInput, TrackInput};

use super::*;

/// A fake input: the test puts frames in, the engine reads them.
#[derive(Clone)]
struct FakeInput {
    frames: Arc<Mutex<VecDeque<f32>>>,
    rate: Option<u32>,
    wanted: Arc<AtomicBool>,
}

impl FakeInput {
    fn new(rate: Option<u32>) -> Self {
        Self { frames: Arc::default(), rate, wanted: Arc::default() }
    }

    /// `count` stereo frames (left, right).
    fn push(&self, count: usize, left: f32, right: f32) {
        let mut frames = self.frames.lock().unwrap();
        for _ in 0..count {
            frames.push_back(left);
            frames.push_back(right);
        }
    }

    fn available(&self) -> usize {
        self.frames.lock().unwrap().len() / 2
    }
}

impl LiveInput for FakeInput {
    fn set_wanted(&mut self, wanted: bool) {
        self.wanted.store(wanted, Ordering::Relaxed);
    }
    fn sample_rate(&self) -> Option<u32> {
        self.rate
    }
    fn available(&self) -> usize {
        FakeInput::available(self)
    }
    fn read(&mut self, out: &mut [f32]) -> usize {
        let mut frames = self.frames.lock().unwrap();
        let count = (out.len() / 2).min(frames.len() / 2);
        for value in &mut out[..count * 2] {
            *value = frames.pop_front().unwrap();
        }
        count
    }
    fn skip(&mut self, count: usize) {
        let mut frames = self.frames.lock().unwrap();
        let count = (count * 2).min(frames.len());
        frames.drain(..count);
    }
}

/// The bench: audio track 20 (a node with volume 1 → master) with mode `mode` and a 48 kHz input.
fn rig_with_input(mode: MonitorMode, rate: Option<u32>) -> (Rig, FakeInput) {
    let mut rig = Rig::new();
    let node = rig.add_audio_track(20, MASTER);
    if let NodeKind::Audio { monitor, .. } = rig.node_kind(node) {
        *monitor = mode;
    }
    rig.publish();
    let input = FakeInput::new(rate);
    rig.link.send(DspCommand::SetLiveInput { input: Box::new(input.clone()) }).unwrap();
    (rig, input)
}

fn arm(rig: &Rig, armed: bool, channels: InputChannels) {
    rig.link.set_track_inputs(&[TrackInput { track: 20, armed, channels }]);
}

#[test]
fn track_with_monitor_on_hears_the_input_through_its_fader() {
    let (mut rig, input) = rig_with_input(MonitorMode::On, Some(SR));
    input.push(2_000, 0.5, 0.25);
    let out = rig.render(512);
    assert!(near(Rig::left(&out, 100), 0.5) && near(Rig::right(&out, 100), 0.25), "the input is audible in the master");
    assert!(input.wanted.load(Ordering::Relaxed), "the engine asks to open the input");
    assert_eq!(rig.link.monitor_status().listening, 1);
    assert!(input.available() < 2_000 - 512, "what piled up beyond the margin is dropped: the latency does not grow");
}

#[test]
fn monitor_off_is_silent_and_does_not_let_the_input_pile_up() {
    let (mut rig, input) = rig_with_input(MonitorMode::Off, Some(SR));
    input.push(2_000, 0.5, 0.5);
    let out = rig.render(512);
    assert!(near(Rig::left(&out, 100), 0.0));
    assert_eq!(input.available(), 0, "the input does not pile up while nobody listens");
    assert!(!input.wanted.load(Ordering::Relaxed), "the input is not needed");

    arm(&rig, true, InputChannels::Stereo);
    rig.render(512);
    assert!(input.wanted.load(Ordering::Relaxed), "an armed track keeps the input open (levels, recording)");
    assert_eq!(rig.link.monitor_status().listening, 0, "but does not listen to it");
}

#[test]
fn auto_hears_the_input_only_while_the_track_is_armed() {
    let (mut rig, input) = rig_with_input(MonitorMode::Auto, Some(SR));
    input.push(2_000, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0), "not armed: quiet");

    arm(&rig, true, InputChannels::Stereo);
    input.push(2_000, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.5), "armed: audible");

    arm(&rig, false, InputChannels::Stereo);
    input.push(2_000, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0));
}

#[test]
fn the_input_choice_of_the_track_picks_the_channels() {
    let (mut rig, input) = rig_with_input(MonitorMode::On, Some(SR));
    arm(&rig, false, InputChannels::Right);
    input.push(2_000, 0.5, 0.25);
    let out = rig.render(512);
    assert!(near(Rig::left(&out, 100), 0.25) && near(Rig::right(&out, 100), 0.25), "channel 2 in the center");
}

#[test]
fn a_short_input_is_silence_and_a_counted_loss() {
    let (mut rig, input) = rig_with_input(MonitorMode::On, Some(SR));
    input.push(700, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.5));
    assert_eq!(rig.link.losses().monitor_underruns, 0);
    let out = rig.render(512);
    assert!(near(Rig::left(&out, 100), 0.5), "what arrived sounds");
    assert!(near(Rig::left(&out, 300), 0.0), "the block tail is quiet");
    assert_eq!(rig.link.losses().monitor_underruns, 1);

    input.push(400, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0), "after a shortage, wait for the margin again (larger now)");
    assert_eq!(rig.link.losses().monitor_underruns, 1, "waiting for the margin is not a loss");
}

#[test]
fn a_different_input_rate_disables_monitoring_with_a_clear_status() {
    let (mut rig, input) = rig_with_input(MonitorMode::On, Some(44_100));
    input.push(2_000, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0));
    let status = rig.link.monitor_status();
    assert_eq!((status.listening, status.input_rate, status.output_rate), (1, 44_100, SR));
    assert!(status.problem().unwrap().contains("44100"), "{status:?}");
    assert_eq!(input.available(), 0);

    let (mut closed, _) = rig_with_input(MonitorMode::On, None);
    closed.render(512);
    assert!(closed.link.monitor_status().problem().unwrap().contains("input closed"));
}

#[test]
fn muted_track_mutes_its_monitored_input_too() {
    let (mut rig, input) = rig_with_input(MonitorMode::On, Some(SR));
    let node = rig.project.graph.source_of(20).unwrap().id;
    rig.set_strip(node, 1.0, 0.0, true, false);
    rig.publish();
    input.push(2_000, 0.5, 0.5);
    assert!(near(Rig::left(&rig.render(512), 100), 0.0));
}
