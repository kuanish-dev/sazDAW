import type { Id, NoteEffectSpec, Project } from "../api/types";
import type { Core } from "../core/core";

export interface NoteEffects {
  /** Appends to the track's chain with default parameters. */
  add(track: Id, kind: string): Promise<void>;
  remove(track: Id, effect: Id): Promise<void>;
  /** Moves to `index` in the chain (0 = first: notes reach it earliest). */
  move(track: Id, effect: Id, index: number): Promise<void>;
  setParam(track: Id, effect: Id, name: string, value: number): Promise<void>;
  setBypass(track: Id, effect: Id, bypass: boolean): Promise<void>;
}

function effectIn(project: Project, track: Id, effect: Id): NoteEffectSpec | undefined {
  return project.tracks.find((t) => t.id === track)?.noteEffects?.find((e) => e.id === effect);
}

export function createNoteEffects(core: Core): NoteEffects {
  const { api } = core;
  return {
    add: (track, kind) => core.calls.exec(api.noteEffects.add(track, kind)),
    remove: (track, effect) => core.calls.exec(api.noteEffects.remove(track, effect)),
    move: (track, effect, index) => core.calls.exec(api.noteEffects.move(track, effect, index)),
    async setParam(track, effect, name, value) {
      core.copy.patchProject((p) => {
        const spec = effectIn(p, track, effect);
        if (spec) spec.params[name] = value;
      });
      await core.calls.exec(api.noteEffects.setParam(track, effect, name, value));
    },
    async setBypass(track, effect, bypass) {
      core.copy.patchProject((p) => {
        const spec = effectIn(p, track, effect);
        if (spec) spec.bypass = bypass;
      });
      await core.calls.exec(api.noteEffects.setBypass(track, effect, bypass));
    },
  };
}
