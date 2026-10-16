//! `noteEffects.*` methods: thin adapters over `NoteEffectControl` — a track's MIDI effects (arpeggiator, chord, note
//! repeat).

use daw_contracts::{NoteEffectId, Params, TrackId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Add {
    track: TrackId,
    /// Kind from `catalog.get` → `noteEffects` (`arp` is the arpeggiator, `chord` the chord, `repeat` the note repeat).
    kind: String,
    /// Param values on top of the defaults (the option index for choice).
    #[serde(default)]
    params: Params,
}

#[derive(Deserialize, JsonSchema)]
struct Effect {
    track: TrackId,
    /// MIDI effect id (from `noteEffects.add` or `project.get` → `tracks[].noteEffects[].id`).
    effect: NoteEffectId,
}

#[derive(Deserialize, JsonSchema)]
struct Move {
    track: TrackId,
    effect: NoteEffectId,
    /// Place in the chain: 0 is the first (notes reach it before the others).
    index: usize,
}

#[derive(Deserialize, JsonSchema)]
struct SetParam {
    track: TrackId,
    effect: NoteEffectId,
    name: String,
    value: f32,
}

#[derive(Deserialize, JsonSchema)]
struct SetBypass {
    track: TrackId,
    effect: NoteEffectId,
    bypass: bool,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "noteEffects.add",
        "Adds a MIDI effect to the end of an instrument track's chain (clip notes and live notes pass the chain in order \
         before the instrument): arp is an arpeggiator (held notes one at a time, in steps on the tempo grid), chord builds a chord from each note, \
         repeat plays every held note again and again on the tempo grid (hi-hat rolls from one long note; `ramp` fades or swells them). \
         Params are the defaults plus params. Returns the effect id. What is played gets recorded, not the effects' output.",
        |c, p: Add| c.note_effects.add(p.track, &p.kind, p.params),
    );
    table.add("noteEffects.remove", "Removes a MIDI effect from a track's chain.", |c, p: Effect| c.note_effects.remove(p.track, p.effect));
    table.add(
        "noteEffects.move",
        "Moves a MIDI effect within the chain (0 is the first). Order matters: arpeggiator → chord gives a chord on every step, \
         chord → arpeggiator arpeggiates the chord's notes.",
        |c, p: Move| c.note_effects.move_to(p.track, p.effect, p.index),
    );
    table.add(
        "noteEffects.setParam",
        "Sets a MIDI effect param (the value is clamped to the range from catalog.get; for choice it is the option index).",
        |c, p: SetParam| c.note_effects.set_param(p.track, p.effect, &p.name, p.value),
    );
    table.add("noteEffects.setBypass", "Bypasses a MIDI effect (true: notes pass it unchanged) or enables it again.", |c, p: SetBypass| {
        c.note_effects.set_bypass(p.track, p.effect, p.bypass)
    });
}
