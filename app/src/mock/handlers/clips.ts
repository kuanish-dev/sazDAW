// Timeline clips: note and audio clips, position, split, trim, duplicate, loop region, audio clip takes and comping,
// normalize and reverse (`ClipProcessing`; the mock has no audio, so normalize assumes a fixed peak).
import type { Clip, ClipContent, CompSegment, Id, Project } from "../../api/types";
import { sampleLengthTicks } from "../../tracks/dropSample";
import { barLengthAt } from "../../common/tempoMap";
import { ticksToSourceFrames } from "../../tracks/warp";
import { t } from "../../i18n";
import { MAX_GAIN, MAX_TICK, applyEdit, checkFits, checkLimit, checkNoOverlaps, checkPlacement, duplicateClip, newClip, trimStart } from "../clipRules";
import { MOCK_SAMPLE_RATE } from "../audio";
import { fail } from "../errors";
import { checkLoopTicks, emptyNotes, fitLength, replaceNotes, setLoopTicks } from "../noteOps";
import { activateTake, chooseTakeRange, removeTake, setComp, splitComp } from "../takeRules";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** Normalize in the mock: every clip is assumed to peak at this level (there is no real audio). */
const MOCK_PEAK = 0.5;
/** As `daw_contracts::clip_processing`. */
const DEFAULT_NORMALIZE_PEAK_DB = -0.3;
const MIN_NORMALIZE_PEAK_DB = -60;

export function clipsHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};

  const place = (p: Project, track: Id, start: number, length: number, content: ClipContent): Clip => {
    checkLimit(p);
    checkFits(w, p, track, content);
    checkPlacement(p, track, start, length);
    const clip = newClip(w.id(p), track, start, length, content);
    p.clips.push(clip);
    return clip;
  };

  // Like "empty clip → notes.replace → notes.setLoopTicks", but as one undo step and all or nothing.
  h["clips.addNote"] = ({ track, startTick, lengthTicks, notes, loopTicks }) => w.edit(t().mockEdits.history.addClip, (p) => {
    if (loopTicks != null) checkLoopTicks(loopTicks);
    // The bar follows the time signature at the clip position: it is both the content length and the default clip length.
    const bar = barLengthAt(p.tempoMap, startTick);
    const content = emptyNotes();
    replaceNotes(content, notes ?? []);
    fitLength(content, bar);
    if (loopTicks != null) setLoopTicks(content, loopTicks);
    return place(p, track, startTick, lengthTicks ?? bar, content);
  });

  h["clips.addAudio"] = ({ track, startTick, sample }) => {
    const info = w.sampleInfo(sample);
    return w.edit(t().mockEdits.history.addAudioClip, (p) => {
      const length = Math.min(MAX_TICK, sampleLengthTicks(info.frameCount, info.sampleRate, p.tempoMap, startTick));
      return place(p, track, startTick, length, { type: "audio", sample });
    });
  };

  h["clips.edit"] = ({ id, edit }) => w.edit(t().mockEdits.history.editClip, (p) => {
    const clip = w.clip(p, id);
    applyEdit(w, p, clip, edit);
    checkNoOverlaps(p, [id]);
    return clip;
  });

  h["clips.split"] = ({ id, tick }) => w.edit(t().mockEdits.history.splitClip, (p) => {
    checkLimit(p);
    const original = w.clip(p, id);
    const end = original.startTick + original.lengthTicks;
    if (tick <= original.startTick || tick >= end) throw fail("CL-112", { tick, id });
    const leftLength = tick - original.startTick;
    const right: Clip = w.clone(original);
    right.id = w.id(p);
    right.startTick = tick;
    right.lengthTicks = end - tick;
    right.fadeInTicks = 0;
    if (right.content.type === "notes") {
      // The right half continues the loop phase where the left one ended.
      right.offsetTicks = (original.offsetTicks + leftLength) % right.content.loopTicks;
    } else {
      const info = w.sampleInfos().find((s) => s.id === (original.content as { sample: Id }).sample);
      right.offsetFrames = original.offsetFrames + Math.round(ticksToSourceFrames(original, leftLength, p.tempoMap, info?.sampleRate ?? MOCK_SAMPLE_RATE));
    }
    original.lengthTicks = leftLength;
    original.fadeOutTicks = 0;
    [original.content, right.content] = splitComp(original.content, leftLength);
    p.clips.push(right);
    return right;
  });

  h["clips.trimStart"] = ({ id, startTick }: { id: Id; startTick: number }) => w.edit(t().mockEdits.history.trimClip, (p) => {
    const clip = w.clip(p, id);
    const content = clip.content;
    const sampleRate = content.type === "audio" ? w.sampleInfo(content.sample).sampleRate : MOCK_SAMPLE_RATE;
    Object.assign(clip, trimStart(clip, startTick, p.tempoMap, sampleRate));
    checkNoOverlaps(p, [id]);
    return clip;
  });

  h["clips.normalize"] = ({ ids, peakDb }: { ids: Id[]; peakDb?: number }) => {
    const db = peakDb ?? DEFAULT_NORMALIZE_PEAK_DB;
    if (!(db >= MIN_NORMALIZE_PEAK_DB && db <= 0)) throw fail("CL-216", { db, min: MIN_NORMALIZE_PEAK_DB });
    if (ids.length === 0) return [];
    return w.edit(t().mockEdits.history.normalize, (p) => ids.map((id) => {
      const clip = w.clip(p, id);
      if (clip.content.type !== "audio") throw fail("CL-130", { id });
      clip.gain = Math.min(MAX_GAIN, 10 ** (db / 20) / MOCK_PEAK);
      return clip;
    }));
  };

  h["clips.reverse"] = ({ id }: { id: Id }) => {
    const clip = w.clip(w.project, id);
    const content = clip.content;
    if (content.type !== "audio") throw fail("CL-130", { id });
    if ((content.takes?.length ?? 0) > 1) throw fail("CL-131", { id });
    const info = w.sampleInfo(content.sample);
    const folder = w.requireFolder(fail("SM-310"));
    const played = ticksToSourceFrames(clip, clip.lengthTicks, w.project.tempoMap, info.sampleRate);
    const mirrored = Math.max(0, info.frameCount - Math.min(info.frameCount, Math.ceil(clip.offsetFrames + played)));
    const path = w.uniqueSamplePath(folder, t().mockEdits.reversedName(info.name), ".wav");
    return w.edit(t().mockEdits.history.reverseClip, (p) => {
      const reversed = w.id(p);
      p.samples.push({ id: reversed, name: path.slice(path.lastIndexOf("/") + 1, -".wav".length), path, markers: [] });
      w.sampleMeta.set(reversed, { frameCount: info.frameCount, sampleRate: info.sampleRate });
      const target = w.clip(p, id);
      target.content = { type: "audio", sample: reversed };
      target.offsetFrames = mirrored;
      return target;
    });
  };

  h["clips.duplicate"] = ({ id }) => w.edit(t().mockEdits.history.duplicateClip, (p) => duplicateClip(w, p, id));

  h["clips.remove"] = ({ id }) => w.edit(t().mockEdits.history.removeClip, (p) => {
    w.clip(p, id);
    p.clips = p.clips.filter((c) => c.id !== id);
  });

  h["clips.setLoopRegion"] = ({ region }) => w.edit(t().mockEdits.history.loop, (p) => {
    if (region && region[0] >= region[1]) throw fail("CL-113", { start: region[0], end: region[1] });
    p.loopRegion = region;
  });

  h["clips.setActiveTake"] = ({ id, take }: { id: Id; take: Id }) => {
    const clip = w.clip(w.project, id);
    // Already active and without a comp: no undo step, as in the backend.
    if (clip.content.type === "audio" && clip.content.sample === take && !clip.content.comp) return clip;
    return w.edit(t().mockEdits.history.selectTake, (p) => {
      const target = w.clip(p, id);
      target.content = activateTake(target.content, take);
      return target;
    });
  };
  h["clips.removeTake"] = ({ id, take }: { id: Id; take: Id }) => w.edit(t().mockEdits.history.removeTake, (p) => {
    const target = w.clip(p, id);
    target.content = removeTake(target.content, take);
    return target;
  });
  h["clips.setComp"] = ({ id, segments }: { id: Id; segments: CompSegment[] }) => w.edit(t().mockEdits.history.comp, (p) => {
    const target = w.clip(p, id);
    target.content = setComp(target.content, target.lengthTicks, segments);
    return target;
  });
  h["clips.chooseTakeRange"] = ({ id, from, to, take }: { id: Id; from: number; to: number; take: Id }) => w.edit(t().mockEdits.history.takeRange, (p) => {
    const target = w.clip(p, id);
    target.content = chooseTakeRange(target.content, target.lengthTicks, from, to, take);
    return target;
  });
  return h;
}
