// Audio devices, their settings and status, input level, the master window for the visualizer, a node's window for its
// editor, the live master meter, and recording audio from the input.
import type { BackendError } from "../../i18n/errors";
import type { Id } from "./model";

/** An audio device as the system sees it. */
export interface AudioDevice {
  name: string;
  isDefault: boolean;
  /** Device channel count (0 if the system did not report it). */
  channels: number;
}

export interface AudioDevices {
  outputs: AudioDevice[];
  inputs: AudioDevice[];
}

/** User's choice; null means "default". */
export interface AudioSettings {
  output: string | null;
  input: string | null;
  bufferFrames: number | null;
  /** How many milliseconds earlier recordings are shifted; null means "auto, from the buffer size". */
  recordLatencyMs: number | null;
}

export interface AudioStatus {
  running: boolean;
  outputName: string | null;
  sampleRate: number;
  channels: number;
  /** Supported device buffer sizes; null if unknown. */
  bufferRange: [number, number] | null;
  /** Why the audio is not running (`null` while it is fine). */
  error: BackendError | null;
}

export interface InputLevel {
  left: number;
  right: number;
}

/** How long a read keeps a live capture going, seconds, as `CAPTURE_LEASE_SECONDS` in Rust: the master window, a node's
 *  window and the master meter have no on/off switch, reading them is what keeps them captured. */
export const CAPTURE_LEASE_SECONDS = 10;

/** The latest stretch of the master output for the visualizer (oscilloscope and spectrum). */
export interface MasterWindow {
  /** Mono samples (the middle of the two channels), oldest first: 2048, or none when this read started the capture. */
  samples: Float32Array;
  /** Sample rate of the samples, Hz. */
  sampleRate: number;
}

/** How many of the newest frames a master meter reading carries for the vectorscope, as `VECTOR_FRAMES` in Rust. */
export const VECTOR_FRAMES = 512;
/** At or above this true peak (dBTP) the master clips, as `CLIP_DBTP` in Rust. */
export const CLIP_DBTP = 0;

/**
 * The live master meter (`audio.masterMeter`, `MasterReading` in Rust): loudness in LUFS, K-weighted and gated as the
 * export measures it (ITU-R BS.1770 / EBU R128), peaks in dBTP. A `null` value: too quiet, or not enough measured yet.
 */
export interface MasterReading {
  /** The last 400 ms. */
  momentaryLufs: number | null;
  /** The last 3 s. */
  shortTermLufs: number | null;
  /** Since the reset or playback from the song start, gated: what a streaming service normalizes by. */
  integratedLufs: number | null;
  /** Loudness range since the reset, LU (EBU Tech 3342): how far the short-term loudness moves. */
  rangeLu: number | null;
  /** The highest true peak of the last 400 ms. */
  truePeakDb: number | null;
  /** The highest true peak since the reset, held. */
  maxTruePeakDb: number | null;
  /** The true peak reached `CLIP_DBTP` since the reset. */
  clipped: boolean;
  /** Phase correlation of the last 400 ms: 1 the same in both channels, 0 unrelated, −1 opposite (cancels in mono);
   *  `null` in silence. */
  correlation: number | null;
  /** Seconds measured since the reset. */
  seconds: number;
  /** The newest stereo frames for the vectorscope: left and right interleaved, oldest first, at most `VECTOR_FRAMES`. */
  vector: number[];
  /** Hz; 0 before the first frame. */
  sampleRate: number;
}

/** Frames of sound kept per captured node, as `NODE_WINDOW_FRAMES` in Rust: a 4096-point spectrum. */
export const NODE_WINDOW_FRAMES = 4096;
/** How many nodes the engine captures at once, as `MAX_WATCHED_NODES` in Rust. */
export const MAX_WATCHED_NODES = 16;

/** How far a reader has read a captured node's sound: the `watch` and `end` of its previous window. */
export interface NodeCursor {
  node: Id;
  watch: number;
  end: number;
}

/** The newest stretch of one captured node's sound, for its live picture (the effect visuals). */
export interface NodeWindow {
  node: Id;
  /** Which watch of the node the frames belong to: a node whose capture starts (again: nobody read it for
   *  `CAPTURE_LEASE_SECONDS`, or its slot went to another node) gets a new number and its sound starts from silence; a
   *  cursor counts only within the same watch. */
  watch: number;
  /** Frames captured since the watch began: the last sample is frame `end − 1`. */
  end: number;
  /** What came into the node: before the effect (the same as `output` while bypassed), before the fader for sources,
   *  buses and the master. Mono, oldest first: the frames after the reader's cursor, or the last `NODE_WINDOW_FRAMES`. */
  input: Float32Array;
  /** What the node sent on, over the same frames as `input` (sample `i` of both is the same moment). */
  output: Float32Array;
  /** Sample rate of the samples, Hz. */
  sampleRate: number;
}

/** Which input channels an audio track records and monitors: pair 1–2, only 1, or only 2. */
export type InputChannels = "stereo" | "left" | "right";

/** An audio track's input: whether recording is armed, its channels and their level (0 while the input is closed). */
export interface TrackRecordState {
  track: Id;
  armed: boolean;
  input: InputChannels;
  left: number;
  right: number;
}

/** Status of recording audio from the input (levels are linear peaks since the last poll). */
export interface AudioRecordStatus {
  /** A take is in progress (from start to stop). */
  recording: boolean;
  /** The take hit the length limit: input is closed, audio is saved on stop. */
  full: boolean;
  seconds: number;
  left: number;
  right: number;
  /** Input frames lost (recording could not keep up); > 0 means the audio has gaps. */
  droppedFrames: number;
  /** Tracks with recording armed or an input selected; the rest are unarmed with stereo input. */
  tracks: TrackRecordState[];
  /** Why monitoring is silent; null if everything is fine. */
  monitorProblem: string | null;
}
