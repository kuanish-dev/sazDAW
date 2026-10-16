//! `preview.*` methods: listening to a project sample or a file from disk in a separate voice (`SamplePreview`).

use daw_contracts::SampleId;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::Path;

#[derive(Deserialize, JsonSchema)]
struct Play {
    id: SampleId,
    from: usize,
    to: usize,
    #[serde(default)]
    looping: bool,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add("preview.play", "Previews a region of a project sample (optionally in a loop).", |c, p: Play| {
        c.preview.play(p.id, p.from, p.to, p.looping)
    });
    table.add("preview.playFile", "Previews an audio file from disk without adding it to the project.", |c, p: Path| {
        c.preview.play_file(&p.path)
    });
    table.add("preview.stop", "Stops the sample preview.", |c, ()| Ok(c.preview.stop()));
}
