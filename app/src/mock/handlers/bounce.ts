import type { Id, Project } from "../../api/types";
import { neutralUtility } from "../../mixer/channelUtility";
import { hasStrip, sourceOfTrack } from "../../graph/graphRules";
import { secondsAt, tickAtSeconds } from "../../common/tempoMap";
import { t } from "../../i18n";
import { MOCK_SAMPLE_RATE } from "../audio";
import { newClip } from "../clipRules";
import { fail } from "../errors";
import { addSource, dropNode } from "../graphOps";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const DEFAULT_TAIL_SECONDS = 2;
const MAX_TAIL_SECONDS = 30;

export function bounceHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};

  const makeCopy = (id: Id, tailSeconds: number | undefined, freeze: boolean): Id => {
    const original = w.track(w.project, id);
    if (original.instrument?.frozen != null) throw fail("TR-114", { id });
    if (freeze && !original.instrument) throw fail("EX-110", { id });
    const tail = tailSeconds ?? DEFAULT_TAIL_SECONDS;
    if (!(tail >= 0 && tail <= MAX_TAIL_SECONDS)) throw fail("EX-250", { seconds: tail, max: MAX_TAIL_SECONDS });
    const clips = w.project.clips.filter((c) => c.track === id);
    if (clips.length === 0) throw fail("EX-350", { id });
    const folder = w.requireFolder(fail("EX-310"));
    const start = Math.min(...clips.map((c) => c.startTick));
    const end = Math.max(...clips.map((c) => c.startTick + c.lengthTicks));
    const map = w.project.tempoMap;
    const seconds = secondsAt(map, end) - secondsAt(map, start) + tail;
    const name = freeze ? t().mockEdits.bounce.frozenName(original.name) : t().mockEdits.bounce.bouncedName(original.name);

    return w.edit(freeze ? t().mockEdits.history.freeze(original.name) : t().mockEdits.history.bounce(original.name), (p: Project) => {
      const sample = w.id(p);
      p.samples.push({ id: sample, name, path: w.uniqueSamplePath(folder, name, ".wav"), markers: [] });
      w.sampleMeta.set(sample, { frameCount: Math.round(seconds * MOCK_SAMPLE_RATE), sampleRate: MOCK_SAMPLE_RATE });
      const copy = w.id(p);
      p.tracks.splice(p.tracks.findIndex((x) => x.id === id) + 1, 0, { id: copy, name, instrument: null, defaultPitch: original.defaultPitch });
      const source = sourceOfTrack(p.graph, id);
      const lead = source && hasStrip(source.kind) ? Math.min(0, source.kind.delayMs) : 0;
      const copyKind = { type: "audio" as const, track: copy, volume: 1, pan: 0, mute: false, solo: false, ...neutralUtility(), delayMs: lead, monitor: "off" as const };
      addSource(p, name, copyKind, source?.x ?? 40, (source?.y ?? 40) + 30);
      const length = Math.round(tickAtSeconds(map, secondsAt(map, start) + seconds) - start);
      p.clips.push(newClip(w.id(p), copy, start, length, { type: "audio", sample }));
      const target = w.track(p, id);
      if (freeze && target.instrument) target.instrument.frozen = copy;
      const kind = sourceOfTrack(p.graph, id)?.kind;
      if (!freeze && kind && (kind.type === "instrument" || kind.type === "audio")) kind.mute = true;
      return copy;
    });
  };

  h["tracks.bounce"] = ({ id, tailSeconds }) => makeCopy(id, tailSeconds, false);
  h["tracks.freeze"] = ({ id, tailSeconds }) => makeCopy(id, tailSeconds, true);
  h["tracks.unfreeze"] = ({ id }) => {
    const original = w.track(w.project, id);
    const copy = original.instrument?.frozen;
    if (copy == null) throw fail("EX-111", { id });
    return w.edit(t().mockEdits.history.unfreeze(original.name), (p) => {
      const samples = p.clips.filter((c) => c.track === copy && c.content.type === "audio").map((c) => (c.content.type === "audio" ? c.content.sample : -1));
      const node = sourceOfTrack(p.graph, copy);
      if (node) dropNode(p, node.id);
      p.clips = p.clips.filter((c) => c.track !== copy);
      p.tracks = p.tracks.filter((x) => x.id !== copy);
      p.samples = p.samples.filter((s) => !samples.includes(s.id) || p.clips.some((c) => c.content.type === "audio" && c.content.sample === s.id));
      const target = w.track(p, id);
      if (target.instrument) target.instrument.frozen = null;
    });
  };
  return h;
}
