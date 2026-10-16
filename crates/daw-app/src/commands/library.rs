//! Library commands: project samples, MIDI files and devices.

use std::path::PathBuf;

use daw_contracts::Controls;

use super::{Words, checked, clips, number, report};

pub(super) fn run(controls: &Controls, command: &str, words: &Words) -> bool {
    match command {
        "import" => {
            if let Some(info) = checked(controls.samples.import(&PathBuf::from(words.rest(1)))) {
                println!("sample #{} \"{}\", {} frames", info.id, info.name, info.frame_count);
            }
        }
        "samples" => {
            for info in controls.samples.samples() {
                println!("{} \"{}\" {} frames, {} Hz", info.id, info.name, info.frame_count, info.sample_rate);
            }
        }
        "midiexp" => match words.arg(2).and_then(|w| w.parse::<usize>().ok()) {
            Some(index) => {
                if let Some(id) = clips::id_at(controls, index) {
                    report(controls.midi_files.export_clip(id, &PathBuf::from(words.rest(1).split_whitespace().next().unwrap_or(""))));
                }
            }
            None => report(controls.midi_files.export_project(&PathBuf::from(words.rest(1)))),
        },
        "midiimp" => {
            if let Some(id) = number(words.arg(1), "clip").and_then(|i| clips::id_at(controls, i)) {
                report(controls.midi_files.import_into(id, &PathBuf::from(words.rest(2))));
            }
        }
        "midi-devices" => {
            for device in controls.midi_input.devices() {
                println!("{device}");
            }
        }
        _ => return false,
    }
    true
}
