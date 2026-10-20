//! `project.*`, `history.*` and `library.home` methods: the whole project, files, recent projects, autosave, export,
//! undo.

use std::path::PathBuf;

use daw_contracts::{ClearOptions, Controls, ExportRequest, HistoryMark, Node, NodeKind, Project, Track};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::RpcTable;
use crate::params::Path;

/// A short status of the project: for `project.status` and with each update of the window's feed (`feed.rs`).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    name: String,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
    undo_label: Option<String>,
    path: Option<PathBuf>,
    folder: Option<PathBuf>,
}

pub(crate) fn status(c: &Controls) -> Status {
    Status {
        name: c.project.project_name(),
        dirty: c.project.is_dirty(),
        can_undo: c.history.can_undo(),
        can_redo: c.history.can_redo(),
        undo_label: c.history.undo_label(),
        path: c.project.path(),
        folder: c.project.folder(),
    }
}

#[derive(Serialize)]
struct Recent {
    path: PathBuf,
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct Squash {
    /// The mark `history.checkpoint` returned before the change.
    since: HistoryMark,
    /// What the change did, as the user sees it in "Undo …": "AI: drum part", "AI: chorus melody".
    label: String,
}

/// The project for the interface and AI without saved plugin states. Those are opaque bytes (~70 KB of base64 for Surge XT)
/// needed only by the plugins themselves, while AI reads `project.get` in full and the window's feed (`feed.rs`) compares and
/// sends the project after every edit.
pub(crate) fn without_plugin_states(mut project: Project) -> Project {
    project.tracks.iter_mut().for_each(strip_track);
    project.graph.nodes.iter_mut().for_each(strip_node);
    project
}

fn strip_track(track: &mut Track) {
    if let Some(spec) = track.instrument.as_mut() {
        spec.state = None;
    }
}

fn strip_node(node: &mut Node) {
    if let NodeKind::Effect { state, .. } = &mut node.kind {
        *state = None;
    }
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "project.get",
        "Returns the whole project: tracks, clips with notes, the sound graph, automation, samples (without saved plugin states — \
         those are bytes for the plugins themselves). The main way to see the state.",
        |c, ()| Ok(without_plugin_states(c.view.snapshot())),
    );
    table.add("project.status", "Returns a short status: file path, unsaved edits, whether undo is possible.", |c, ()| Ok(status(c)));
    table.add("project.new", "Creates a new empty project without a file.", |c, ()| Ok(c.project.new_project()));
    table.add_flushing(
        "project.create",
        "Creates a new empty project, saves it and makes it current. `path` is the project FILE, not a folder: \
         `<projects folder>/<Name>/<Name>.dawproj`, each project in its own folder named like the file; the projects folder \
         is `profile.projectsFolder` from `preferences.get`, or `<home>/Documents` when that is null (`library.home`). \
         Missing folders are created. The file name without `.dawproj` becomes the project name. An existing file is an error.",
        |c, p: Path| c.project.create(&p.path),
    );
    table.add("project.load", "Opens the project at a path.", |c, p: Path| c.project.load(&p.path));
    table.add_flushing("project.save", "Saves the project to a path.", |c, p: Path| c.project.save(&p.path));
    table.add("project.recent", "Lists recent projects.", |c, ()| {
        let recent = c
            .recent
            .list()
            .into_iter()
            .map(|path| Recent { name: path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned()), path });
        Ok(recent.collect::<Vec<_>>())
    });
    table.add("project.forgetRecent", "Removes a project from the recent list.", |c, p: Path| Ok(c.recent.forget(&p.path)));
    table.add(
        "project.autosave",
        "Returns the autosave file left from the previous run (null if there is none); project.restoreAutosave brings it back.",
        |c, ()| Ok(c.autosave.pending()),
    );
    table.add("project.restoreAutosave", "Restores the project from the autosave.", |c, ()| c.autosave.restore());
    table.add("project.discardAutosave", "Discards the autosave.", |c, ()| Ok(c.autosave.discard()));
    table.add("project.takeWarnings", "Takes the warnings collected while opening the project.", |c, ()| Ok(c.project.take_warnings()));
    table.add(
        "project.clear",
        "Clears the project to start over: removes all tracks, clips, buses, effects (master ones too), connections, \
         automation, markers, folders and the loop in one undo step (history.undo brings everything back). The master, file and \
         project name stay. `keepTempo: true` keeps tempo and time signature (otherwise as in a new project); `removeSamples: true` \
         removes samples too (otherwise they stay in the project).",
        |c, p: ClearOptions| c.clear.clear(p),
    );
    table.add("project.fillDemo", "Fills the project with a starter beat (drums, bass, effects).", |c, ()| c.demo.fill_demo());
    table.add_flushing(
        "project.export",
        "Renders the project to an audio file faster than real time. Required: `path`, `sampleRate` (8000–192000), \
         `bitDepth` (\"int16\"|\"int24\"|\"float32\"), `stems` (also a file per mixer bus next to it: \
         <file>_<node>_<bus>.<extension>), `tailSeconds` (tail after the end: reverb). Optional: `format` — \
         \"wav\" (default) | \"flac\" (lossless, int16/int24 only) | \"mp3\" (`mp3Bitrate`: 128|192|256|320, \
         320 by default; bitDepth is ignored); the file extension is replaced to match the format. `normalize: { lufs, truePeakDb? }` \
         fits the integrated loudness (BS.1770) to `lufs` (−14 streaming, −16 podcasts, −9 club; −40…0) with a single gain; \
         the true peak stays at or below `truePeakDb` (−1 by default; −12…0): if the target needs more, loudness rises \
         only up to the ceiling (no limiter) and the reached value is in the answer; stems get the same gain. `dither` adds \
         triangular noise when rounding (on by default for int16, off for int24). `range` is \"song\" (default: \
         from 0 to the end of the last clip) | \"loop\" (the loop region) | { fromTick, toTick }; clips past the end of the range \
         are silent, the tail is added. Answer: { files (the mix first), frames, loudnessLufs, peakDb (true peak, dBTP) } \
         — the mix loudness and peak after fitting; null means silence.",
        |c, request: ExportRequest| c.exporter.export(&request, &|_| {}),
    );
    table.add(
        "project.summary",
        "A short overview of the project — read it first instead of `project.get`: tempo and length, sections from the \
         markers with the tracks playing in each, every track (instrument, volume/pan/mute/solo, inserted effects and \
         output, where its clips play, note count and pitch range), buses with the tracks they gather, shared effects, \
         automation lanes. Ticks are absolute, spans are [start, end), bars count from 1. Notes, parameters and points \
         are not included: `project.get`.",
        |c, ()| Ok(c.summary.summarize(&c.view.snapshot())),
    );
    table.add("history.undo", "Undoes the last project edit (a compound edit is undone as a whole).", |c, ()| Ok(c.history.undo()));
    table.add("history.redo", "Redoes an undone edit.", |c, ()| Ok(c.history.redo()));
    table.add(
        "history.list",
        "Lists the undo history without changing it: `done` — the steps undo takes back, oldest first (the last one is the \
         next undo); `undone` — the undone steps redo brings back, the next redo first. A step is its undo label; a squashed \
         change is one step under its label (\"AI: drum part\").",
        |c, ()| Ok(c.history.steps()),
    );
    table.add(
        "history.checkpoint",
        "Marks the current point in the undo history and returns it (a number): call it before a change made of many \
         calls, then `history.squash` turns the whole change into one undo step.",
        |c, ()| Ok(c.history.checkpoint()),
    );
    table.add(
        "history.squash",
        "Turns every undo step made after `since` (a mark from `history.checkpoint`) into ONE step named `label`, so the \
         user undoes the whole change with one undo (to throw the change away: squash, then `history.undo`). Answer: \
         false — nothing changed since the mark. Fails if the mark was undone past or another project was opened.",
        |c, p: Squash| c.history.squash(p.since, &p.label),
    );
    // The interface uses it as the default folder for new projects and exports.
    table.add("library.home", "Returns the user's home folder: the default place for new projects and files.", |_, ()| {
        Ok(std::env::var("HOME").unwrap_or_else(|_| "/".to_string()))
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use daw_contracts::{InstrumentSpec, Node, Params, PluginState, Track};

    use super::*;

    #[test]
    fn project_for_the_interface_has_no_plugin_state_bytes() {
        let state = Some(Arc::new(PluginState { component: vec![1; 64], controller: Vec::new() }));
        let mut project = Project::empty();
        project.tracks.push(Track::new(
            1,
            "Synth",
            Some(InstrumentSpec { state: state.clone(), ..InstrumentSpec::new("vst3:surge_xt", Params::new()) }),
            60,
        ));
        let kind = NodeKind::Effect { effect: "vst3:fx".into(), bypass: false, params: Params::new(), state };
        project.graph.nodes.push(Node { id: 9, name: "Effect".into(), kind, x: 0.0, y: 0.0 });
        let json = serde_json::to_string(&without_plugin_states(project)).unwrap();
        assert!(!json.contains("\"state\""), "{json}");
    }
}
