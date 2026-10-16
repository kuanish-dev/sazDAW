//! `chords.*` methods: the strum patterns and chord parts — thin adapters over `ChordPartControl` and
//! `ChordPartPreview`.

use daw_contracts::{ChordPartRequest, STRUMS};

use crate::RpcTable;

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "chords.strums",
        "Lists the strum patterns of a chord part: id, name and one 4/4 bar of eighth notes (D a down stroke, U an up \
         stroke, X a muted down stroke or \"chuck\", lower case softer, . nothing).",
        |_, ()| Ok(STRUMS.to_vec()),
    );
    table.add(
        "chords.build",
        "Builds a chord part in one undo step: a chord progression strummed with a strum pattern as one note clip on a \
         track (`target`: {\"type\":\"track\",\"id\"} or a new track {\"type\":\"new\",\"kind\":\"guitar\"|\"electric_guitar\"|\
         \"ukulele\"|\"bass_guitar\"|…}). `bars` holds one or two chord symbols per bar ([[\"C\"],[\"G\"],[\"Am\",\"F\"]]); the \
         guitars and the ukulele strum voiced chords, the bass plays roots on the down strokes, other instruments block \
         chords; `repeat` loops the progression.",
        |c, p: ChordPartRequest| c.chord_part.build(p),
    );
    // Preview: the project doesn't change; it loops until `chords.stopPreview` is called.
    table.add(
        "chords.preview",
        "Previews a chord part in a loop without changing the project (same request as chords.build; up to 16 bars).",
        |c, p: ChordPartRequest| c.chord_preview.play(p),
    );
    table.add("chords.stopPreview", "Stops the chord part preview.", |c, (): ()| Ok(c.chord_preview.stop()));
}
