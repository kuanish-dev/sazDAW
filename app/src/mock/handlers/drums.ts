// Drum patterns in the browser without Rust: a small set of the same shape `patterns.list` returns
// (the real library is `crates/daw-patterns/src/data.rs`), and building a part with the same methods as the backend.
// Pattern names and descriptions are backend-made labels, so they are taken in the current language on each request.
import type { DrumPartRequest, DrumPattern, Id, LaneAssignment, LaneTarget, NewNote, PatternRole, PatternSlot } from "../../api/types";
import { BAR_TICKS, STEP_TICKS } from "../../api/types";
import { laneLabel } from "../../drums/drumPart";
import { GM_DRUM_PITCHES, MIDDLE_C } from "../../common/pitch";
import { barIndex, barTick } from "../../common/tempoMap";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

type PatternId = keyof ReturnType<typeof t>["mockEdits"]["drums"]["patterns"];

const pattern = (id: PatternId, style: string, role: PatternRole, bars: number, lanes: Record<string, string>) => ({
  id, style, role, bars, lanes: Object.entries(lanes).map(([lane, grid]) => ({ lane, grid })),
});

const PATTERN_DATA = [
  pattern("rock_basic", "rock", "groove", 1,
    { kick: "X... .... X... ....", snare: ".... X... .... X...", hat: "x.x. x.x. x.x. x.x." }),
  pattern("house_four", "house", "groove", 1,
    { kick: "X... X... X... X...", clap: ".... X... .... X...", hat: ".o.o .o.o .o.o .o.o", openhat: "..X. ..X. ..X. ..X." }),
  pattern("boom_bap", "hiphop", "groove", 1,
    { kick: "X... ..x. x... ....", snare: ".... X... .... X...", hat: "x.x. x.x. x.x. x.x." }),
  pattern("fill_snare_roll", "fills", "fill", 1,
    { kick: "X... .... X... ....", snare: ".... X... oxox xxXX", hat: "x.x. x.x. .... ...." }),
  pattern("fill_tom_cascade", "fills", "fill", 1,
    { kick: "X... .... X... ...X", snare: ".... X... .... ....", tom_hi: ".... .... xx.. ....", tom_mid: ".... .... ..xx ....", tom_lo: ".... .... .... xxX.", hat: "x.x. x.x. .... ...." }),
  pattern("break_stop", "breaks", "break", 1,
    { kick: "X... .... .... ....", crash: "X... .... .... ...." }),
  pattern("intro_crash_start", "intros", "intro", 1,
    { kick: "X... .... X... ....", snare: ".... X... .... X...", hat: "x.x. x.x. x.x. x.x.", crash: "X... .... .... ...." }),
  pattern("ending_crash_stop", "endings", "ending", 1,
    { kick: "X... .... .... ....", snare: "X... .... .... ....", crash: "X... .... .... ...." }),
];

const mockPatterns = (): DrumPattern[] => PATTERN_DATA.map((p) => ({ ...p, ...t().mockEdits.drums.patterns[p.id] }));

/** Hit velocity by grid symbol: `X` accent, `x` normal, `o` ghost. */
const VELOCITY: Record<string, number> = { X: 115, x: 92, o: 52 };

const gmPitch = (lane: string) => GM_DRUM_PITCHES[lane] ?? MIDDLE_C;

function gridNotes(grid: string, pitch: number): NewNote[] {
  const notes: NewNote[] = [];
  [...grid.replace(/[ |]/g, "")].forEach((symbol, step) => {
    const velocity = VELOCITY[symbol];
    if (velocity !== undefined) notes.push({ pitch, startTick: step * STEP_TICKS, lengthTicks: STEP_TICKS, velocity });
  });
  return notes;
}

/** Where a drum's notes go: track and pitch. */
interface LanePlace {
  track: Id;
  pitch: number;
}

export function drumsHandlers(w: MockWorld, h: () => HandlerSet): HandlerSet {
  /**
   * Track and pitch of a drum's notes by its assignment; `null` means the drum is skipped. Missing tracks are created
   * (one per drum per build) and their names go to `createdTracks`.
   */
  const createLaneResolver = (assigned: LaneAssignment[], createdTracks: string[]) => {
    const created = new Map<string, LanePlace>();
    return (lane: string): LanePlace | null => {
      const target: LaneTarget = assigned.find((a) => a.lane === lane)?.target ?? { type: "auto" };
      if (target.type === "skip") return null;
      if (target.type === "track") {
        const mine = w.track(w.project, target.id);
        if (!mine.instrument) throw fail("PT-173", { name: mine.name, lane });
        return { track: mine.id, pitch: target.pitch ?? mine.defaultPitch };
      }
      const made = created.get(lane);
      if (made) return made;
      let place: LanePlace;
      if (target.type === "auto") {
        const found = w.project.tracks.find((t) => t.instrument?.kind === lane);
        if (found) return { track: found.id, pitch: found.defaultPitch };
        createdTracks.push(laneLabel(lane));
        place = { track: h()["tracks.add"]!({ name: laneLabel(lane), kind: lane, defaultPitch: gmPitch(lane) }) as Id, pitch: gmPitch(lane) };
      } else {
        const sample = w.must(w.project.samples.find((s) => s.id === target.id), fail("SM-001", { id: target.id }));
        const name = `${laneLabel(lane)}: ${sample.name}`;
        createdTracks.push(name);
        const ids = h()["tracks.addMany"]!({ tracks: [{ name, kind: "sampler", defaultPitch: MIDDLE_C, params: { sample: sample.id } }] }) as Id[];
        place = { track: ids[0]!, pitch: MIDDLE_C };
      }
      created.set(lane, place);
      return place;
    };
  };

  /** Clips of one part slot from tick `tick`: one clip per drum (a taken place is an error or gets replaced); the part starts at `startBar`. */
  const placeSlot = (
    def: ReturnType<typeof pattern>, slot: PatternSlot, tick: number, startBar: number, replace: boolean, resolve: (lane: string) => LanePlace | null,
  ) => {
    const handlers = h();
    const length = def.bars * slot.repeat * BAR_TICKS;
    const clips: Id[] = [];
    const tracks: Id[] = [];
    for (const { lane, grid } of def.lanes) {
      const place = resolve(lane);
      if (!place) continue;
      tracks.push(place.track);
      const busy = w.project.clips.filter((c) => c.track === place.track && c.startTick < tick + length && tick < c.startTick + c.lengthTicks);
      if (busy.length > 0 && !replace) {
        const name = w.track(w.project, place.track).name;
        throw fail("PT-370", { name, from: startBar, to: barIndex(w.project.tempoMap, tick + length - 1) });
      }
      for (const clip of busy) handlers["clips.remove"]!({ id: clip.id });
      const clip = handlers["clips.addNote"]!({
        track: place.track, startTick: tick, lengthTicks: length, notes: gridNotes(grid, place.pitch), loopTicks: def.bars * BAR_TICKS,
      }) as { id: Id };
      clips.push(clip.id);
    }
    return { clips, tracks, length };
  };

  return {
    "patterns.list": () => mockPatterns(),
    // The browser without Rust has no sound: preview plays nothing but answers like the backend.
    "drums.preview": () => null,
    "drums.stopPreview": () => null,
    // Simplified: the backend does everything as one undo step and checks for taken places up front.
    "drums.build": ({ startBar, slots, replace, lanes = [] }: DrumPartRequest) => {
      const placed = slots.map((slot) => ({ slot, def: PATTERN_DATA.find((p) => p.id === slot.pattern) }));
      const unknown = placed.find((p) => !p.def);
      if (unknown) throw fail("PT-070", { id: unknown.slot.pattern });
      if (placed.length === 0) throw fail("PT-170");
      const createdTracks: string[] = [];
      const resolve = createLaneResolver(lanes, createdTracks);
      const clips: Id[] = [];
      const tracks = new Set<Id>();
      // The start is a bar of the tempo map (as `daw-patterns/src/plan.rs`); patterns keep their own 4/4 bars.
      let tick = barTick(w.project.tempoMap, startBar);
      for (const { slot, def } of placed) {
        const result = placeSlot(def!, slot, tick, startBar, replace, resolve);
        clips.push(...result.clips);
        for (const track of result.tracks) tracks.add(track);
        tick += result.length;
      }
      return { bars: placed.reduce((sum, p) => sum + p.def!.bars * p.slot.repeat, 0), clips, tracks: [...tracks], createdTracks };
    },
  };
}
