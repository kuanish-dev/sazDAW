//! The console root of the app: picks the audio output and runs the command line.
//! `daw-bootstrap` builds the implementations; from here on only contracts are used.
#![allow(clippy::print_stdout, clippy::print_stderr, reason = "console client: printing to the terminal is its job")]

mod commands;

use daw_bootstrap::{SessionConfig, build_session_in, default_state_dir, install_panic_hook};
use std::io::{self, BufRead};

fn main() {
    let session = build_session_in(&SessionConfig::new(default_state_dir()));
    install_panic_hook(session.controls.log.clone());
    match session.startup().open() {
        Some(file) => println!("project: {}", file.display()),
        None => println!("no project: create one with `create PATH/Name.dawproj` or open one with `load PATH`"),
    }
    let controls = session.controls.clone();
    if controls.autosave.pending().is_some() {
        println!("an autosave was found (the `recover` command loads it)");
    }
    let _autosave = session.start_autosave();

    let _audio = match session.start_audio() {
        Ok(audio) => {
            println!("audio: {} Hz, {} ch.", audio.config().sample_rate, audio.config().channels);
            audio
        }
        Err(error) => {
            eprintln!("could not open audio: {error}");
            std::process::exit(1);
        }
    };

    println!("type `help` for the list of commands");
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        match commands::run(&controls, line.trim()) {
            commands::Outcome::Continue => {}
            commands::Outcome::Quit => break,
        }
    }
    // A normal exit: the next start must not mistake it for a plugin crash.
    controls.plugin_crash.end_session();
}
