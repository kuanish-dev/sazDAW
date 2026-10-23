import type { Graph, GraphEdge, GraphNode } from "./graph";
import type { Room } from "./stage";

export type Id = number;
export type Params = Record<string, number>;

/** Ticks per quarter note, same as `PPQ` in Rust. */
export const PPQ = 960;
/** Step-input step: a sixteenth note. */
export const STEP_TICKS = PPQ / 4;
/**
 * A 4/4 bar as a fixed measure (limits, drum patterns in the mock), not a song bar: song bars come from the tempo
 * map (`common/tempoMap.ts: barTick`, `barLengthAt`, `snapToBar`).
 */
export const BAR_TICKS = PPQ * 4;
/** Maximum length of a clip's looped content: 64 bars. */
export const MAX_CLIP_LOOP_TICKS = BAR_TICKS * 64;

export interface InstrumentSpec {
  kind: string;
  params: Params;
  /** Frozen: the instrument is not instantiated; its rendered copy plays instead, the audio track with this id. */
  frozen?: Id | null;
}

/** A track: an instrument (or `null` for an audio track) and clips on the timeline. The track's fader, mute and solo live on its graph node. */
export interface Track {
  id: Id;
  name: string;
  instrument: InstrumentSpec | null;
  defaultPitch: number;
  /** Color name from `TRACK_COLORS`; absent means color by row index. */
  color?: TrackColor | null;
  /** The track's folder (`Folder.id`); a folder's tracks are contiguous in the list. */
  folder?: Id | null;
  /** MIDI effects in order: clip notes and live notes pass through them before the instrument. An empty chain is omitted from JSON. */
  noteEffects?: NoteEffectSpec[];
  /** The track's own row height on the timeline, pixels; absent follows Appearance → track height. */
  height?: number | null;
}

/** A MIDI effect on a track: a kind from `Catalog.noteEffects`, its params, and bypass (notes pass straight through). */
export interface NoteEffectSpec {
  id: Id;
  kind: string;
  params: Params;
  bypass: boolean;
}

/** Track and folder colors as names, like `TRACK_COLORS` in Rust; the theme-specific shade is in `common/trackColors.ts`. */
export const TRACK_COLORS = [
  "red", "orange", "amber", "yellow", "lime", "green", "teal", "cyan", "blue", "indigo", "violet", "pink",
] as const;
export type TrackColor = (typeof TRACK_COLORS)[number];

/** Song section marker ("Intro", "Drop"); kept sorted by `tick` in the project. */
export interface Marker {
  id: Id;
  tick: number;
  name: string;
}

/** Track folder: a header above its tracks; when collapsed, their rows are hidden. Does not affect sound. */
export interface Folder {
  id: Id;
  name: string;
  collapsed: boolean;
  color?: TrackColor | null;
}

/** Partial folder edit: an omitted field is unchanged; `color: null` removes the color. */
export interface FolderEdit {
  name?: string;
  collapsed?: boolean;
  color?: TrackColor | null;
}

/** New track description for batch creation (`kind: null` means an audio track). */
export interface NewTrack {
  name: string;
  kind: string | null;
  defaultPitch: number;
  params: Params;
}

export interface NewNote {
  pitch: number;
  startTick: number;
  lengthTicks: number;
  velocity: number;
}

/**
 * A new note clip with its content (`ClipControl::add_note_clip`): one call and one undo step.
 * No length means one bar; no notes means an empty clip; `loopTicks` is the repeated content length (a multiple of the step), absent means auto length.
 * Notes past `loopTicks` are dropped and long ones are trimmed; an error in any note means the clip is not created.
 */
export interface NewNoteClip {
  track: Id;
  startTick: number;
  lengthTicks?: number;
  notes?: NewNote[];
  loopTicks?: number;
}

/** A stored note: `NewNote` fields plus an id (a type intersection, not inheritance). */
export type Note = NewNote & { id: Id };

/** A note clip's notes: times are from the clip start; content of length `loopTicks` repeats. */
export interface NoteContent {
  notes: Note[];
  nextNoteId: number;
  loopTicks: number;
  /** Content length follows the notes (grows and shrinks by bars). */
  autoLength: boolean;
  /** The clip's MIDI controller lanes (at most one per kind); absent means none. Edited via `notes.setControllerLane`. */
  controllers?: ControllerLane[];
}

/** MIDI controller kind: CC 0..119, pitch bend (values −1..1) or channel aftertouch (0..1); CC values are 0..1. */
export type ControllerKind = { type: "cc"; number: number } | { type: "pitchBend" } | { type: "aftertouch" };

/** Controller lane point: tick from the clip content start (like notes); the value holds until the next point (step). */
export interface ControllerPoint {
  tick: number;
  value: number;
}

/** A note clip's controller lane: points sorted by tick, repeating with the content (`loopTicks`). */
export interface ControllerLane {
  kind: ControllerKind;
  points: ControllerPoint[];
}

/** Scale for `notes.fitToScale`, the song key and piano roll row highlighting. */
export type Scale =
  | "major" | "minor" | "dorian" | "phrygian" | "lydian" | "mixolydian" | "harmonicMinor" | "pentatonicMajor" | "pentatonicMinor" | "chromatic";

/** Scale with a tonic: `root` 0…11 (0 = C). */
export interface ScaleKey {
  root: number;
  scale: Scale;
}

/**
 * `notes.groove` template (a table per beat of four sixteenths): eighth or sixteenth swing (`percent`
 * 50…75), laid-back / pushed timing of everything off the beat (`ticks` 0…120), accent on beats (`percent` 100…200).
 */
export type GrooveTemplate =
  | { type: "swing"; grid: "eighth" | "sixteenth"; percent: number }
  | { type: "laidBack"; ticks: number }
  | { type: "pushed"; ticks: number }
  | { type: "accent"; percent: number };

/** `notes.vary` pattern variation: amounts are per-note probabilities; `density` −1…1, the rest 0…1; the same seed gives the same result. */
export interface Variation {
  seed?: number;
  density: number;
  pitch: number;
  rhythm: number;
  velocity: number;
  scale?: ScaleKey;
}

/** Take comp segment: from `startTicks` (from the clip start) until the next segment, take `take` plays. */
export interface CompSegment {
  startTicks: number;
  take: Id;
}

/**
 * Clip content. For audio, `sample` is what the clip plays (the active take); `takes` lists all takes in recording
 * order, including the active one (absent means the clip has a single sound); `comp` stitches pieces of different takes (sorted,
 * the first at 0; absent means the whole clip plays `sample`). Position, length, warp and fades are shared by all takes.
 */
export type ClipContent = ({ type: "notes" } & NoteContent) | { type: "audio"; sample: Id; takes?: Id[]; comp?: CompSegment[] };

export interface Clip {
  id: Id;
  track: Id;
  startTick: number;
  lengthTicks: number;
  content: ClipContent;
  gain: number;
  fadeInTicks: number;
  fadeOutTicks: number;
  /** Curves of the audio clip's fades (`tracks/fades.ts`, the same formulas as the engine). */
  fadeInShape: FadeShape;
  fadeOutShape: FadeShape;
  offsetTicks: number;
  offsetFrames: number;
  /** Fits an audio clip to the project tempo: the audio was recorded at `sourceBpm`; `null` means it plays at its own tempo. */
  warp: Warp | null;
  /** Audio clip pitch shift in semitones (±24) without changing length. */
  pitchSemitones: number;
}

export interface Warp {
  sourceBpm: number;
}

/** Fade curve: every shape rises from silence at the clip edge to full level. */
export type FadeShape = "linear" | "equalPower" | "sCurve" | "exponential";

/**
 * Edit of individual clip properties, as accepted by `clips.edit`/`clips.editMany`. `warp`: `{ sourceBpm }` enables
 * or changes the source tempo (`{}` or `sourceBpm: null` guesses from the file name or length), `null` disables it.
 */
export type ClipEdit = Partial<Pick<Clip, "track" | "startTick" | "lengthTicks" | "gain" | "fadeInTicks" | "fadeOutTicks" | "fadeInShape" | "fadeOutShape" | "pitchSemitones">> & {
  warp?: { sourceBpm?: number | null } | null;
};

export type ParamTarget =
  | { type: "nodeVolume"; node: Id }
  | { type: "nodePan"; node: Id }
  | { type: "nodeWidth"; node: Id }
  | { type: "effectParam"; node: Id; param: string }
  | { type: "instrumentParam"; track: Id; param: string }
  /** Level of the `from → to` connection (send) of this kind; the lane lives as long as the connection does. */
  | { type: "edgeLevel"; from: Id; to: Id; sidechain: boolean }
  /** A channel's depth and height on the virtual stage (each −1…1); the lanes live while it stands on the stage. */
  | { type: "stageDepth"; node: Id }
  | { type: "stageHeight"; node: Id };

/**
 * A MIDI controller knob (CC `cc`, channel `channel` 0..15, absent means any) mapped to parameter `target` (MIDI learn).
 * CC 0..127 is mapped to `min..max` in parameter units (absent means the parameter's range edge).
 */
export interface MidiMapping {
  cc: number;
  channel?: number;
  target: ParamTarget;
  /** The parameter's full range in its units, remembered when the knob is learned: CC 0…127 maps to `min…max`. */
  min: number;
  max: number;
}

/** `midi.mappings` response: mappings to existing parameters and the target being learned (`null` if none). */
export interface MidiMappings {
  mappings: MidiMapping[];
  learning: ParamTarget | null;
}

export interface AutoPoint {
  tick: number;
  value: number;
  /** Shape of the segment to the next point, -1..1 (0 or absent — straight); formula: `automation/automationRules.ts` `curveShape`. */
  curve?: number;
}

/** A shape drawn over a range of a clip's pattern (`automation.drawShape`, pattern ticks); ramps ignore the period. */
export type AutomationShape = "sine" | "triangle" | "saw" | "square" | "rampUp" | "rampDown";

export interface ShapeRequest {
  fromTick: number;
  toTick: number;
  shape: AutomationShape;
  periodTicks: number;
  min: number;
  max: number;
}

/** Live automation writing: off, touch (while the control is held), latch (until playback stops). */
export type AutomationWriteMode = "off" | "touch" | "latch";

/** One parameter's automation: clips by start, never overlapping; between clips the parameter plays its own value. */
export interface AutomationLane {
  id: Id;
  target: ParamTarget;
  clips: AutomationClip[];
}

/** A piece of a lane, like a looped note clip: a pattern of points `0…loopTicks` (from the clip start) repeated every
 * `loopTicks` over `lengthTicks`. Value at a tick: `automation/automationCurve.ts` `clipValueAt`. */
export interface AutomationClip {
  id: Id;
  startTick: number;
  lengthTicks: number;
  loopTicks: number;
  /** Pattern points by time within `0…loopTicks`; empty — the clip does not touch the parameter. */
  points: AutoPoint[];
}

/** Partial automation clip change (`automation.editClip`); an omitted field stays. */
export interface AutomationClipEdit {
  startTick?: number;
  lengthTicks?: number;
  loopTicks?: number;
}

export interface SampleRef {
  id: Id;
  name: string;
  path: string;
  /** Slice markers in file frames, ascending. */
  markers: number[];
}

export interface TimeSignature {
  numerator: number;
  denominator: number;
}

/** Tempo point; `ramp` means a smooth transition from the previous point to this one (linear in ticks), otherwise a step. */
export interface TempoChange {
  tick: number;
  bpm: number;
  ramp: boolean;
}

/** Time signature change: at the start of a bar of the previous signature. */
export interface SignatureChange {
  tick: number;
  signature: TimeSignature;
}

/** The song's tempo map (`daw-contracts::TempoMap`): the first points are at tick 0. Math over it is in `common/tempoMap.ts`. */
export interface TempoMap {
  tempos: TempoChange[];
  signatures: SignatureChange[];
}

export interface Project {
  name: string;
  tempoMap: TempoMap;
  tracks: Track[];
  clips: Clip[];
  /** Playback loop region in ticks. */
  loopRegion: [number, number] | null;
  graph: Graph;
  /** The shared room of the virtual stage; who stands where is in the strips (`Strip.stage`). */
  room: Room;
  automation: AutomationLane[];
  samples: SampleRef[];
  /** Song layout: section markers sorted by tick. */
  markers: Marker[];
  /** Track folders (a folder's position is the position of its tracks in `tracks`). */
  folders: Folder[];
  /** The song's key (`key.set`); `null` when it is not set. */
  key: ScaleKey | null;
  /** MIDI controller knobs mapped to parameters (MIDI learn); absent means no mappings. */
  midiMappings?: MidiMapping[];
  nextId: number;
}

/** A list of entities with ids as it changed (`ListChanges` in Rust): the ids of the whole list in its order and the
 *  entities that changed or appeared. An id no longer there was removed; one not in `changed` is as the copy has it. */
export interface ListChanges<T> {
  ids: Id[];
  changed: T[];
}

/** What changed in the project (`ProjectChanges` in Rust, sent by the project feed): each part that changed, whole;
 *  absent when it did not. The graph comes as `nodes` and `edges`. `core/projectChanges.ts` takes it into the
 *  interface's copy. */
export interface ProjectChanges {
  name?: string;
  tempoMap?: TempoMap;
  tracks?: ListChanges<Track>;
  clips?: ListChanges<Clip>;
  /** `null`: the loop region was removed. */
  loopRegion?: [number, number] | null;
  nodes?: ListChanges<GraphNode>;
  edges?: GraphEdge[];
  room?: Room;
  automation?: ListChanges<AutomationLane>;
  samples?: ListChanges<SampleRef>;
  markers?: ListChanges<Marker>;
  folders?: ListChanges<Folder>;
  /** `null`: the key was removed. */
  key?: ScaleKey | null;
  midiMappings?: MidiMapping[];
  nextId?: number;
}
