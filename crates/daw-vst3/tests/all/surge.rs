//! The instrument host checked on a real plugin: `/Library/Audio/Plug-Ins/VST3/Surge XT.vst3` (a free synth).
//! No plugin — the test reports the reason and finishes without an error. Plays the default (init patch): no presets are loaded.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: a panic with a clear message is the check"
)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::support::{Cache, Effects, Instruments, MemoryLog, SURGE};
use daw_contracts::sync::lock;
use daw_contracts::{AppLog, AudioBlock, Instrument, InstrumentFactory, ParamKind, Params, PluginScanner, StreamConfig};
use daw_vst3::{MAX_EXPOSED_PARAMS, PluginKind, Vst3Plugins, Vst3Scanner};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

/// Tests run one at a time: on first start Surge XT touches the user's shared files and global state,
/// and the app creates plugin instances from a single command thread too. Running them in parallel in tests is needless risk.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    lock(&ONE_AT_A_TIME)
}

/// A folder holding only Surge XT (a link to the bundle): the test does not go through all plugins on the machine.
/// `None` when Surge XT is not installed (the test is skipped).
fn surge_dir(tag: &str) -> Option<PathBuf> {
    if !std::path::Path::new(SURGE).exists() {
        eprintln!("SKIPPED: no {SURGE}");
        return None;
    }
    let dir = std::env::temp_dir().join(format!("daw-vst3-surge-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let link = dir.join("Surge XT.vst3");
    if !link.exists() {
        std::os::unix::fs::symlink(SURGE, &link).unwrap();
    }
    Some(dir)
}

struct Rig {
    factory: Box<dyn InstrumentFactory>,
    log: Arc<MemoryLog>,
    _serial: MutexGuard<'static, ()>,
    /// The bundle opens lazily, when the instrument is created, so the folder with the link is removed only at the end of the test.
    dir: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn rig(tag: &str) -> Option<Rig> {
    let serial = serial();
    let dir = surge_dir(tag)?;
    let log = Arc::new(MemoryLog::default());
    let (effects, instruments) = (Arc::new(Effects::default()), Arc::new(Instruments::default()));
    let plugins = Vst3Plugins::new(vec![dir.clone()], effects.clone(), instruments.clone(), Arc::new(Cache::default()), log.clone());
    let report = plugins.scan().expect("the scan ran");
    assert_eq!((report.effects, report.instruments, report.found), (0, 1, 1), "{report:?}");
    assert!(effects.0.lock().unwrap().is_empty(), "the instrument did not get into effects");
    let factory = instruments.0.lock().unwrap().pop().expect("the instrument is found");
    Some(Rig { factory, log, _serial: serial, dir })
}

fn prepared(factory: &dyn InstrumentFactory) -> Box<dyn Instrument> {
    let mut instrument = factory.create(&Params::new(), None).expect("the instrument is created");
    instrument.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
    instrument
}

/// Renders `blocks` blocks and returns (peak, whether all numbers are finite).
fn render(instrument: &mut dyn Instrument, blocks: usize, frames: usize) -> (f32, bool) {
    let (mut peak, mut finite) = (0.0f32, true);
    let mut buffer = vec![0.0f32; frames * 2];
    for _ in 0..blocks {
        buffer.fill(0.0);
        instrument.render(&mut AudioBlock::new(&mut buffer, 2));
        for value in &buffer {
            finite &= value.is_finite();
            peak = peak.max(value.abs());
        }
    }
    (peak, finite)
}

#[test]
fn scanner_sees_surge_xt_as_an_instrument_with_capped_parameters() {
    let _serial = serial();
    let Some(dir) = surge_dir("scan") else { return };
    let log = Arc::new(MemoryLog::default());
    let outcome = Vst3Scanner::new(vec![dir.clone()], log.clone()).scan();
    std::fs::remove_dir_all(&dir).ok();
    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
    let surge = outcome.plugins.iter().find(|p| p.name == "Surge XT").expect("Surge XT is found");
    assert_eq!(surge.kind, PluginKind::Instrument);
    assert_eq!(surge.vendor, "Surge Synth Team");
    eprintln!("Surge XT: {} parameters in total, {} exposed", surge.params_total, surge.params.len());
    assert!(surge.params_total > MAX_EXPOSED_PARAMS, "expected \"thousands\" of parameters, got {}", surge.params_total);
    assert_eq!(surge.params.len(), MAX_EXPOSED_PARAMS);
    assert!(surge.params.iter().all(|p| p.min <= p.default && p.default <= p.max), "the default is within the real limits");
    assert!(surge.params.iter().all(|p| (0.0..=1.0).contains(&p.default_normalized)));
    for p in surge.params.iter().take(12) {
        eprintln!("  p{}: \"{}\" {:?} {}..{} (default {}) {:?}", p.id, p.label, p.shape, p.min, p.max, p.default, p.choices);
    }
    assert!(log.recent(10).is_empty(), "{:?}", log.recent(10));
}

#[test]
fn factory_describes_kind_label_and_parameters() {
    let Some(rig) = rig("describe") else { return };
    assert_eq!(rig.factory.kind(), "vst3:surge_xt");
    assert_eq!(rig.factory.label().get(), "Surge XT (VST3)");
    let params = rig.factory.describe();
    assert_eq!(params.len(), MAX_EXPOSED_PARAMS);
    assert!(params.iter().all(|p| p.name.starts_with('p') && p.min <= p.default && p.default <= p.max));
    assert!(params.iter().all(|p| (p.kind == ParamKind::Choice) != p.choices.is_empty()), "only a choice has choices");
    assert!(rig.factory.rebuild_on().is_empty());
}

#[test]
fn a_held_note_makes_sound_and_release_fades_out() {
    let Some(rig) = rig("sound") else { return };
    let mut synth = prepared(&*rig.factory);
    // Warm-up: the synth needs a few blocks before a note sounds.
    render(&mut *synth, 4, BLOCK);
    synth.note_on(60, 1.0);
    let (peak, finite) = render(&mut *synth, 100, BLOCK);
    eprintln!("Surge XT: peak with a held note {peak}");
    assert!(finite, "the sound has NaN/inf");
    assert!(peak > 0.001, "silence instead of a note: peak {peak}");

    synth.note_off(60);
    // The init patch release is short but not zero: give it two seconds and look at the tail.
    render(&mut *synth, 400, BLOCK);
    let (tail, finite) = render(&mut *synth, 20, BLOCK);
    assert!(finite);
    assert!(tail < peak * 0.1 && tail < 0.01, "the sound did not fade after note_off: tail {tail}, peak {peak}");
    assert!(rig.log.recent(10).is_empty(), "{:?}", rig.log.recent(10));
}

#[test]
fn all_notes_off_silences_every_held_note_and_does_not_panic_when_idle() {
    let Some(rig) = rig("all-off") else { return };
    let mut synth = prepared(&*rig.factory);
    synth.all_notes_off();
    render(&mut *synth, 2, BLOCK);
    for note in [48, 55, 60, 64] {
        synth.note_on(note, 0.9);
    }
    let (peak, _) = render(&mut *synth, 60, BLOCK);
    assert!(peak > 0.001, "the chord did not sound: {peak}");
    synth.all_notes_off();
    render(&mut *synth, 400, BLOCK);
    let (tail, finite) = render(&mut *synth, 20, BLOCK);
    assert!(finite && tail < 0.01, "tail {tail}");
    synth.all_notes_off();
}

#[test]
fn render_adds_to_the_block_instead_of_overwriting_it() {
    let Some(rig) = rig("add") else { return };
    let mut synth = prepared(&*rig.factory);
    // Without notes the plugin is silent, and a block with a constant value must stay as it was.
    let mut buffer = vec![0.25f32; BLOCK * 2];
    synth.render(&mut AudioBlock::new(&mut buffer, 2));
    assert!(buffer.iter().all(|v| (v - 0.25).abs() < 1e-3), "the block was overwritten");
}

#[test]
fn parameter_change_and_unknown_names_are_safe() {
    let Some(rig) = rig("params") else { return };
    let mut synth = prepared(&*rig.factory);
    let first = rig.factory.describe()[0].name;
    synth.set_param(first, 0.3);
    synth.set_param(first, f32::NAN);
    synth.set_param("no_such_param", 1.0);
    synth.set_param("p4000000000", 1.0);
    synth.note_on(60, 1.0);
    let (_, finite) = render(&mut *synth, 20, BLOCK);
    assert!(finite);
    assert!(rig.log.recent(10).is_empty());
}

#[test]
fn twenty_instances_create_play_and_drop() {
    let Some(rig) = rig("twenty") else { return };
    for round in 0..20 {
        let mut synth = prepared(&*rig.factory);
        synth.note_on(60 + (round % 12) as u8, 1.0);
        let (_, finite) = render(&mut *synth, 10, BLOCK);
        assert!(finite, "instance {round}");
        // A repeated prepare sets the plugin up again correctly.
        synth.prepare(StreamConfig { sample_rate: 44_100, channels: 2 });
        render(&mut *synth, 2, BLOCK);
    }
    assert!(rig.log.recent(10).is_empty(), "{:?}", rig.log.recent(10));
}

#[test]
fn a_block_larger_than_the_internal_buffers_is_rendered_in_chunks() {
    let Some(rig) = rig("big") else { return };
    let mut synth = prepared(&*rig.factory);
    render(&mut *synth, 4, BLOCK);
    synth.note_on(60, 1.0);
    let (peak, finite) = render(&mut *synth, 3, 20_000);
    assert!(finite);
    assert!(peak > 0.001, "silence in a large block: {peak}");
}

#[test]
fn a_flood_of_notes_beyond_queue_capacity_is_survived() {
    let Some(rig) = rig("flood") else { return };
    let mut synth = prepared(&*rig.factory);
    for n in 0..2000u32 {
        synth.note_on((n % 128) as u8, 0.5);
    }
    let (_, finite) = render(&mut *synth, 5, BLOCK);
    assert!(finite);
    synth.all_notes_off();
    render(&mut *synth, 5, BLOCK);
}
