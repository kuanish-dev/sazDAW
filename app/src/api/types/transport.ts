import type { AutomationWriteMode, Id, TimeSignature } from "./model";

export type TransportState = "stopped" | "playing" | "paused";

export interface MeterReading {
  node: Id;
  left: number;
  right: number;
}

/** How much an effect (compressor, gate, limiter) turned the sound down since the previous tick, in dB (0 or more). */
export interface GainReductionReading {
  node: Id;
  db: number;
}

/** What each band of a band-split effect (multiband compressor, dynamic EQ) did to its band since the previous tick, in
 *  dB, lowest band first: negative a cut, positive a boost (per band the value farthest from 0). */
export interface BandGainsReading {
  node: Id;
  db: number[];
}

export interface RecordState {
  /** Recording is armed. */
  armed: boolean;
  /** Recording is happening right now (armed and transport playing). */
  capturing: boolean;
  /** Note count in the current or last take. */
  takeNotes: number;
  /** Record quantize grid in ticks; 0 = no quantize. */
  quantizeTicks: number;
}

export interface Tick {
  state: TransportState;
  /** Audible position: during playback the backend has already subtracted the total compensation latency (`latencyMs`). */
  positionTicks: number;
  /** The start position (set by a click on the ruler): play from pause/stop and stop return to it; `null` — none. */
  cue: number | null;
  tempo: number;
  signature: TimeSignature;
  /**
   * What playback wraps around now: the loop region, otherwise the song (0 → the song end rounded up to a bar) while the
   * song loop is on and nothing records; `null` — playback runs on.
   */
  wrap: [number, number] | null;
  /** The song loop mode (on by default): at the song end playback jumps back to the start. */
  songLoop: boolean;
  metronome: boolean;
  /** The mono check: the speakers get the master summed to mono. A monitoring mode of this run (not saved, not exported). */
  mono: boolean;
  /** Live automation write mode (a state of this run, not of the project). */
  automationWrite: AutomationWriteMode;
  cpu: number;
  /** Total plugin delay compensation (PDC) latency in ms: how far the master lags the transport; 0 = none. */
  latencyMs: number;
  meters: MeterReading[];
  /** Effects that report gain reduction and were processed since the previous tick (a bypassed one is absent). */
  reductions: GainReductionReading[];
  /** Band-split effects that were processed since the previous tick (a bypassed one is absent). */
  bands: BandGainsReading[];
  record: RecordState;
  /** Sample preview position in file frames; `null` when nothing is playing. */
  preview: number | null;
}
