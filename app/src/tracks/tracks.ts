import type { Id, MonitorMode, TrackColor } from "../api/types";
import { instrumentInfo } from "../common/catalog";
import { defaultPitchFor, nextTrackName } from "./trackRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import type { ParamTouch } from "../automation/paramTouch";

export interface Tracks {
  /** The "+ Track" button: name from the catalog ("Synth", "Synth 2", …), step input pitch by instrument kind; selects the track. */
  create(kind: string | null): Promise<Id | undefined>;
  remove(id: Id): Promise<void>;
  rename(id: Id, name: string): Promise<void>;
  /** Moves the track to `index` (0 — top); timeline and header order come from the project. */
  move(id: Id, index: number): Promise<void>;
  /** Track instrument parameter; the value shows immediately. */
  setParam(id: Id, name: string, value: number): Promise<void>;
  /** Audio track input monitoring: the mode shows immediately, a backend refusal as a toast. */
  setMonitor(id: Id, mode: MonitorMode): Promise<void>;
  /** Track color from the palette (`null` — by row number): shows immediately, a refusal as a toast. */
  setColor(id: Id, color: TrackColor | null): Promise<void>;
  /** While the header's edge is dragged: the row takes `height` on screen only (`null` — the setting's); `setHeight` saves it. */
  resize(id: Id, height: number | null): void;
  /** The track's own row height (`null` — back to the interface setting): shows immediately, one undo step. */
  setHeight(id: Id, height: number | null): Promise<void>;
}

export function createTracks(core: Core, views: Views, touch: ParamTouch): Tracks {
  const { api } = core;

  const resize: Tracks["resize"] = (id, height) => {
    core.copy.patchProject((p) => {
      const track = p.tracks.find((t) => t.id === id);
      if (track) track.height = height;
    });
  };

  /** New track (`kind: null` — audio) with a given name; selects it. Returns the id or `undefined` on failure. */
  const add = async (name: string, kind: string | null, defaultPitch: number) => {
    const id = await core.calls.run(api.tracks.add(name, kind, defaultPitch));
    if (id !== undefined) await views.showCreated({ track: id });
    return id;
  };

  return {
    async create(kind) {
      const label = kind === null ? t().tracks.audioName : instrumentInfo(core.copy.catalog(), kind)?.label;
      if (label === undefined) return undefined;
      const count = core.copy.project.tracks.filter((t) => (t.instrument?.kind ?? null) === kind).length;
      return add(nextTrackName(label, count), kind, defaultPitchFor(kind));
    },
    remove: (id) => core.calls.exec(api.tracks.remove(id)),
    rename: (id, name) => core.calls.exec(api.tracks.rename(id, name)),
    move: (id, index) => core.calls.exec(api.tracks.move(id, index)),
    async setParam(id, name, value) {
      touch.moved({ type: "instrumentParam", track: id, param: name }, value);
      core.copy.patchProject((p) => {
        const instrument = p.tracks.find((t) => t.id === id)?.instrument;
        if (instrument) instrument.params[name] = value;
      });
      await core.calls.exec(api.tracks.setParam(id, name, value));
    },
    async setMonitor(id, mode) {
      core.copy.patchProject((p) => {
        const kind = p.graph.nodes.find((n) => n.kind.type === "audio" && n.kind.track === id)?.kind;
        if (kind?.type === "audio") kind.monitor = mode;
      });
      await core.calls.exec(api.tracks.setMonitor(id, mode));
    },
    async setColor(id, color) {
      core.copy.patchProject((p) => {
        const track = p.tracks.find((t) => t.id === id);
        if (track) track.color = color;
      });
      await core.calls.exec(api.tracks.setColor(id, color));
    },
    resize,
    async setHeight(id, height) {
      resize(id, height);
      await core.calls.exec(api.tracks.setHeight(id, height));
    },
  };
}
