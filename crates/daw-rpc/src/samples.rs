//! `samples.*` methods: the project's sample catalog (`SampleCatalog`) and the single-sound editor (`SampleEditor`).

use std::path::PathBuf;

use daw_contracts::{Region, SampleId};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::RpcTable;
use crate::params::{Id, Path};

#[derive(Deserialize, JsonSchema)]
struct WaveformParams {
    id: SampleId,
    from: usize,
    to: usize,
    buckets: usize,
}

#[derive(Deserialize, JsonSchema)]
struct Markers {
    id: SampleId,
    markers: Vec<u64>,
}

#[derive(Deserialize, JsonSchema)]
struct RegionExport {
    id: SampleId,
    from: usize,
    to: usize,
    path: PathBuf,
}

#[derive(Deserialize, JsonSchema)]
struct Extract {
    id: SampleId,
    regions: Vec<Region>,
}

pub(crate) fn register(table: &mut RpcTable) {
    table.add(
        "samples.import",
        "Imports an audio file into the project (copied into the project folder); importing it again returns the same sample.",
        |c, p: Path| c.samples.import(&p.path),
    );
    table.add("samples.list", "Lists the project's samples.", |c, ()| Ok(c.samples.samples()));
    table.add("samples.remove", "Removes a sample from the project together with its audio clips (the file on disk stays).", |c, p: Id| {
        c.samples.remove(p.id)
    });
    table.add("samples.waveform", "Returns the waveform envelope of a sample over a frame range.", |c, p: WaveformParams| {
        c.sample_editor.waveform(p.id, p.from, p.to, p.buckets)
    });
    table.add("samples.setMarkers", "Sets a sample's slice markers in frames.", |c, p: Markers| {
        c.sample_editor.set_markers(p.id, p.markers)
    });
    table.add(
        "samples.exportRegion",
        "Saves a region of a sample to a WAV at the given path; the file is not added to the project.",
        |c, p: RegionExport| c.sample_editor.export_region(p.id, p.from, p.to, &p.path),
    );
    table.add(
        "samples.extract",
        "Cuts regions of a sample into separate WAVs in the project folder and adds them as new samples (in one undo step).",
        |c, p: Extract| c.sample_editor.extract_regions(p.id, &p.regions),
    );
}
