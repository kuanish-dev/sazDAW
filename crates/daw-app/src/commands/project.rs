//! Project commands: undo and redo, save and load, autosave, audio export.

use std::path::PathBuf;

use daw_contracts::{Controls, ExportRequest};

use super::{Words, checked, report};

pub(super) fn run(controls: &Controls, command: &str, words: &Words) -> bool {
    match command {
        "undo" => {
            let label = controls.history.undo_label();
            if controls.history.undo() {
                println!("undone: {}", label.unwrap_or_default());
            } else {
                println!("nothing to undo");
            }
        }
        "redo" => {
            if !controls.history.redo() {
                println!("nothing to redo");
            }
        }
        "save" => report(controls.project.save(&PathBuf::from(words.rest(1)))),
        "load" => {
            report(controls.project.load(&PathBuf::from(words.rest(1))));
            for warning in controls.project.take_warnings() {
                eprintln!("warning: {}: {warning}", warning.code().code);
            }
        }
        "new" => controls.project.new_project(),
        "recover" => match controls.autosave.pending() {
            Some(_) => report(controls.autosave.restore()),
            None => println!("no autosave"),
        },
        "create" => report(controls.project.create(&PathBuf::from(words.rest(1)))),
        "recent" => {
            for file in controls.recent.list() {
                println!("{}", file.display());
            }
        }
        "export" => {
            let request = ExportRequest {
                stems: words.arg(2) == Some("stems"),
                ..ExportRequest::new(PathBuf::from(words.arg(1).unwrap_or("export.wav")))
            };
            for file in checked(controls.exporter.export(&request, &|_| {})).map(|done| done.files).unwrap_or_default() {
                println!("written: {}", file.display());
            }
        }
        _ => return false,
    }
    true
}
