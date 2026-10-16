// Mock backend projects: an empty one (as after "New project") and the starter beat (like `DemoBeat` in `daw-bootstrap`).
// Names are in the language current at creation, as the real backend creates them.
import type { Id, NewNote, Project } from "../api/types";
import { BAR_TICKS, DEFAULT_ROOM, MASTER_NODE, STEP_TICKS } from "../api/types";
import { sourceOfTrack } from "../domain/graph";
import { DEFAULT_VELOCITY } from "../domain/pitch";
import { constantMap } from "../domain/tempoMap";
import { t } from "../i18n";
import { fail } from "./errors";
import type { HandlerSet } from "./handlers/types";
import type { MockWorld } from "./world";

/** An empty project, as the backend creates it: only the master. */
export function emptyProject(): Project {
  return {
    name: t().mock.untitled, tempoMap: constantMap(130, { numerator: 4, denominator: 4 }),
    tracks: [], clips: [], loopRegion: null,
    graph: { nodes: [{ id: MASTER_NODE, name: "Master", kind: { type: "master", volume: 0.8 }, x: 640, y: 160 }], edges: [] },
    room: { ...DEFAULT_ROOM },
    automation: [], samples: [], markers: [], folders: [], nextId: 1,
  };
}

/** A note on sixteenth steps; slightly shorter than the step so neighbouring notes do not merge. */
const stepNote = (pitch: number, step: number, lengthSteps: number): NewNote => ({
  pitch, startTick: step * STEP_TICKS, lengthTicks: lengthSteps * STEP_TICKS - STEP_TICKS / 6, velocity: DEFAULT_VELOCITY,
});

/**
 * The starter beat exactly as in the backend (`daw-bootstrap::demo`): the same steps through the same methods,
 * so ids, node positions and values match by themselves.
 */
export function fillDemo(w: MockWorld, h: HandlerSet) {
  const sourceOf = (track: Id): Id => w.must(sourceOfTrack(w.project.graph, track), fail("TR-911", { id: track })).id;
  const addTrack = (name: string, kind: string, defaultPitch: number): Id => h["tracks.add"]!({ name, kind, defaultPitch }) as Id;
  const addBus = (name: string, x: number, y: number): Id => h["graph.addBus"]!({ name, x, y }) as Id;
  const addEffect = (effect: string, x: number, y: number): Id => h["graph.addEffect"]!({ effect, x, y }) as Id;
  const connect = (from: Id, to: Id, level = 1) => h["graph.connect"]!({ from, to, level, sidechain: false });
  // The beat is written in 4/4 (like `daw-demo`): a bar is the constant `BAR_TICKS`, not the map's bar.
  const fourBarClip = (track: Id, notes: NewNote[]) => h["clips.addNote"]!({ track, startTick: 0, lengthTicks: 4 * BAR_TICKS, notes });

  const names = t().mock.demo;
  h["tempo.set"]!({ bpm: 128 });
  const kick = addTrack(names.kick, "kick", 36);
  const snare = addTrack(names.snare, "snare", 38);
  const hat = addTrack(names.hat, "hat", 42);
  const clap = addTrack(names.clap, "clap", 39);
  const synth = addTrack(names.synth, "synth", 36);

  const drums = addBus(names.drums, 420, 60);
  const bass = addBus(names.bass, 420, 260);
  const reverbBus = addBus(names.reverb, 640, 360);
  const compressor = addEffect("compressor", 220, 60);
  const reverb = addEffect("reverb", 420, 360);
  h["graph.setEffectParam"]!({ id: reverb, name: "mix", value: 1 });
  h["graph.setEffectParam"]!({ id: reverb, name: "room", value: 0.7 });
  h["graph.setVolume"]!({ id: drums, volume: 0.9 });
  h["graph.setVolume"]!({ id: bass, volume: 0.9 });
  h["graph.setVolume"]!({ id: reverbBus, volume: 0.7 });
  connect(compressor, drums);
  connect(compressor, reverb, 0.18);
  connect(reverb, reverbBus);
  for (const track of [kick, snare, hat, clap]) {
    h["graph.disconnect"]!({ from: sourceOf(track), to: MASTER_NODE, sidechain: false });
    connect(sourceOf(track), compressor);
  }
  h["graph.disconnect"]!({ from: sourceOf(synth), to: MASTER_NODE, sidechain: false });
  connect(sourceOf(synth), bass);

  fourBarClip(kick, [0, 4, 8, 12].map((s) => stepNote(36, s, 1)));
  fourBarClip(snare, [4, 12].map((s) => stepNote(38, s, 1)));
  fourBarClip(hat, [2, 6, 10, 14].map((s) => stepNote(42, s, 1)));
  fourBarClip(synth, [stepNote(36, 0, 2), stepNote(36, 3, 2), stepNote(39, 6, 2), stepNote(36, 10, 2), stepNote(34, 12, 3)]);

  const lane = h["automation.addLane"]!({ target: { type: "nodeVolume", node: bass } }) as Id;
  const clip = h["automation.addClip"]!({ lane, startTick: 0, lengthTicks: 15360 }) as Id;
  h["automation.editClip"]!({ lane, clip, loopTicks: 7680 });
  h["automation.setPoints"]!({ lane, clip, points: [{ tick: 0, value: 0.2 }, { tick: 7680, value: 0.9 }] });

  // The built beat is the project's initial state: no edits, nothing to undo.
  w.undoStack = [];
  w.redoStack = [];
  w.dirty = false;
  w.touch();
}
