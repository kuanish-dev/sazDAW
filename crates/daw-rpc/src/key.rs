//! `key.*` methods: the song's key (`KeyControl`). Melodic step lines (`clips.addSteps`) count scale degrees in it.

use daw_contracts::{Scale, ScaleKey};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;

fn minor() -> Scale {
    Scale::Minor
}

#[derive(Deserialize, JsonSchema)]
struct SetKey {
    /// The tonic: 0 is C, 1 C#, 2 D, 3 Eb, 4 E, 5 F, 6 F#, 7 G, 8 Ab, 9 A, 10 Bb, 11 B.
    root: u8,
    /// The scale, minor when omitted.
    #[serde(default = "minor")]
    scale: Scale,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "key.get",
        "Returns the song's key { root, scale } (root 0 is C … 11 is B), or null when it is not set. The same is in project.get in \
         the key field.",
        |c, ()| Ok(c.key.key()),
    );
    table.add(
        "key.set",
        "Sets the song's key: the tonic root (0 is C, 1 C#, 2 D, 3 Eb, 4 E, 5 F, 6 F#, 7 G, 8 Ab, 9 A, 10 Bb, 11 B) and the scale \
         (minor by default; major, minor, dorian, phrygian, lydian, mixolydian, harmonicMinor, pentatonicMajor, pentatonicMinor, \
         chromatic). The key changes no sound: scale degrees in clips.addSteps lines count in it, and it tells a reader what the song \
         is in. Can be undone.",
        |c, p: SetKey| c.key.set_key(Some(ScaleKey { root: p.root, scale: p.scale })),
    );
    table.add("key.clear", "Removes the song's key (step lines can then use only note names). Can be undone.", |c, ()| c.key.set_key(None));
}
