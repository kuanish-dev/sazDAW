//! `ProjectSampleEditor`: everything done with one project sound while it is open — the waveform, slice markers,
//! saving and cutting out regions, measuring its peak and writing a reversed copy.

use std::path::Path;
use std::sync::Arc;

use daw_contracts::codes::samples;
use daw_contracts::{
    AssetStore, ContractError, HistoryControl, MAX_REGIONS, Region, SampleCatalog, SampleData, SampleEditor, SampleId, SampleInfo,
    SampleStore, SampleWriter, Waveform, tr,
};

use crate::ProjectService;

const MAX_MARKERS: usize = 512;
const MAX_BUCKETS: usize = 20_000;

pub struct ProjectSampleEditor {
    project: Arc<ProjectService>,
    catalog: Arc<dyn SampleCatalog>,
    store: Arc<dyn SampleStore>,
    writer: Arc<dyn SampleWriter>,
    assets: Arc<dyn AssetStore>,
}

impl ProjectSampleEditor {
    pub fn new(
        project: Arc<ProjectService>,
        catalog: Arc<dyn SampleCatalog>,
        store: Arc<dyn SampleStore>,
        writer: Arc<dyn SampleWriter>,
        assets: Arc<dyn AssetStore>,
    ) -> Self {
        Self { project, catalog, store, writer, assets }
    }
}

impl SampleEditor for ProjectSampleEditor {
    fn waveform(&self, id: SampleId, from: usize, to: usize, buckets: usize) -> Result<Waveform, ContractError> {
        let data = self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        if buckets == 0 || buckets > MAX_BUCKETS {
            return Err(samples::WAVEFORM_BUCKETS_OUT_OF_RANGE.with("buckets", buckets).with("max", MAX_BUCKETS));
        }
        let frames = data.frame_count();
        let to = to.min(frames);
        let from = from.min(to);
        Ok(Waveform {
            frame_count: frames,
            sample_rate: data.sample_rate,
            channels: data.channels,
            from,
            to,
            peaks: data.peaks(from, to, buckets),
        })
    }

    fn set_markers(&self, id: SampleId, markers: Vec<u64>) -> Result<(), ContractError> {
        let frames = self.store.get(id).map_or(u64::MAX, |d| d.frame_count() as u64);
        if markers.len() > MAX_MARKERS {
            return Err(samples::TOO_MANY_MARKERS.with("max", MAX_MARKERS));
        }
        let mut markers: Vec<u64> = markers.into_iter().filter(|m| *m > 0 && *m < frames).collect();
        markers.sort_unstable();
        markers.dedup();
        self.project.edit_keyed(&tr!("Sample markers", "Метки сэмпла"), Some(format!("markers:{id}")), |project| {
            let sample = project.samples.iter_mut().find(|s| s.id == id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
            sample.markers = markers;
            Ok(())
        })
    }

    fn export_region(&self, id: SampleId, from: usize, to: usize, path: &Path) -> Result<(), ContractError> {
        let data = self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        let frames = data.frame_count();
        if from >= to || from >= frames {
            return Err(samples::REGION_EMPTY.with("from", from).with("to", to).with("frames", frames));
        }
        self.writer.write(path, &data.slice(from, to.min(frames)))
    }

    fn extract_regions(&self, id: SampleId, regions: &[Region]) -> Result<Vec<SampleInfo>, ContractError> {
        let data = self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        let project_file = self.project.path().ok_or_else(|| samples::CUT_NEEDS_SAVED_PROJECT.err())?;
        if regions.is_empty() {
            return Err(samples::NO_REGIONS.err());
        }
        if regions.len() > MAX_REGIONS {
            return Err(samples::TOO_MANY_REGIONS.with("max", MAX_REGIONS));
        }
        let frames = data.frame_count();
        if let Some(empty) = regions.iter().find(|r| r.from >= r.to || r.from >= frames) {
            return Err(samples::REGION_EMPTY.with("from", empty.from).with("to", empty.to).with("frames", frames));
        }
        self.project.begin_group(&tr!("Cut regions", "Вырезать области"));
        let result: Result<Vec<_>, _> = regions.iter().map(|region| self.extract(&data, region, &project_file)).collect();
        if result.is_err() {
            self.project.cancel_group();
        } else {
            self.project.end_group();
        }
        result
    }

    fn peak(&self, id: SampleId, from: usize, to: usize) -> Result<f32, ContractError> {
        let data = self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        let channels = usize::from(data.channels.max(1));
        let to = to.min(data.frame_count());
        let from = from.min(to);
        Ok(data.frames[from * channels..to * channels].iter().fold(0.0f32, |peak, v| peak.max(v.abs())))
    }

    fn reverse_copy(&self, id: SampleId) -> Result<SampleInfo, ContractError> {
        let data = self.store.get(id).ok_or_else(|| samples::NOT_FOUND.with("id", id))?;
        let project_file = self.project.path().ok_or_else(|| samples::REVERSE_NEEDS_SAVED_PROJECT.err())?;
        let name = self.catalog.samples().into_iter().find(|s| s.id == id).map_or_else(|| format!("{id}"), |s| s.name);
        let path = self.assets.new_asset_path(&project_file, &tr!("{} reversed", "{} реверс", name));
        self.writer.write(&path, &reversed(&data))?;
        self.catalog.import(&path)
    }
}

/// The sound played backwards: frames in reverse order, channels within a frame unchanged.
fn reversed(data: &SampleData) -> SampleData {
    let channels = usize::from(data.channels.max(1));
    SampleData {
        sample_rate: data.sample_rate,
        channels: data.channels,
        frames: data.frames.chunks(channels).rev().flatten().copied().collect(),
    }
}

impl ProjectSampleEditor {
    /// One region: its own WAV in the project folder and a new sample from it.
    fn extract(&self, data: &SampleData, region: &Region, project_file: &Path) -> Result<SampleInfo, ContractError> {
        let path = self.assets.new_asset_path(project_file, &region.name);
        self.writer.write(&path, &data.slice(region.from, region.to.min(data.frame_count())))?;
        self.catalog.import(&path)
    }
}
