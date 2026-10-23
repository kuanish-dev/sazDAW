//! Track commands: instrument kinds, list, add and remove.

use daw_contracts::{Controls, DEFAULT_PITCH};

use super::{Words, number, report};

/// A track by its number in the `tracks` list (clip commands need it too).
pub(super) fn id_at(controls: &Controls, index: usize) -> Option<u32> {
    let id = controls.tracks.tracks().get(index).map(|t| t.id);
    if id.is_none() {
        eprintln!("no track #{index}");
    }
    id
}

pub(super) fn run(controls: &Controls, command: &str, words: &Words) -> bool {
    match command {
        "kinds" => {
            for kind in controls.instruments.kinds() {
                println!("{:<10} {}", kind.kind, kind.label);
            }
        }
        "tracks" => {
            let graph = controls.graph.graph();
            for (index, track) in controls.tracks.tracks().iter().enumerate() {
                let kind = track.instrument.as_ref().map_or("audio", |i| i.kind.as_str());
                let path = graph.source_of(track.id).map(|source| {
                    let chain = graph.chain(source.id);
                    let mut names: Vec<String> = chain.effects.iter().filter_map(|id| graph.node(*id).map(|n| n.name.clone())).collect();
                    names.push(chain.output.and_then(|id| graph.node(id)).map_or("split".to_string(), |n| n.name.clone()));
                    names.join(" → ")
                });
                println!("{index} {:<10} {:<8} → {}", track.name, kind, path.unwrap_or_default());
            }
        }
        "addtr" => {
            let (Some(kind), name) = (words.arg(1), words.rest(2)) else {
                eprintln!("addtr KIND NAME");
                return true;
            };
            let name = if name.is_empty() { kind.to_string() } else { name };
            report(controls.tracks.add_track(&name, Some(kind), DEFAULT_PITCH));
        }
        "addaudio" => report(controls.tracks.add_track(&words.rest(1), None, DEFAULT_PITCH)),
        "rmtr" => {
            if let Some(id) = number(words.arg(1), "track").and_then(|i| id_at(controls, i)) {
                report(controls.tracks.remove_track(id));
            }
        }
        _ => return false,
    }
    true
}
