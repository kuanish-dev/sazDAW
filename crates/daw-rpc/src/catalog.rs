//! `catalog.get` method: which kinds of instruments, effects and MIDI effects can be created, and with which params.

use daw_contracts::KindInfo;
use serde::Serialize;

use crate::RpcTable;

#[derive(Serialize)]
struct Catalog {
    instruments: Vec<KindInfo>,
    effects: Vec<KindInfo>,
    #[serde(rename = "noteEffects")]
    note_effects: Vec<KindInfo>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "catalog.get",
        "Lists the kinds of instruments, effects and MIDI effects (noteEffects: placed on a track before the instrument, \
         see noteEffects.add) with their params, i.e. what can be created. Param values are in real \
         units (the unit follows the comma in the label: \"Cutoff, Hz\"), within min…max. kind: number is a number, \
         switch is 0/1, sample is a sample id, choice is an option index 0…n-1 from choices.",
        |c, ()| Ok(Catalog { instruments: c.instruments.kinds(), effects: c.effects.kinds(), note_effects: c.note_effect_kinds.kinds() }),
    );
}
