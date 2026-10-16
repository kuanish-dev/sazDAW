// Track actions: instrument tracks (note clips) and audio tracks (audio clips). Backend errors are shown as toasts.
import type { Id, MonitorMode, TrackColor } from "../api/types";
import { instrumentInfo } from "../domain/catalog";
import { defaultPitchFor, nextTrackName } from "../domain/tracks";
import { t } from "../i18n";
import type { Backend, Calls, ProjectCopy, View } from "./context";
import type { ParamTouch } from "./paramTouch";

export interface TracksActions {
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

export function createTracksActions(ctx: Backend & Calls & ProjectCopy & View, touch: ParamTouch): TracksActions {
  const { api } = ctx;

  const resize: TracksActions["resize"] = (id, height) => {
    ctx.patchProject((p) => {
      const track = p.tracks.find((t) => t.id === id);
      if (track) track.height = height;
    });
  };

  /** New track (`kind: null` — audio) with a given name; selects it. Returns the id or `undefined` on failure. */
  const add = async (name: string, kind: string | null, defaultPitch: number) => {
    const id = await ctx.run(api.tracks.add(name, kind, defaultPitch));
    if (id !== undefined) await ctx.showCreated({ track: id });
    return id;
  };

  return {
    async create(kind) {
      const label = kind === null ? t().tracks.audioName : instrumentInfo(ctx.catalog(), kind)?.label;
      if (label === undefined) return undefined;
      const count = ctx.project.tracks.filter((t) => (t.instrument?.kind ?? null) === kind).length;
      return add(nextTrackName(label, count), kind, defaultPitchFor(kind));
    },
    remove: (id) => ctx.exec(api.tracks.remove(id)),
    rename: (id, name) => ctx.exec(api.tracks.rename(id, name)),
    move: (id, index) => ctx.exec(api.tracks.move(id, index)),
    async setParam(id, name, value) {
      touch.moved({ type: "instrumentParam", track: id, param: name }, value);
      ctx.patchProject((p) => {
        const instrument = p.tracks.find((t) => t.id === id)?.instrument;
        if (instrument) instrument.params[name] = value;
      });
      await ctx.exec(api.tracks.setParam(id, name, value));
    },
    async setMonitor(id, mode) {
      ctx.patchProject((p) => {
        const kind = p.graph.nodes.find((n) => n.kind.type === "audio" && n.kind.track === id)?.kind;
        if (kind?.type === "audio") kind.monitor = mode;
      });
      await ctx.exec(api.tracks.setMonitor(id, mode));
    },
    async setColor(id, color) {
      ctx.patchProject((p) => {
        const track = p.tracks.find((t) => t.id === id);
        if (track) track.color = color;
      });
      await ctx.exec(api.tracks.setColor(id, color));
    },
    resize,
    async setHeight(id, height) {
      resize(id, height);
      await ctx.exec(api.tracks.setHeight(id, height));
    },
  };
}
