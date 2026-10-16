//! Transport commands: playback, tempo, meter, metronome, loop, position.

use daw_contracts::{Bpm, Controls, TimeSignature, TransportState};

use super::{Words, bar_tick, number, report};

pub(super) fn run(controls: &Controls, command: &str, words: &Words) -> bool {
    match command {
        "play" => controls.transport.play(),
        "pause" => controls.transport.pause(),
        "stop" => controls.transport.stop(),
        "bpm" => {
            if let Some(value) = number::<f64>(words.arg(1), "bpm") {
                match Bpm::new(value) {
                    Ok(bpm) => report(controls.tempo.set_tempo(bpm)),
                    Err(error) => eprintln!("error: {}: {error}", error.code().code),
                }
            }
        }
        "sig" => match words.arg(1).and_then(|v| v.split_once('/')) {
            Some((n, d)) => match (n.parse(), d.parse()) {
                (Ok(n), Ok(d)) => match TimeSignature::new(n, d) {
                    Ok(signature) => report(controls.tempo.set_time_signature(signature)),
                    Err(error) => eprintln!("error: {}: {error}", error.code().code),
                },
                _ => eprintln!("sig: the format is N/D, e.g. 3/4"),
            },
            None => eprintln!("sig: the format is N/D, e.g. 3/4"),
        },
        "click" => controls.metronome.set_enabled(words.arg(1) == Some("on")),
        "loop" => {
            if words.arg(1) == Some("off") {
                report(controls.clips.set_loop_region(None));
            } else if let (Some(from), Some(to)) = (number::<u32>(words.arg(1), "from bar"), number::<u32>(words.arg(2), "to bar")) {
                report(controls.clips.set_loop_region(Some((bar_tick(controls, from), bar_tick(controls, to)))));
            }
        }
        "pos" => {
            let state = match controls.transport.state() {
                TransportState::Stopped => "stopped",
                TransportState::Playing => "playing",
                TransportState::Paused => "paused",
            };
            let position = controls.transport.position_ticks();
            println!(
                "{state}, tick {position}, {} bpm, project \"{}\"{}",
                controls.tempo.tempo_map().tempo_at(position as f64).get(),
                controls.project.project_name(),
                if controls.project.is_dirty() { " *" } else { "" }
            );
        }
        _ => return false,
    }
    true
}
