//! The project model: plain data, with no logic and no knowledge of implementations.
//! It is serialized as is (JSON) and goes into the audio engine snapshot in full.

use schemars::JsonSchema;
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::codes::notes as codes;
use crate::{
    CompSegment, ContractError, DEFAULT_VOLUME, FadeShape, Graph, MASTER, Node, NodeId, NodeKind, PPQ, Room, ScaleKey, TempoMap, Warp,
};

pub type TrackId = u32;
pub type ClipId = u32;
pub type SampleId = u32;
pub type LaneId = u32;
pub type AutomationClipId = u32;
pub type NoteId = u32;
pub type MarkerId = u32;
pub type FolderId = u32;
pub type NoteEffectId = u32;

/// Length of a 4/4 bar in ticks. Not the song's bar (the tempo map gives that: `TempoMap::bar_tick`, `bar_length_at`),
/// but a fixed measure for things that are 4/4 by definition: drum patterns and the demo (16 steps per bar) and limits
/// (clip content length, recording snap).
pub const BAR_TICKS: u32 = PPQ * 4;
/// Maximum length of a note clip's content: 64 bars.
pub const MAX_CLIP_LOOP_TICKS: u32 = BAR_TICKS * 64;
/// One step (a sixteenth note) in ticks.
pub const STEP_TICKS: u32 = PPQ / 4;

/// Default pitch for step input: middle C (C4).
pub const DEFAULT_PITCH: u8 = 60;
/// Default note velocity (step input, demo).
pub const DEFAULT_VELOCITY: u8 = 100;

/// MIDI velocity (1..=127) as the 0…1 level instruments expect.
pub fn velocity_level(velocity: u8) -> f32 {
    f32::from(velocity) / 127.0
}

/// Instrument or effect parameters: name → value.
pub type Params = BTreeMap<String, f32>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentSpec {
    pub kind: String,
    pub params: Params,
    /// Saved plugin state (a preset from its window); `params` are applied on top of it. `None` means nothing was
    /// saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<std::sync::Arc<crate::PluginState>>,
    /// The instrument is frozen (`TrackBounce::freeze`): the engine does not create it, and the track's sound comes
    /// from its rendered copy, the audio track with this id. `None` means the instrument plays live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen: Option<TrackId>,
}

impl InstrumentSpec {
    pub fn new(kind: impl Into<String>, params: Params) -> Self {
        Self { kind: kind.into(), params, state: None, frozen: None }
    }
}

/// A MIDI effect in a track's chain (see `NoteEffectControl`): a kind from `NoteEffectCatalog` and its parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoteEffectSpec {
    pub id: NoteEffectId,
    pub kind: String,
    pub params: Params,
    /// Bypassed: notes pass through unchanged.
    #[serde(default)]
    pub bypass: bool,
}

/// A project track: an instrument (or audio clips) and clips on the timeline. The track's volume, pan, mute and solo
/// live on its source node in the graph. `instrument: None` means an audio track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub instrument: Option<InstrumentSpec>,
    /// Pitch that step input uses.
    pub default_pitch: u8,
    /// Track color, a name from [`crate::TRACK_COLORS`]; `None` means a color picked by row number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The folder the track is in (`FolderControl`); tracks of one folder are adjacent in the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<FolderId>,
    /// The track's MIDI effects in order: notes pass through them before reaching the instrument (`NoteEffectControl`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub note_effects: Vec<NoteEffectSpec>,
    /// The track's own row height on the timeline, pixels (`TrackControl::set_height`); `None` follows the interface
    /// setting (Appearance → track height). Like the color it is part of how the song is laid out, so it is saved with
    /// the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u16>,
}

impl Track {
    /// A track with no color, outside any folder.
    pub fn new(id: TrackId, name: impl Into<String>, instrument: Option<InstrumentSpec>, default_pitch: u8) -> Self {
        Self { id, name: name.into(), instrument, default_pitch, color: None, folder: None, note_effects: Vec::new(), height: None }
    }

    pub fn is_audio(&self) -> bool {
        self.instrument.is_none()
    }

    /// The instrument the engine must create: `None` for an audio track or a frozen instrument.
    pub fn sounding_instrument(&self) -> Option<&InstrumentSpec> {
        self.instrument.as_ref().filter(|spec| spec.frozen.is_none())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewNote {
    pub pitch: u8,
    pub start_tick: u32,
    pub length_ticks: u32,
    /// 1..=127
    pub velocity: u8,
}

impl NewNote {
    /// Checks that the note fits into clip content of length `limit_ticks`.
    pub fn validate(&self, limit_ticks: u32) -> Result<(), ContractError> {
        if self.pitch > 127 {
            return Err(codes::PITCH_OUT_OF_RANGE.with("pitch", self.pitch));
        }
        if !(1..=127).contains(&self.velocity) {
            return Err(codes::VELOCITY_OUT_OF_RANGE.with("velocity", self.velocity));
        }
        let end = u64::from(self.start_tick) + u64::from(self.length_ticks);
        if self.length_ticks == 0 || end > u64::from(limit_ticks) {
            return Err(codes::NOTE_END_OUT_OF_RANGE.with("end", end).with("max", limit_ticks));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: NoteId,
    pub pitch: u8,
    pub start_tick: u32,
    pub length_ticks: u32,
    pub velocity: u8,
}

impl Note {
    pub fn from_new(id: NoteId, note: NewNote) -> Self {
        Self { id, pitch: note.pitch, start_tick: note.start_tick, length_ticks: note.length_ticks, velocity: note.velocity }
    }

    pub fn end_tick(&self) -> u32 {
        self.start_tick + self.length_ticks
    }
}

/// The notes of a note clip. Note times are relative to the clip start (taking the clip's `offset_ticks` into account);
/// content of length `loop_ticks` repeats for as long as the clip is longer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoteContent {
    pub notes: Vec<Note>,
    pub next_note_id: NoteId,
    /// Length of the repeating content in ticks.
    pub loop_ticks: u32,
    /// The length follows the notes: it grows when a note is placed past the end and shrinks to the last bar with
    /// notes (never shorter than one bar). Setting the length by hand turns this mode off.
    #[serde(default = "default_true")]
    pub auto_length: bool,
    /// The clip's MIDI controller lanes (pitch bend, CC, aftertouch), at most one per kind; they repeat together with
    /// the notes every `loop_ticks`. Edited through `NoteControl::set_controller_lane`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controllers: Vec<ControllerLane>,
}

impl NoteContent {
    /// Empty content one bar long.
    pub fn empty() -> Self {
        Self { notes: Vec::new(), next_note_id: 1, loop_ticks: BAR_TICKS, auto_length: true, controllers: Vec::new() }
    }
}

/// Highest regular MIDI controller (CC) number: 120–127 are channel mode messages (All Notes Off and so on).
pub const MAX_CC_NUMBER: u8 = 119;
/// CC number of the modulation wheel.
pub const CC_MODULATION: u8 = 1;
/// CC number of the sustain pedal (a value ≥ 0.5 means pressed).
pub const CC_SUSTAIN: u8 = 64;

/// What a controller lane sends. JSON: `{ "type": "cc", "number": 0..=119 }`, `{ "type": "pitchBend" }`,
/// `{ "type": "aftertouch" }` (channel pressure for the whole channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ControllerKind {
    Cc { number: u8 },
    PitchBend,
    Aftertouch,
}

impl ControllerKind {
    /// Allowed values: `-1..=1` for pitch bend (0 is the wheel at center), `0..=1` for the rest.
    pub fn range(self) -> (f32, f32) {
        match self {
            ControllerKind::PitchBend => (-1.0, 1.0),
            ControllerKind::Cc { .. } | ControllerKind::Aftertouch => (0.0, 1.0),
        }
    }

    /// A CC number outside `0..=MAX_CC_NUMBER` is `notes::CC_NUMBER_OUT_OF_RANGE`.
    pub fn validate(self) -> Result<(), ContractError> {
        match self {
            ControllerKind::Cc { number } if number > MAX_CC_NUMBER => {
                Err(codes::CC_NUMBER_OUT_OF_RANGE.with("number", number).with("max", MAX_CC_NUMBER))
            }
            _ => Ok(()),
        }
    }
}

/// A controller lane point: the time is relative to the clip content start (like notes), the value is within
/// `ControllerKind::range`. The value holds until the next point (a step, no smooth transition): that is how MIDI
/// messages behave, and dense points give smoothness on their own.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ControllerPoint {
    pub tick: u32,
    pub value: f32,
}

/// A note clip's controller lane: points in ascending `tick` order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ControllerLane {
    pub kind: ControllerKind,
    pub points: Vec<ControllerPoint>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClipContent {
    /// Notes of an instrument track.
    Notes(NoteContent),
    /// An audio file on an audio track. `sample` is what the clip plays (the active take) unless there is a `comp`.
    Audio {
        sample: SampleId,
        /// All of the clip's takes in recording order, including the active one (`TakeControl`); empty means the clip
        /// has a single sound. Takes are whole files: offset (`offset_frames`), warp, pitch, gain and fades are shared
        /// by all.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        takes: Vec<SampleId>,
        /// A comp of pieces from different takes (`TakeControl::set_comp`): segments in ascending order, the first at
        /// 0; empty means the whole clip plays `sample`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        comp: Vec<CompSegment>,
    },
}

impl ClipContent {
    /// A sound with no takes.
    pub fn audio(sample: SampleId) -> Self {
        ClipContent::Audio { sample, takes: Vec::new(), comp: Vec::new() }
    }

    /// The sound the clip plays (`None` for a note clip).
    pub fn sample(&self) -> Option<SampleId> {
        match self {
            ClipContent::Audio { sample, .. } => Some(*sample),
            ClipContent::Notes(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: ClipId,
    pub track: TrackId,
    pub start_tick: u32,
    pub length_ticks: u32,
    pub content: ClipContent,
    /// Audio clip gain (linear).
    pub gain: f32,
    pub fade_in_ticks: u32,
    pub fade_out_ticks: u32,
    /// Curve of the audio clip's fade-in and fade-out (`FadeShape::gain`).
    #[serde(default)]
    pub fade_in_shape: FadeShape,
    #[serde(default)]
    pub fade_out_shape: FadeShape,
    /// Phase of the note content at the clip start (after a split).
    pub offset_ticks: u32,
    /// Offset into the audio file at the clip start (after a split).
    pub offset_frames: u64,
    /// Fits the audio clip to the project tempo (`warp.rs`); `None` means the sound plays at its own tempo.
    #[serde(default)]
    pub warp: Option<Warp>,
    /// Pitch shift of the audio clip in semitones (±[`crate::MAX_PITCH_SEMITONES`]) without changing its length.
    #[serde(default)]
    pub pitch_semitones: f32,
}

impl Clip {
    /// The tick right after the clip's end.
    pub fn end_tick(&self) -> u64 {
        u64::from(self.start_tick) + u64::from(self.length_ticks)
    }

    /// Whether the clip overlaps `[start, end)` (touching edges do not count as overlap).
    pub fn overlaps(&self, start: u64, end: u64) -> bool {
        u64::from(self.start_tick) < end && start < self.end_tick()
    }

    /// Crossfades of an audio clip with its neighbours on the same track among `clips`: `(in, out)` in ticks, the
    /// overlap with the clip that started earlier and with the clip that starts inside this one. Placement rules
    /// (`ClipControl`) guarantee at most two clips at any point and that no clip lies inside another.
    pub fn crossfades(&self, clips: &[Clip]) -> (u32, u32) {
        let (mut fade_in, mut fade_out) = (0u64, 0u64);
        for other in clips {
            if other.id == self.id || other.track != self.track || !other.overlaps(u64::from(self.start_tick), self.end_tick()) {
                continue;
            }
            if other.start_tick < self.start_tick {
                fade_in = fade_in.max(other.end_tick().min(self.end_tick()) - u64::from(self.start_tick));
            } else {
                fade_out = fade_out.max(self.end_tick().min(other.end_tick()) - u64::from(other.start_tick));
            }
        }
        (fade_in as u32, fade_out as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ParamTarget {
    NodeVolume {
        node: NodeId,
    },
    NodePan {
        node: NodeId,
    },
    /// Stereo width of a source or bus, `0..=MAX_WIDTH` (`Strip::width`).
    NodeWidth {
        node: NodeId,
    },
    EffectParam {
        node: NodeId,
        param: String,
    },
    InstrumentParam {
        track: TrackId,
        param: String,
    },
    /// Level of the `from → to` connection (send) of this kind (regular or sidechain), `0..=MAX_LEVEL`.
    /// The lane lives as long as the connection: removing the connection removes the lane.
    EdgeLevel {
        from: NodeId,
        to: NodeId,
        #[serde(default)]
        sidechain: bool,
    },
    /// How far ahead of (positive) or behind (negative) the listener a channel stands on the virtual stage, `-1..=1`
    /// (`StagePlace::depth`; its left/right is `NodePan`). The lane lives as long as the channel stands on the stage.
    StageDepth {
        node: NodeId,
    },
    /// How high a channel stands on the virtual stage, `-1..=1` (`StagePlace::height`). The lane lives as long as the
    /// channel stands on the stage.
    StageHeight {
        node: NodeId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutoPoint {
    pub tick: u32,
    pub value: f32,
    /// Shape of the segment from this point to the next one, `-1..=1`; 0 is a straight line. The formula is
    /// [`crate::curve_shape`] (the engine and the interface draw with the same one).
    #[serde(default)]
    pub curve: f32,
}

impl AutoPoint {
    /// A point with a straight segment after it.
    pub const fn new(tick: u32, value: f32) -> Self {
        Self { tick, value, curve: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutomationLane {
    pub id: LaneId,
    pub target: ParamTarget,
    /// Clips by ascending `start_tick`, never overlapping. Outside clips the lane does not touch the param.
    pub clips: Vec<AutomationClip>,
}

/// A piece of an automation lane, like a looped note clip: a pattern of points `0..=loop_ticks` (relative to the clip
/// start) repeated every `loop_ticks` over `length_ticks`. The value at a tick is [`crate::clip_value_at`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutomationClip {
    pub id: AutomationClipId,
    pub start_tick: u32,
    pub length_ticks: u32,
    /// Length of one pattern repeat, `1..=MAX_CLIP_LOOP_TICKS`.
    pub loop_ticks: u32,
    /// Pattern points in ascending `tick` order within `0..=loop_ticks`; between two points the value follows the
    /// first one's `curve`. Empty: the clip does not touch the param.
    pub points: Vec<AutoPoint>,
}

impl AutomationClip {
    /// A clip whose pattern plays once over it (`loop_ticks = length_ticks`, capped at [`MAX_CLIP_LOOP_TICKS`]).
    pub fn once(id: AutomationClipId, start_tick: u32, length_ticks: u32, points: Vec<AutoPoint>) -> Self {
        Self { id, start_tick, length_ticks, loop_ticks: length_ticks.min(MAX_CLIP_LOOP_TICKS), points }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SampleRef {
    pub id: SampleId,
    pub name: String,
    pub path: PathBuf,
    /// Slice markers in file frames, ascending (placed by hand in the sample editor).
    #[serde(default)]
    pub markers: Vec<u64>,
}

/// A MIDI controller knob (CC `cc` on channel `channel`) mapped to the parameter `target` (the same targets as
/// automation). The CC value 0…127 maps linearly to `min…max` (in parameter units): the parameter's full range,
/// remembered when the knob is learned (volume and connection level `0..=MAX_LEVEL`, pan `-1..=1`, other parameters
/// from the catalog), so turning the knob never consults the catalogs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MidiMapping {
    /// CC number, `0..=MAX_CC_NUMBER`.
    pub cc: u8,
    /// MIDI channel `0..=15`; `None` means any channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<u8>,
    pub target: ParamTarget,
    pub min: f32,
    pub max: f32,
}

/// A song section marker on the timeline ("Intro", "Verse", "Drop"); see `MarkerControl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub id: MarkerId,
    pub tick: u32,
    pub name: String,
}

/// A track folder: a collapsible header over adjacent tracks (see `FolderControl`).
/// Membership is stored on the track itself (`Track::folder`); a folder does not affect the sound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    /// Collapsed: its tracks' rows are hidden on the timeline.
    #[serde(default)]
    pub collapsed: bool,
    /// A name from [`crate::TRACK_COLORS`]; `None` means no color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    /// The song's tempo and meter: tempo points (with ramps) and time signature changes; the first of each is at tick 0
    /// (`TempoControl`).
    #[serde(default)]
    pub tempo_map: TempoMap,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    /// Playback loop region in ticks.
    pub loop_region: Option<(u32, u32)>,
    /// The audio graph: sources (tracks) → effects → buses → master.
    pub graph: Graph,
    /// The shared room of the virtual stage (`StageControl`); who stands where is in the strips (`Strip::stage`).
    pub room: Room,
    pub automation: Vec<AutomationLane>,
    pub samples: Vec<SampleRef>,
    /// Song structure: section markers in ascending `tick` order.
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// Track folders (order does not matter: a folder's position is that of its tracks in `tracks`).
    #[serde(default)]
    pub folders: Vec<Folder>,
    /// The song's key (`KeyControl`); `None` when it is not set.
    pub key: Option<ScaleKey>,
    /// MIDI controller knobs mapped to parameters (MIDI learn, `MidiMappingControl`). Stored in the project rather than
    /// in preferences: the targets are this project's nodes, tracks and parameters (their ids mean something else
    /// elsewhere).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_mappings: Vec<MidiMapping>,
    /// Id counter shared by all project entities.
    pub next_id: u32,
}

impl Project {
    /// An empty project: just the master.
    pub fn empty() -> Self {
        Self {
            name: crate::tr!("Untitled", "Без названия"),
            tempo_map: TempoMap::default(),
            tracks: Vec::new(),
            clips: Vec::new(),
            loop_region: None,
            graph: Graph {
                nodes: vec![Node {
                    id: MASTER,
                    name: "Master".into(),
                    kind: NodeKind::Master { volume: DEFAULT_VOLUME },
                    x: 640.0,
                    y: 160.0,
                }],
                edges: Vec::new(),
            },
            room: Room::default(),
            automation: Vec::new(),
            samples: Vec::new(),
            markers: Vec::new(),
            folders: Vec::new(),
            key: None,
            midi_mappings: Vec::new(),
            next_id: 1,
        }
    }

    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    /// The song end: the tick right after the last note or audio clip, 0 for an empty arrangement. Automation clips do
    /// not count: they make no sound of their own. The export's "whole song" and the level analysis end here too.
    pub fn song_end(&self) -> u64 {
        self.clips.iter().map(Clip::end_tick).max().unwrap_or(0)
    }

    /// Where the song loop jumps back to the start (`TransportControl::set_song_end`): the song end rounded up to a
    /// whole bar of the tempo map, so the looped song keeps its beat. `None` for an empty arrangement.
    pub fn song_loop_end(&self) -> Option<u64> {
        let end = self.song_end();
        if end == 0 {
            return None;
        }
        let bar_start = self.tempo_map.bar_start(end as f64);
        if bar_start >= end as f64 {
            return Some(end);
        }
        // Signature changes sit on bar starts, so the bar holding the end keeps its signature to its own end.
        let bar = self.tempo_map.signature_at(bar_start).signature.ticks_per_bar();
        Some((bar_start + bar).round() as u64)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(id: ClipId, start: u32, length: u32) -> Clip {
        Clip {
            id,
            track: 1,
            start_tick: start,
            length_ticks: length,
            content: ClipContent::audio(1),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        }
    }

    #[test]
    fn controller_lanes_use_the_exact_wire_and_are_omitted_when_empty() {
        let lane = ControllerLane { kind: ControllerKind::Cc { number: 1 }, points: vec![ControllerPoint { tick: 0, value: 0.5 }] };
        let json = serde_json::to_value(&lane).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": { "type": "cc", "number": 1 }, "points": [{ "tick": 0, "value": 0.5 }] }));
        for (wire, kind) in [("pitchBend", ControllerKind::PitchBend), ("aftertouch", ControllerKind::Aftertouch)] {
            assert_eq!(serde_json::from_value::<ControllerKind>(serde_json::json!({ "type": wire })).unwrap(), kind);
        }
        let empty = serde_json::to_value(NoteContent::empty()).unwrap();
        assert!(empty.get("controllers").is_none(), "{empty}");
        let old: NoteContent = serde_json::from_value(serde_json::json!({ "notes": [], "nextNoteId": 1, "loopTicks": 3840 })).unwrap();
        assert!(old.controllers.is_empty());
        assert!(ControllerKind::Cc { number: MAX_CC_NUMBER + 1 }.validate().is_err());
    }

    #[test]
    fn note_effects_are_omitted_when_empty_and_old_tracks_open_without_them() {
        let track = Track::new(1, "Synth", None, 60);
        let json = serde_json::to_value(&track).unwrap();
        assert!(json.get("noteEffects").is_none(), "{json}");
        let spec = NoteEffectSpec { id: 7, kind: "arp".into(), params: Params::new(), bypass: false };
        let with = Track { note_effects: vec![spec], ..track };
        let json = serde_json::to_value(&with).unwrap();
        assert_eq!(json["noteEffects"][0], serde_json::json!({ "id": 7, "kind": "arp", "params": {}, "bypass": false }));
        let old: NoteEffectSpec = serde_json::from_value(serde_json::json!({ "id": 7, "kind": "arp", "params": {} })).unwrap();
        assert!(!old.bypass);
        assert_eq!(serde_json::from_value::<Track>(json).unwrap(), with);
    }

    #[test]
    fn crossfades_are_the_overlaps_with_neighbours() {
        let clips = [audio(1, 0, 1000), audio(2, 800, 1000), audio(3, 1700, 500), audio(4, 5000, 100)];
        assert_eq!(clips[0].crossfades(&clips), (0, 200));
        assert_eq!(clips[1].crossfades(&clips), (200, 100));
        assert_eq!(clips[2].crossfades(&clips), (100, 0));
        assert_eq!(clips[3].crossfades(&clips), (0, 0), "touching or distant clips get no crossfade");
        let mut other_track = audio(5, 900, 1000);
        other_track.track = 2;
        assert_eq!(other_track.crossfades(&clips), (0, 0));
    }

    #[test]
    fn the_song_ends_after_the_last_clip_and_its_loop_at_the_end_of_that_bar() {
        let mut project = Project::empty();
        assert_eq!((project.song_end(), project.song_loop_end()), (0, None), "an empty arrangement has no song to loop");

        project.clips = vec![audio(1, 0, 1000), audio(2, 3840, 3840)];
        assert_eq!(project.song_end(), 7680);
        assert_eq!(project.song_loop_end(), Some(7680), "an end on a bar line stays");

        project.clips.push(audio(3, 7000, 1000));
        assert_eq!(project.song_end(), 8000);
        assert_eq!(project.song_loop_end(), Some(11_520), "a clip ending inside bar 3 loops at the end of that bar");

        let three_four = crate::SignatureChange { tick: 0, signature: crate::TimeSignature::new(3, 4).unwrap() };
        project.tempo_map = project.tempo_map.with_signatures(vec![three_four]).unwrap();
        assert_eq!(project.song_loop_end(), Some(8640), "bars follow the tempo map: 3/4 bars are 2880 ticks");

        project.automation = vec![AutomationLane {
            id: 9,
            target: ParamTarget::NodeVolume { node: MASTER },
            clips: vec![AutomationClip::once(10, 0, 40_000, Vec::new())],
        }];
        assert_eq!(project.song_end(), 8000, "automation makes no sound of its own and does not lengthen the song");
    }
}
