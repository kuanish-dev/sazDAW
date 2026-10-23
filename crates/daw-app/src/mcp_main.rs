//! `daw-mcp`: the DAW's MCP server over stdio, in two modes. Everything else (messages, errors) goes to stderr so as
//! not to break the protocol.
//!
//! 1. Windowless (default): builds its own session, reads requests from stdin, answers on stdout.
//!    `daw-mcp [--audio] [--project PATH] [--state-dir DIR]`. `--audio` opens the audio output (needed to hear
//!    `transport_play`); `--project` opens the project (or creates it if missing). Without arguments the project is
//!    empty and silent: the AI works with an in-memory project, and `project_save`, `project_export` produce files.
//! 2. `daw-mcp --attach [--port N]`: a bridge to an ALREADY RUNNING app with a window. No session of its own: requests
//!    go to the server inside the app, and the AI works in the same project that is open on screen.
#![allow(clippy::print_stderr, reason = "stdout belongs to the protocol: all messages for people go to stderr")]

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use daw_bootstrap::{SessionConfig, build_session_in, default_mcp_state_dir, install_panic_hook};
use daw_mcp::{McpServer, port_from_env, relay};

struct Options {
    attach: bool,
    port: u16,
    audio: bool,
    project: Option<PathBuf>,
    state_dir: PathBuf,
}

fn options() -> Result<Options, String> {
    let mut options = Options { attach: false, port: port_from_env(), audio: false, project: None, state_dir: default_mcp_state_dir() };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--attach" => options.attach = true,
            "--port" => options.port = args.next().and_then(|p| p.parse().ok()).ok_or("--port needs a port number")?,
            "--audio" => options.audio = true,
            "--project" => options.project = Some(args.next().ok_or("--project needs a path")?.into()),
            "--state-dir" => options.state_dir = args.next().ok_or("--state-dir needs a folder")?.into(),
            other => {
                return Err(format!(
                    "unknown argument \"{other}\" (available: --attach, --port N, --audio, --project PATH, --state-dir DIR)"
                ));
            }
        }
    }
    Ok(options)
}

fn main() {
    let options = match options() {
        Ok(options) => options,
        Err(text) => {
            eprintln!("daw-mcp: {text}");
            std::process::exit(2);
        }
    };
    if options.attach {
        if let Err(error) = relay(options.port, io::stdin().lock(), io::stdout().lock()) {
            eprintln!("daw-mcp: the bridge stopped: {error}");
            std::process::exit(1);
        }
        return;
    }
    let session = build_session_in(&SessionConfig::new(options.state_dir));
    install_panic_hook(session.controls.log.clone());
    let controls = session.controls.clone();
    if let Some(path) = &options.project {
        let opened = if path.exists() { controls.project.load(path) } else { controls.project.create(path) };
        if let Err(error) = opened {
            eprintln!("daw-mcp: could not open the project {}: {error}", path.display());
            std::process::exit(1);
        }
    }
    let _autosave = session.start_autosave();
    let _audio = if options.audio {
        match session.start_audio() {
            Ok(audio) => Some(audio),
            Err(error) => {
                eprintln!("daw-mcp: could not open audio: {error}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let crash_guard = controls.plugin_crash.clone();
    let server = McpServer::new(controls);
    eprintln!("daw-mcp: ready, tools: {}", server.tool_count());
    let mut out = io::stdout().lock();
    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let Some(reply) = server.handle_line(&line) else { continue };
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            break;
        }
    }
    crash_guard.end_session();
}
