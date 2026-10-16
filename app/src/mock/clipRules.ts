// Mock backend clip rules: range, matching track, placement (overlaps and audio clip crossfades), warp and pitch.
// Shared by the single and group methods.
import type { Clip, ClipContent, ClipEdit, Id, Project, TempoMap } from "../api/types";
import { PPQ } from "../api/types";
import { clipEnd, trimmedClip } from "../domain/clips";
import { tempoAt } from "../domain/tempoMap";
import { MAX_BPM, MAX_PITCH_SEMITONES, MIN_BPM, guessBpm, sourceFramesToTicks, ticksToSourceFrames } from "../domain/warp";
import { fail } from "./errors";
import { fitComp, shiftComp } from "./takeRules";
import type { MockWorld } from "./world";

export const MAX_CLIPS = 4096;
export const MAX_GAIN = 4;
/** Caps clip positions so numbers cannot grow without bound (as in the backend). */
export const MAX_TICK = PPQ * 60 * 500;

export const newClip = (id: Id, track: Id, startTick: number, lengthTicks: number, content: ClipContent): Clip => ({
  id, track, startTick, lengthTicks, content, gain: 1, fadeInTicks: 0, fadeOutTicks: 0, fadeInShape: "linear", fadeOutShape: "linear", offsetTicks: 0, offsetFrames: 0, warp: null, pitchSemitones: 0,
});

export const checkRange = (start: number, length: number) => {
  if (length < 1 || start < 0 || start > MAX_TICK || length > MAX_TICK) throw fail("CL-210", { start, length, max: MAX_TICK });
};

const isAudioTrack = (p: Project, track: Id) => p.tracks.find((t) => t.id === track)?.instrument === null;

/** The track's placement rules (as `clip_rules.rs`): note clips never overlap; audio clips overlap only at their edges, two at most. */
const checkTrack = (p: Project, clips: Clip[], track: Id) => {
  const audio = isAudioTrack(p, track);
  const spans = clips.filter((c) => c.track === track).map((c) => [c.startTick, clipEnd(c)] as const)
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  for (let i = 0; i + 1 < spans.length; i++) {
    const [start, end] = spans[i]!;
    const [nextStart, nextEnd] = spans[i + 1]!;
    if (nextStart >= end) continue;
    if (!audio) throw fail("CL-114", { track });
    if (nextStart === start || nextEnd <= end) throw fail("CL-115", { track });
    if (i + 2 < spans.length && spans[i + 2]![0] < end) throw fail("CL-116", { track });
  }
};

/** Room for a new clip (`ignore` is a clip treated as absent). */
export const checkPlacement = (p: Project, track: Id, start: number, length: number, ignore?: Id) => {
  checkRange(start, length);
  const probe = { id: -1, track, startTick: start, lengthTicks: length } as Clip;
  checkTrack(p, [...p.clips.filter((c) => c.id !== ignore), probe], track);
};

/** Checked on the final state: the tracks of touched clips follow the placement rules. */
export function checkNoOverlaps(p: Project, touched: Id[]) {
  const tracks = new Set(touched.map((id) => p.clips.find((x) => x.id === id)!.track));
  for (const track of tracks) checkTrack(p, p.clips, track);
}

/** Content must suit the track: notes go on instrument tracks, audio on audio tracks. */
export const checkFits = (w: MockWorld, p: Project, track: Id, content: ClipContent) => {
  const audio = w.track(p, track).instrument === null;
  if (content.type === "notes" && audio) throw fail("CL-110");
  if (content.type === "audio" && !audio) throw fail("CL-111");
};

export const checkLimit = (p: Project, adding = 1) => {
  if (p.clips.length + adding > MAX_CLIPS) throw fail("CL-213", { max: MAX_CLIPS });
};

/**
 * Warp and pitch (as `warp_rules.rs`): changes `clip` and returns the length at which the clip keeps
 * the same piece of audio. The mock has no real DSP, only lengths.
 */
function applyWarp(w: MockWorld, p: Project, clip: Clip, edit: ClipEdit): number {
  if (edit.warp === undefined && edit.pitchSemitones === undefined) return clip.lengthTicks;
  const content = clip.content;
  if (content.type !== "audio") throw fail("CL-119", { id: clip.id });
  if (edit.pitchSemitones !== undefined) {
    const pitch = edit.pitchSemitones;
    if (!Number.isFinite(pitch)) throw fail("CL-120", { value: pitch });
    if (Math.abs(pitch) > MAX_PITCH_SEMITONES) throw fail("CL-214", { semitones: pitch, max: MAX_PITCH_SEMITONES });
    clip.pitchSemitones = pitch;
  }
  if (edit.warp === undefined) return clip.lengthTicks;
  const info = w.sampleInfo(content.sample);
  const remaining = Math.max(0, info.frameCount - clip.offsetFrames);
  const covered = Math.min(remaining, ticksToSourceFrames(clip, clip.lengthTicks, p.tempoMap, info.sampleRate));
  if (edit.warp === null) {
    clip.warp = null;
  } else {
    const wanted = edit.warp.sourceBpm;
    if (wanted != null && !(wanted >= MIN_BPM && wanted <= MAX_BPM)) throw fail("TM-101", { bpm: wanted, min: MIN_BPM, max: MAX_BPM });
    const name = p.samples.find((s) => s.id === content.sample)?.name ?? "";
    clip.warp = { sourceBpm: wanted ?? guessBpm(name, info.frameCount, info.sampleRate) ?? tempoAt(p.tempoMap, clip.startTick) };
  }
  return Math.min(MAX_TICK, Math.max(1, Math.round(sourceFramesToTicks(clip, covered, p.tempoMap, info.sampleRate))));
}

/** Applies an edit to a clip of the draft project without checking placement (that is checked on the result). */
export function applyEdit(w: MockWorld, p: Project, clip: Clip, edit: ClipEdit) {
  const draft: Clip = { ...clip };
  const soundLength = applyWarp(w, p, draft, edit);
  const set = <K extends keyof Clip>(key: K): Clip[K] => ((edit as Partial<Clip>)[key] ?? draft[key]) as Clip[K];
  const [track, start] = [set("track"), set("startTick")];
  const length = edit.lengthTicks ?? soundLength;
  checkFits(w, p, track, clip.content);
  checkRange(start, length);
  const gain = set("gain");
  if (!(gain >= 0 && gain <= MAX_GAIN)) throw fail("CL-212", { gain, max: MAX_GAIN });
  Object.assign(clip, {
    track, startTick: start, lengthTicks: length, gain, warp: draft.warp, pitchSemitones: draft.pitchSemitones,
    fadeInTicks: Math.min(set("fadeInTicks"), length), fadeOutTicks: Math.min(set("fadeOutTicks"), length),
    fadeInShape: set("fadeInShape"), fadeOutShape: set("fadeOutShape"),
    content: fitComp(clip.content, length),
  });
}

/** `clips.trimStart` (as `clip_rules::trim_start`): the start moves, the end and the content stay; placement is checked by the caller. */
export function trimStart(clip: Clip, start: number, map: TempoMap, sampleRate: number): Clip {
  const end = clipEnd(clip);
  if (start >= end) throw fail("CL-129", { tick: start, id: clip.id, end });
  const trimmed = trimmedClip(clip, start, map, sampleRate);
  checkRange(trimmed.startTick, trimmed.lengthTicks);
  return { ...trimmed, content: fitComp(shiftComp(clip.content, trimmed.startTick - clip.startTick), trimmed.lengthTicks) };
}

/** A copy of the clip right after it (`clips.duplicate`; `notes.varyToNewClip` uses it too). */
export function duplicateClip(w: MockWorld, p: Project, id: Id): Clip {
  checkLimit(p);
  const copy: Clip = w.clone(w.clip(p, id));
  copy.startTick += copy.lengthTicks;
  checkPlacement(p, copy.track, copy.startTick, copy.lengthTicks);
  copy.id = w.id(p);
  p.clips.push(copy);
  return copy;
}
