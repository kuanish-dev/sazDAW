import type { Id } from "./model";

export interface SampleInfo {
  id: Id;
  name: string;
  path: string;
  frameCount: number;
  sampleRate: number;
  markers: number[];
}

/** Waveform envelope: [min, max] pairs, one per bucket of the range [from, to). */
export interface Waveform {
  frameCount: number;
  sampleRate: number;
  channels: number;
  from: number;
  to: number;
  peaks: [number, number][];
}

export interface LibraryEntry {
  name: string;
  path: string;
  isDir: boolean;
}
