//! `tempo.*` methods: the song's tempo map (`TempoControl`) — tempo and time signature with their changes. All edits can be undone.

use daw_contracts::{Bpm, SignatureChange, TempoChange, TimeSignature};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

#[derive(Deserialize, JsonSchema)]
struct Tempo {
    bpm: f64,
}

#[derive(Deserialize, JsonSchema)]
struct Signature {
    numerator: u8,
    denominator: u8,
}

#[derive(Deserialize, JsonSchema)]
struct TempoChanges {
    changes: Vec<TempoChange>,
}

#[derive(Deserialize, JsonSchema)]
struct SignatureChanges {
    changes: Vec<SignatureChange>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "tempo.get",
        "Returns the song's tempo map: tempos are tempo points { tick, bpm, ramp }, signatures are signature changes { tick, signature: \
         { numerator, denominator } }. The first points are at tick 0. The same is in project.get in the tempoMap field.",
        |c, ()| Ok(c.tempo.tempo_map()),
    );
    table.add(
        "tempo.set",
        "Sets the tempo at the song start (the point at tick 0), 20–999 BPM; other tempo changes stay. Can be undone.",
        |c, p: Tempo| c.tempo.set_tempo(Bpm::new(p.bpm)?),
    );
    table.add(
        "tempo.setSignature",
        "Sets the time signature at the song start (numerator 1–32, denominator 1, 2, 4, 8, 16 or 32); other signature changes stay \
         if they still fall on a bar start. Can be undone.",
        |c, p: Signature| c.tempo.set_time_signature(TimeSignature::new(p.numerator, p.denominator)?),
    );
    table.add(
        "tempo.setChanges",
        "Replaces all tempo points of the song in one undo step. changes are by ascending tick, the first at tick 0 (the start tempo). \
         ramp: true makes the tempo glide (linearly in beats) from the previous point to this one; false or absent is a step at this point. \
         Bars count from 1; in 4/4 bar N starts at tick (N−1)×3840. Example \"a 124 BPM song, a breakdown from bar 64 \
         at 128 BPM, a ramp up to 140 by bar 80\": [{\"tick\":0,\"bpm\":124},{\"tick\":241920,\"bpm\":128},\
         {\"tick\":303360,\"bpm\":140,\"ramp\":true}]. To remove all changes: [{\"tick\":0,\"bpm\":124}].",
        |c, p: TempoChanges| c.tempo.set_tempo_changes(p.changes),
    );
    table.add(
        "tempo.setSignatureChanges",
        "Replaces all time signature changes in one undo step. changes are by ascending tick, the first at tick 0; each next one \
         is at a bar start of the previous signature (a bar = numerator × 3840 / denominator ticks). Example \"4/4, from bar 17 \
         two bars of 7/8, then 4/4 again\": [{\"tick\":0,\"signature\":{\"numerator\":4,\"denominator\":4}},\
         {\"tick\":61440,\"signature\":{\"numerator\":7,\"denominator\":8}},\
         {\"tick\":68160,\"signature\":{\"numerator\":4,\"denominator\":4}}].",
        |c, p: SignatureChanges| c.tempo.set_signature_changes(p.changes),
    );
}
