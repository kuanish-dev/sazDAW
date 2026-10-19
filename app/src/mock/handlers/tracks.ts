// Tracks: adding (together with the graph source node), removing, name, color, instrument params.
import type { Id, MonitorMode, NewTrack, NodeKind, Params, Project, TrackColor } from "../../api/types";
import { neutralUtility } from "../../mixer/channelUtility";
import { sourceOfTrack } from "../../graph/graphRules";
import { MAX_TRACK_HEIGHT, MIN_TRACK_HEIGHT } from "../../tracks/rowLayout";
import { t } from "../../i18n";
import { fitParam, paramsFor } from "../catalog";
import { fail } from "../errors";
import { gatherFolders } from "../folderRules";
import { addSource, dropNode } from "../graphOps";
import { checkColor } from "./folders";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const MAX_TRACKS = 64;
const NEW_VOLUME = 0.8;
/** Sources of new tracks stand in a column on the left of the graph. */
const SOURCE_X = 40;
const SOURCE_Y = 40;
const SOURCE_ROW = 60;

/** Name of a track and of its graph source node (they always match). */
function renameTrack(w: MockWorld, p: Project, id: Id, name: string) {
  w.track(p, id).name = name;
  const source = sourceOfTrack(p.graph, id);
  if (source) source.name = name;
}

export function tracksHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const instrument = (kind: string) => w.must(w.catalog.instruments.find((k) => k.kind === kind), fail("TR-002", { kind }));

  /** Checks the spec before the edit: a batch is created whole or not at all. */
  const prepare = (spec: NewTrack): Params | null => {
    if (!(spec.defaultPitch >= 0 && spec.defaultPitch <= 127)) throw fail("TR-210", { pitch: spec.defaultPitch });
    if (spec.kind === null) {
      if (Object.keys(spec.params ?? {}).length > 0) throw fail("TR-110");
      return null;
    }
    return paramsFor(instrument(spec.kind), spec.params ?? {}, "TR-113");
  };

  const add = (label: string, specs: NewTrack[]) => {
    const prepared = specs.map((spec) => ({ spec, params: prepare(spec) }));
    return w.edit(label, (p: Project) => {
      const ids: Id[] = [];
      for (const { spec, params } of prepared) {
        if (p.tracks.length >= MAX_TRACKS) throw fail("TR-211", { max: MAX_TRACKS });
        const id = w.id(p);
        p.tracks.push({
          id, name: spec.name, instrument: params && spec.kind ? { kind: spec.kind, params } : null, defaultPitch: spec.defaultPitch,
        });
        const fader = { track: id, volume: NEW_VOLUME, pan: 0, mute: false, solo: false, ...neutralUtility() };
        const kind: NodeKind = params ? { type: "instrument", ...fader } : { type: "audio", ...fader, monitor: "off" };
        addSource(p, spec.name, kind, SOURCE_X, SOURCE_Y + SOURCE_ROW * (p.tracks.length - 1));
        ids.push(id);
      }
      return ids;
    });
  };

  h["tracks.add"] = ({ name, kind, defaultPitch }) => add(t().mock.undo.addTrack, [{ name, kind, defaultPitch, params: {} }])[0];
  h["tracks.addMany"] = ({ tracks }) => add(t().mock.undo.addTracks, tracks);
  h["tracks.remove"] = ({ id }) => w.edit(t().mock.undo.removeTrack, (p) => {
    w.track(p, id);
    const source = sourceOfTrack(p.graph, id);
    if (source) dropNode(p, source.id);
    p.clips = p.clips.filter((c) => c.track !== id);
    p.tracks = p.tracks.filter((t) => t.id !== id);
    p.automation = p.automation.filter((l) => !(l.target.type === "instrumentParam" && l.target.track === id));
    gatherFolders(p);
  });
  h["tracks.setColor"] = ({ id, color }: { id: Id; color: TrackColor | null }) => {
    w.track(w.project, id);
    if (color !== null) checkColor(color);
    return w.edit(t().mock.undo.trackColor, (p) => {
      w.track(p, id).color = color;
    });
  };
  h["tracks.setHeight"] = ({ id, height }: { id: Id; height: number | null }) => {
    w.track(w.project, id);
    if (height !== null && (height < MIN_TRACK_HEIGHT || height > MAX_TRACK_HEIGHT)) {
      throw fail("TR-212", { height, min: MIN_TRACK_HEIGHT, max: MAX_TRACK_HEIGHT });
    }
    return w.edit(t().mock.undo.trackHeight, (p) => {
      w.track(p, id).height = height;
    });
  };
  h["tracks.setMonitor"] = ({ track, mode }: { track: Id; mode: MonitorMode }) => {
    const found = w.track(w.project, track);
    const source = sourceOfTrack(w.project.graph, track);
    if (!["off", "on", "auto"].includes(mode)) throw fail("RP-101", { method: "tracks.setMonitor", details: `unknown variant \`${mode}\`` });
    if (source?.kind.type !== "audio") throw fail("TR-112", { name: found.name });
    if (source.kind.monitor === mode) return null;
    return w.edit(t().mock.undo.inputMonitor, (p) => {
      const kind = sourceOfTrack(p.graph, track)?.kind;
      if (kind?.type === "audio") kind.monitor = mode;
    });
  };
  h["tracks.rename"] = ({ id, name }) => w.edit(t().mock.undo.renameTrack, (p) => renameTrack(w, p, id, name));
  h["tracks.move"] = ({ id, index }) => {
    w.track(w.project, id);
    if (!Number.isInteger(index) || index < 0) throw fail("RP-101", { method: "tracks.move", details: `invalid value: ${index}, expected usize` });
    // As in the contract: a position past the end is clamped to the end.
    const target = Math.min(index, w.project.tracks.length - 1);
    return w.edit(t().mock.undo.moveTrack, (p) => {
      const from = p.tracks.findIndex((t) => t.id === id);
      const [track] = p.tracks.splice(from, 1);
      p.tracks.splice(target, 0, track!);
      // As in the contract: a folder's tracks gather together at the topmost one.
      gatherFolders(p);
    });
  };
  h["tracks.setInstrument"] = ({ id, kind, params = {}, name }: { id: Id; kind: string; params?: Params; name?: string }) => {
    const current = w.track(w.project, id).instrument;
    if (!current) throw fail("TR-160", { id });
    if (current.frozen != null) throw fail("TR-114", { id });
    const values = paramsFor(instrument(kind), params, "TR-113");
    if (current.kind === kind && Object.keys(params).length === 0 && name === undefined) return;
    return w.edit(t().mock.undo.changeInstrument, (p) => {
      w.track(p, id).instrument = { kind, params: values };
      if (name !== undefined) renameTrack(w, p, id, name);
      // As in the contract: the old kind's params do not exist on the new one.
      p.automation = p.automation.filter((l) => !(l.target.type === "instrumentParam" && l.target.track === id));
    });
  };
  h["tracks.setParam"] = ({ id, name, value }) => {
    if (!Number.isFinite(value)) throw fail("TR-111", { value });
    const current = w.track(w.project, id).instrument;
    if (!current) throw fail("TR-160", { id });
    const info = w.must(
      w.catalog.instruments.find((k) => k.kind === current.kind)?.params.find((x) => x.name === name),
      fail("TR-113", { kind: current.kind, param: name }),
    );
    return w.edit(t().mock.undo.instrumentParam, (p) => {
      const target = w.track(p, id).instrument;
      if (target) target.params[name] = fitParam(info, value);
    });
  };
  return h;
}
