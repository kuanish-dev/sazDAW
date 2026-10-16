// Catalog of instrument and effect kinds (built-in and discovered VST3) with their parameter descriptions.

/** number: a value in real units (the unit follows the comma in the label); switch: 0/1; sample: a sample id;
 *  choice: option index 0…n-1 into `choices`. */
export type ParamKind = "number" | "switch" | "sample" | "choice";

export interface ParamInfo {
  name: string;
  label: string;
  min: number;
  max: number;
  default: number;
  kind: ParamKind;
  /** Option names of a `choice` parameter, in order; absent for other kinds. */
  choices?: string[];
}

export interface KindInfo {
  kind: string;
  label: string;
  params: ParamInfo[];
}

export interface Catalog {
  instruments: KindInfo[];
  effects: KindInfo[];
  /** MIDI effects: placed on an instrument track before the instrument (arpeggiator, chord). */
  noteEffects: KindInfo[];
}

/** Third-party plugin scan result: counts found (total, effects, instruments) and plugins that failed to load. */
export interface ScanReport {
  found: number;
  effects: number;
  instruments: number;
  failed: { path: string; reason: string }[];
}
