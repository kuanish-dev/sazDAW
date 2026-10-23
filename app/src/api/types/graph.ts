import type { Id, Params } from "./model";
import type { StagePlace } from "./stage";

/** A node's "mixer strip": volume, pan, mute, solo, the utility controls (polarity, width, mono, fine time alignment)
 * and its place on the virtual stage (in JSON these fields sit directly in the node kind). The utility controls work
 * like a console's input section: before the volume and pan. */
export interface Strip {
  volume: number;
  /** −1 left … 1 right; on the stage: the left/right position, wall to wall. */
  pan: number;
  mute: boolean;
  solo: boolean;
  /** Which sides play with their polarity ("phase") inverted. */
  polarity: Polarity;
  /** Stereo width, mid/side, 0 (mono) … 1 (as recorded) … `MAX_WIDTH`. */
  width: number;
  /** Sums the channel to mono whatever the width (which is kept for when mono is off). */
  mono: boolean;
  /** Fine time alignment, ±`MAX_CHANNEL_DELAY_MS`: positive plays the channel later, negative earlier (the rest of the
   * mix waits for it through delay compensation). */
  delayMs: number;
  /** Where the channel stands on the virtual stage; absent — off the stage (plain pan). */
  stage?: StagePlace;
}

/** A channel's polarity invert, per side. */
export interface Polarity {
  invertLeft: boolean;
  invertRight: boolean;
}

/** Upper bound of a channel's stereo width (`daw_contracts::MAX_WIDTH`). */
export const MAX_WIDTH = 2;
/** How far the fine time alignment reaches either way, ms (`daw_contracts::MAX_CHANNEL_DELAY_MS`). */
export const MAX_CHANNEL_DELAY_MS = 10;

/** Input monitoring on an audio track: off = never, on = always, auto = while recording is armed. */
export type MonitorMode = "off" | "on" | "auto";

/** What a graph node does: a source only outputs sound, master only receives it, effects and buses do both. */
export type NodeKind =
  | ({ type: "instrument"; track: Id } & Strip)
  | ({ type: "audio"; track: Id; monitor: MonitorMode } & Strip)
  | { type: "effect"; effect: string; bypass: boolean; params: Params }
  | ({ type: "bus" } & Strip)
  | { type: "master"; volume: number };

export interface GraphNode {
  id: Id;
  name: string;
  kind: NodeKind;
  /** Card position in the node editor. */
  x: number;
  y: number;
}

/** Output of node `from` feeds the input of node `to`; `sidechain` marks an effect's control input.
 * `preFader` is a pre-fader send: sound is taken before the node's volume and pan (unaffected by them), but mute silences it.
 * Only on connections from sources and buses. */
export interface GraphEdge {
  from: Id;
  to: Id;
  level: number;
  sidechain: boolean;
  preFader: boolean;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

/** There is always exactly one master node, and it always has this id. */
export const MASTER_NODE = 0;

/** Whose native plugin window to open: a graph node's effect or a track's instrument. */
export type EditorTarget =
  | { type: "effect"; node: Id }
  | { type: "instrument"; track: Id };
