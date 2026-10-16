//! `sounds.*` methods: the sound browser (the project folder and linked folders, `ProjectSounds`) and the links themselves (`SoundSources`).

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Path;

#[derive(Deserialize, JsonSchema)]
struct Dir {
    dir: PathBuf,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("sounds.root", "Returns the project folder, the root of the sound browser (null for a project without a file).", |c, ()| {
        Ok(c.sounds.root())
    });
    table.add(
        "sounds.list",
        "Lists folders and audio files inside the project folder or a linked folder (the rest of the disk is closed).",
        |c, p: Dir| c.sounds.list(&p.dir),
    );
    table.add(
        "sounds.sources",
        "Lists linked sound folders (packs): one list shared by all projects; their files are not copied until a sound is taken into the project.",
        |c, ()| Ok(c.sound_sources.list()),
    );
    table.add("sounds.addSource", "Adds a linked sound folder.", |c, p: Path| c.sound_sources.add(&p.path));
    table.add("sounds.removeSource", "Removes a linked folder (its files are not touched).", |c, p: Path| {
        Ok(c.sound_sources.remove(&p.path))
    });
}
