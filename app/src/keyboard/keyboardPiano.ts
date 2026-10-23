import { createSignal } from "solid-js";
import type { LiveNotes } from "./live";
import type { Id } from "../api/types";
import { MAX_OCTAVE, MIN_OCTAVE, OCTAVE_DOWN, OCTAVE_UP, keyToPitch } from "./keyboard";
import { clamp } from "../common/math";
import { LIVE_VELOCITY } from "../common/pitch";

const START_OCTAVE = 4;

export interface KeyboardPiano {
  /** Base octave (changed with `[` and `]`). */
  octave(): number;
  /** Pitches held on the computer keyboard (the piano roll highlights them). */
  pressedPitches(): Set<number>;
  /** Attaches keys to the window; returns a detach function (which releases all held notes). */
  install(): () => void;
}

export function createKeyboardPiano(deps: { selectedTrack: () => Id | null; live: LiveNotes }): KeyboardPiano {
  const [octave, setOctave] = createSignal(START_OCTAVE);
  const [pressedPitches, setPressedPitches] = createSignal<Set<number>>(new Set<number>());

  const install = (): (() => void) => {
    const held = new Map<string, { track: Id; pitch: number }>();
    const sync = () => setPressedPitches(new Set([...held.values()].map((n) => n.pitch)));

    const releaseAll = () => {
      for (const note of held.values()) deps.live.noteOff(note.track, note.pitch);
      held.clear();
      sync();
    };

    const down = (event: KeyboardEvent) => {
      if (event.repeat || event.metaKey || event.ctrlKey || event.altKey) return;
      if ((event.target as HTMLElement).closest("input, select, textarea")) return;
      if (event.code === OCTAVE_DOWN || event.code === OCTAVE_UP) {
        setOctave(clamp(octave() + (event.code === OCTAVE_UP ? 1 : -1), MIN_OCTAVE, MAX_OCTAVE));
        return;
      }
      const track = deps.selectedTrack();
      const pitch = keyToPitch(event.code, octave());
      if (track === null || pitch === null) return;
      event.preventDefault();
      held.set(event.code, { track, pitch });
      sync();
      deps.live.noteOn(track, pitch, LIVE_VELOCITY);
    };

    const up = (event: KeyboardEvent) => {
      const note = held.get(event.code);
      if (!note) return;
      held.delete(event.code);
      sync();
      deps.live.noteOff(note.track, note.pitch);
    };

    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", releaseAll);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", releaseAll);
      releaseAll();
    };
  };

  return { octave, pressedPitches, install };
}
