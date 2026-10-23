//! Checked on a real plugin: the dev-only fixture `test-plugins/distortion` (built by `scripts/build-test-plugins.sh`).
//! No plugin — the test reports the reason and finishes without an error.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: a panic with a clear message is the check"
)]

use std::path::PathBuf;
use std::sync::Arc;

use crate::support::{Cache, DISTORTION, Effects, Instruments, MemoryLog};
use daw_contracts::{AppLog, Effect, EffectFactory, PluginScanner, StreamConfig};
use daw_vst3::{ParamShape, Vst3Plugins, Vst3Scanner};

const RATE: u32 = 48_000;

/// The plugin folder, or `None` (the test is skipped).
fn plugin_dir() -> Option<PathBuf> {
    let bundle = PathBuf::from(DISTORTION);
    if bundle.exists() {
        bundle.parent().map(PathBuf::from)
    } else {
        eprintln!("SKIPPED: no {DISTORTION} (build it: scripts/build-test-plugins.sh)");
        None
    }
}

/// The "Distortion" factory from a folder holding only the plugin we need.
fn distortion_factory(log: Arc<MemoryLog>) -> Option<Box<dyn EffectFactory>> {
    let dir = plugin_dir()?;
    let groups = Arc::new(Effects::default());
    let plugins = Vst3Plugins::new(vec![dir], groups.clone(), Arc::new(Instruments::default()), Arc::new(Cache::default()), log);
    plugins.scan().expect("the scan ran");
    let factories = std::mem::take(&mut *groups.0.lock().unwrap());
    factories.into_iter().find(|f| f.label().get() == "Distortion (VST3)")
}

/// Parameter name by title (the label without units: "Drive" for "Drive, dB").
fn param(factory: &dyn EffectFactory, title: &str) -> &'static str {
    let found = factory.describe().into_iter().find(|p| p.label.get().split(", ").next() == Some(title));
    found.unwrap_or_else(|| panic!("no parameter {title}")).name
}

fn prepared(factory: &dyn EffectFactory, settings: &[(&str, f32)]) -> Box<dyn Effect> {
    let params = settings.iter().map(|(label, value)| (param(factory, label).to_string(), *value)).collect();
    let mut effect = factory.create(&params, None).expect("the effect is created");
    effect.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
    effect
}

fn sine_block(start: usize, frames: usize) -> Vec<f32> {
    (start..start + frames)
        .flat_map(|n| {
            let v = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * n as f32 / RATE as f32).sin();
            [v, v]
        })
        .collect()
}

/// Runs a sine through the effect and returns the last `last` blocks (input, output), so parameter smoothing has time to finish.
fn run_sine(effect: &mut dyn Effect, blocks: usize, last: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut input, mut output) = (Vec::new(), Vec::new());
    for block in 0..blocks {
        let source = sine_block(block * 256, 256);
        let mut buffer = source.clone();
        effect.process(&mut buffer, None);
        if block >= blocks - last {
            input.extend(source);
            output.extend(buffer);
        }
    }
    (input, output)
}

#[test]
fn scanner_finds_distortion_with_five_parameters() {
    let Some(dir) = plugin_dir() else { return };
    let log = Arc::new(MemoryLog::default());
    let found = Vst3Scanner::new(vec![dir], log.clone()).scan().plugins;
    let distortion = found.iter().find(|p| p.name == "Distortion").expect("Distortion is found");
    assert_eq!(distortion.vendor, "DAW Test");
    let labels: Vec<_> = distortion.params.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["Drive, dB", "Tone, Hz", "Mix, %", "Output, dB", "Shape"], "units are in the label");
    let by_label = |label: &str| distortion.params.iter().find(|p| p.label == label).unwrap();
    let drive = by_label("Drive, dB");
    assert!((drive.min - 0.0).abs() < 0.06 && (drive.max - 40.0).abs() < 0.06, "{drive:?}");
    let tone = by_label("Tone, Hz");
    assert!((tone.min - 200.0).abs() < 0.5 && (tone.max - 12_000.0).abs() < 0.5 && (tone.default - 6_000.0).abs() < 30.0, "{tone:?}");
    let mix = by_label("Mix, %");
    assert_eq!((mix.min, mix.max, mix.default), (0.0, 100.0, 100.0));
    let output = by_label("Output, dB");
    assert!((output.min + 24.0).abs() < 0.06 && (output.max - 6.0).abs() < 0.06 && (output.default + 6.0).abs() < 0.06, "{output:?}");
    let shape = by_label("Shape");
    assert_eq!((shape.shape, shape.min, shape.max, shape.default), (ParamShape::Choice, 0.0, 2.0, 0.0));
    assert_eq!(shape.choices, ["Soft (tanh)", "Hard (clip)", "Foldback"]);
}

#[test]
fn hard_clip_with_high_drive_limits_and_changes_signal() {
    let log = Arc::new(MemoryLog::default());
    let Some(factory) = distortion_factory(log.clone()) else { return };
    assert_eq!(factory.kind(), "vst3:distortion");
    let mut effect = prepared(&*factory, &[("Drive", 40.0), ("Shape", 1.0), ("Mix", 100.0), ("Output", 6.0)]);
    let (input, output) = run_sine(&mut *effect, 40, 8);
    assert!(output.iter().all(|v| v.is_finite()));
    let peak = output.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 2.1, "peak {peak}");
    let difference: f32 = input.iter().zip(&output).map(|(a, b)| (a - b).abs()).sum::<f32>() / input.len() as f32;
    assert!(difference > 0.1, "the signal barely changed: {difference}");
    assert!(log.recent(10).is_empty(), "log: {:?}", log.recent(10));
}

#[test]
fn mix_zero_passes_input_with_output_gain_only() {
    let log = Arc::new(MemoryLog::default());
    let Some(factory) = distortion_factory(log) else { return };
    let mut effect = prepared(&*factory, &[("Mix", 100.0), ("Output", 6.0)]);
    run_sine(&mut *effect, 10, 1);
    effect.set_param(param(&*factory, "Mix"), 0.0);
    let (input, output) = run_sine(&mut *effect, 40, 4);
    let gain = 10f32.powf(6.0 / 20.0);
    for (i, o) in input.iter().zip(&output) {
        assert!((i * gain - o).abs() < 0.02, "input {i} output {o}");
    }
}

#[test]
fn thousand_blocks_stay_finite_and_report_no_latency() {
    let log = Arc::new(MemoryLog::default());
    let Some(factory) = distortion_factory(log.clone()) else { return };
    let mut effect = prepared(&*factory, &[]);
    assert_eq!(effect.latency_samples(), 0);
    for block in 0..1000 {
        let mut buffer = sine_block(block * 256, 256);
        effect.process(&mut buffer, None);
        assert!(buffer.iter().all(|v| v.is_finite()), "block {block}");
    }
    let mut big = sine_block(0, 20_000);
    effect.process(&mut big, None);
    assert!(big.iter().all(|v| v.is_finite()));
    effect.reset();
    assert!(log.recent(10).is_empty());
}

#[test]
fn twenty_instances_create_prepare_and_drop() {
    let log = Arc::new(MemoryLog::default());
    let Some(factory) = distortion_factory(log.clone()) else { return };
    for round in 0..20 {
        let mut effect = prepared(&*factory, &[("Drive", 20.0)]);
        let mut buffer = sine_block(0, 128);
        effect.process(&mut buffer, None);
        assert!(buffer.iter().all(|v| v.is_finite()), "instance {round}");
        effect.prepare(StreamConfig { sample_rate: 44_100, channels: 2 });
        effect.process(&mut buffer, None);
        drop(effect);
    }
    assert!(log.recent(10).is_empty());
}
