//! Console commands: a thin layer over the contracts. Word parsing and shared helpers live here, the commands
//! themselves by area (transport, tracks, clips, graph, project, library): each area returns `true` if the command is its.

mod clips;
mod graph;
mod library;
mod project;
mod tracks;
mod transport;

use daw_contracts::{ContractError, Controls};

pub enum Outcome {
    Continue,
    Quit,
}

const HELP: &str = "\
transport : play | pause | stop | bpm N | sig N/D | click on|off | pos | loop FROM_BAR TO_BAR | loop off
tracks    : tracks | kinds | addtr KIND NAME | addaudio NAME | rmtr N
clips     : clips | clip TRACK BAR [BARS] | rmclip N | notes CLIP | step CLIP STEP | clear CLIP
graph     : graph | vol NODE 0..1.5 | pan NODE -1..1 | mute NODE | solo NODE | meters
            bus NAME | fxkinds | fx KIND | rmnode NODE | link FROM TO [LEVEL] [side] | unlink FROM TO [side]
project   : undo | redo | create PATH.dawproj | save PATH | load PATH | recent | new | recover | export PATH.wav [stems]
samples   : import FILE | samples
midi      : midiexp PATH.mid [CLIP] | midiimp CLIP PATH.mid | midi-devices
other     : cpu | help | quit";

/// Prints a contract error (`CODE: message`) and passes on a successful result (the command shows it if needed).
fn checked<T>(result: Result<T, ContractError>) -> Option<T> {
    result.map_err(|error| eprintln!("error: {}: {error}", error.code().code)).ok()
}

/// Prints a contract error; the console does not show a successful result.
fn report<T>(result: Result<T, ContractError>) {
    checked(result);
}

fn number<T: std::str::FromStr>(word: Option<&str>, what: &str) -> Option<T> {
    let parsed = word.and_then(|w| w.parse().ok());
    if parsed.is_none() {
        eprintln!("a number is needed: {what}");
    }
    parsed
}

/// The tick where bar `bar` (from zero) starts, by the project tempo map: bars differ in length when the meter changes.
fn bar_tick(controls: &Controls, bar: u32) -> u32 {
    u32::try_from(controls.tempo.tempo_map().bar_tick(bar)).unwrap_or(u32::MAX)
}

/// Command words: `arg(n)` is word n, `rest(n)` is everything from word n as one line (names and paths with spaces).
struct Words<'a>(Vec<&'a str>);

impl<'a> Words<'a> {
    fn arg(&self, n: usize) -> Option<&'a str> {
        self.0.get(n).copied()
    }

    fn rest(&self, n: usize) -> String {
        self.0.get(n..).map(|w| w.join(" ")).unwrap_or_default()
    }
}

pub fn run(controls: &Controls, line: &str) -> Outcome {
    let words = Words(line.split_whitespace().collect());
    let Some(command) = words.arg(0) else {
        return Outcome::Continue;
    };
    match command {
        "help" => println!("{HELP}"),
        "quit" | "exit" => return Outcome::Quit,
        _ => {
            let handled = transport::run(controls, command, &words)
                || tracks::run(controls, command, &words)
                || clips::run(controls, command, &words)
                || graph::run(controls, command, &words)
                || project::run(controls, command, &words)
                || library::run(controls, command, &words);
            if !handled {
                eprintln!("unknown command, `help` shows the list");
            }
        }
    }
    Outcome::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_bootstrap::build_session;

    /// Console smoke test: commands reach the contracts, and bad input does not crash the program.
    #[test]
    fn console_commands_reach_the_contracts_and_bad_input_does_not_panic() {
        let controls = build_session(false).controls;
        let step = |line: &str| matches!(run(&controls, line), Outcome::Continue);

        assert!(step("bpm 100"));
        assert!((controls.tempo.tempo_map().base_tempo().get() - 100.0).abs() < 1e-9);
        assert!(step("sig 3/4"));
        assert!(step("addtr kick Kick"));
        assert!(step("addaudio Vocals"));
        assert_eq!(controls.tracks.tracks().len(), 2);
        assert!(step("clip 0 1 2"));
        assert_eq!(controls.clips.clips().len(), 1);
        assert!(step("step 0 0"));
        let clip = controls.clips.clips()[0].id;
        assert_eq!(controls.notes.notes(clip).unwrap().len(), 1, "the step is on");
        assert!(step("step 0 0"));
        assert_eq!(controls.notes.notes(clip).unwrap().len(), 0, "the step is off");
        assert!(step("mute 1"), "tracks have mute too, not only buses");
        let source = controls.graph.graph().nodes[1].clone();
        assert!(source.kind.strip().is_some_and(|s| s.mute), "node 1 is the source of the first track: {source:?}");
        assert!(step("solo 1"));
        assert!(controls.graph.graph().nodes[1].kind.strip().is_some_and(|s| s.solo));
        assert!(step("bus Bus"));
        assert!(controls.graph.graph().nodes.iter().any(|n| n.name == "Bus"));
        assert!(step("vol 1 0.5"));
        assert!(step("undo"));
        assert!(step("rmclip 0"));
        assert!(controls.clips.clips().is_empty());
        assert!(step("rmtr 0"));

        for bad in ["", "   ", "bpm", "bpm abc", "sig 7", "rmtr 99", "clip x y", "link 98 99", "nonsense", "vol 0", "midiimp 5 /missing"] {
            assert!(step(bad), "\"{bad}\" must not end the program");
        }
        assert!(matches!(run(&controls, "quit"), Outcome::Quit));
        assert!(matches!(run(&controls, "exit"), Outcome::Quit));
    }
}
