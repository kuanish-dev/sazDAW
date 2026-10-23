//! `ProjectTracks`: the project's tracks — the list, instruments and names; a track's source node in the graph lives
//! and dies with it.

use std::sync::Arc;

use daw_contracts::codes::tracks;
use daw_contracts::{
    ContractError, InstrumentCatalog, InstrumentSpec, MAX_TRACK_HEIGHT, MAX_TRACKS, MIN_TRACK_HEIGHT, MonitorMode, NewTrack, NodeKind,
    Params, Project, Strip, Track, TrackControl, TrackId, check_color, tr,
};

use crate::ProjectService;
use crate::{folder_rules, ops};

/// Sources stand in a column on the left of the graph view, one per track.
const SOURCE_X: f32 = 40.0;
const SOURCE_TOP: f32 = 40.0;
const SOURCE_STEP: f32 = 60.0;

pub struct ProjectTracks {
    project: Arc<ProjectService>,
    instruments: Arc<dyn InstrumentCatalog>,
}

impl ProjectTracks {
    pub fn new(project: Arc<ProjectService>, instruments: Arc<dyn InstrumentCatalog>) -> Self {
        Self { project, instruments }
    }

    /// Checks the description before the edit and builds the instrument: a batch is created whole or not at all.
    fn prepare(&self, track: &NewTrack) -> Result<Option<InstrumentSpec>, ContractError> {
        if track.default_pitch > 127 {
            return Err(tracks::DEFAULT_PITCH_OUT_OF_RANGE.with("pitch", track.default_pitch));
        }
        let Some(kind) = &track.kind else {
            if !track.params.is_empty() {
                return Err(tracks::AUDIO_TRACK_TAKES_NO_PARAMS.err());
            }
            return Ok(None);
        };
        self.spec(kind, &track.params).map(Some)
    }

    /// An instrument of kind `kind`: the defaults with `overrides` on top (clamped to each parameter's range).
    fn spec(&self, kind: &str, overrides: &Params) -> Result<InstrumentSpec, ContractError> {
        let mut params = self.instruments.defaults(kind)?;
        for (name, value) in overrides {
            let param = ops::param_info(self.instruments.kinds(), kind, name, tracks::UNKNOWN_INSTRUMENT_PARAM)?;
            params.insert(name.clone(), param.fit(ops::finite(*value, tracks::PARAM_NOT_A_NUMBER)?));
        }
        Ok(InstrumentSpec::new(kind, params))
    }

    fn add(&self, label: &str, tracks: Vec<NewTrack>) -> Result<Vec<TrackId>, ContractError> {
        let prepared = tracks.into_iter().map(|track| Ok((self.prepare(&track)?, track))).collect::<Result<Vec<_>, ContractError>>()?;
        self.project.edit(label, |project| prepared.into_iter().map(|(instrument, track)| push_track(project, track, instrument)).collect())
    }
}

/// The track and its source node connected to the master.
fn push_track(project: &mut Project, track: NewTrack, instrument: Option<InstrumentSpec>) -> Result<TrackId, ContractError> {
    ops::limit(project.tracks.len(), MAX_TRACKS, tracks::TOO_MANY)?;
    let id = project.alloc_id();
    let strip = Strip::default();
    let kind = if instrument.is_some() {
        NodeKind::Instrument { track: id, strip }
    } else {
        NodeKind::Audio { track: id, strip, monitor: MonitorMode::Off }
    };
    let y = SOURCE_TOP + SOURCE_STEP * project.tracks.len() as f32;
    project.tracks.push(Track::new(id, track.name.clone(), instrument, track.default_pitch));
    ops::add_node_to_master(project, &track.name, kind, SOURCE_X, y)?;
    Ok(id)
}

/// The track name and the label of its source node change together.
fn rename(project: &mut Project, id: TrackId, name: &str) -> Result<(), ContractError> {
    ops::track_mut(project, id)?.name = name.to_string();
    if let Some(node) = ops::source_node(project, id) {
        ops::node_mut(project, node)?.name = name.to_string();
    }
    Ok(())
}

impl TrackControl for ProjectTracks {
    fn tracks(&self) -> Vec<Track> {
        self.project.read(|p| p.tracks.clone())
    }

    fn add_tracks(&self, tracks: Vec<NewTrack>) -> Result<Vec<TrackId>, ContractError> {
        self.add(&tr!("Add tracks", "Добавить дорожки"), tracks)
    }

    fn add_track(&self, name: &str, kind: Option<&str>, default_pitch: u8) -> Result<TrackId, ContractError> {
        let track = NewTrack { name: name.to_string(), kind: kind.map(str::to_string), default_pitch, params: Params::new() };
        let ids = self.add(&tr!("Add track", "Добавить дорожку"), vec![track])?;
        ids.first().copied().ok_or_else(|| tracks::NOT_CREATED.err())
    }

    fn remove_track(&self, id: TrackId) -> Result<(), ContractError> {
        self.project.edit(&tr!("Delete track", "Удалить дорожку"), |project| {
            ops::track(project, id)?;
            if let Some(node) = ops::source_node(project, id) {
                ops::drop_node(project, node);
            }
            project.clips.retain(|c| c.track != id);
            project.tracks.retain(|t| t.id != id);
            ops::drop_instrument_lanes(project, id);
            folder_rules::gather(project);
            Ok(())
        })
    }

    fn rename_track(&self, id: TrackId, name: &str) -> Result<(), ContractError> {
        self.project.edit(&tr!("Rename track", "Переименовать дорожку"), |project| rename(project, id, name))
    }

    fn move_track(&self, id: TrackId, index: usize) -> Result<(), ContractError> {
        self.project.edit(&tr!("Move track", "Переставить дорожку"), |project| {
            let from = project.tracks.iter().position(|t| t.id == id).ok_or_else(|| tracks::NOT_FOUND.with("id", id))?;
            let track = project.tracks.remove(from);
            project.tracks.insert(index.min(project.tracks.len()), track);
            folder_rules::gather(project);
            Ok(())
        })
    }

    fn set_instrument(&self, id: TrackId, kind: &str, params: Params, name: Option<&str>) -> Result<(), ContractError> {
        let spec = self.spec(kind, &params)?;
        self.project.edit(&tr!("Change instrument", "Сменить инструмент"), |project| {
            let track = ops::track_mut(project, id)?;
            let current = track.instrument.as_ref().ok_or_else(|| tracks::NO_INSTRUMENT.with("id", id))?;
            if current.frozen.is_some() {
                return Err(tracks::FROZEN.with("id", id));
            }
            if current.kind == kind && params.is_empty() && name.is_none() {
                return Ok(());
            }
            let state = track.instrument.take().filter(|old| old.kind == kind).and_then(|old| old.state);
            track.instrument = Some(InstrumentSpec { state, ..spec });
            if let Some(name) = name {
                rename(project, id, name)?;
            }
            ops::drop_instrument_lanes(project, id);
            Ok(())
        })
    }

    fn set_monitor(&self, id: TrackId, mode: MonitorMode) -> Result<(), ContractError> {
        self.project.edit(&tr!("Input monitoring", "Прослушивание входа"), |project| {
            let name = ops::track(project, id)?.name.clone();
            let node = ops::source_node(project, id).ok_or_else(|| tracks::NO_SOURCE_NODE.with("id", id))?;
            match &mut ops::node_mut(project, node)?.kind {
                NodeKind::Audio { monitor, .. } => *monitor = mode,
                _ => return Err(tracks::MONITOR_NEEDS_AUDIO_TRACK.with("name", name)),
            }
            Ok(())
        })
    }

    fn set_color(&self, id: TrackId, color: Option<&str>) -> Result<(), ContractError> {
        if let Some(color) = color {
            check_color(color)?;
        }
        self.project.edit(&tr!("Track color", "Цвет дорожки"), |project| {
            ops::track_mut(project, id)?.color = color.map(str::to_string);
            Ok(())
        })
    }

    fn set_height(&self, id: TrackId, height: Option<u16>) -> Result<(), ContractError> {
        if let Some(height) = height
            && !(MIN_TRACK_HEIGHT..=MAX_TRACK_HEIGHT).contains(&height)
        {
            return Err(tracks::HEIGHT_OUT_OF_RANGE.with("height", height).with("min", MIN_TRACK_HEIGHT).with("max", MAX_TRACK_HEIGHT));
        }
        self.project.edit_keyed(&tr!("Track height", "Высота дорожки"), Some(format!("height:{id}")), |project| {
            ops::track_mut(project, id)?.height = height;
            Ok(())
        })
    }

    fn set_instrument_param(&self, id: TrackId, name: &str, value: f32) -> Result<(), ContractError> {
        let value = ops::finite(value, tracks::PARAM_NOT_A_NUMBER)?;
        let kind = self.project.read(|p| ops::instrument_kind(p, id))?;
        let info = ops::param_info(self.instruments.kinds(), &kind, name, tracks::UNKNOWN_INSTRUMENT_PARAM)?;
        let value = info.fit(value);
        self.project.edit_keyed(
            &tr!("Instrument parameter", "Параметр инструмента"),
            Some(format!("param:{id}:{name}")),
            |project| {
                if let Some(instrument) = &mut ops::track_mut(project, id)?.instrument {
                    instrument.params.insert(name.to_string(), value);
                }
                Ok(())
            },
        )
    }
}
