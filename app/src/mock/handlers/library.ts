// Sounds and samples: project folders and linked folders, import, waveform, markers, preview, extraction.
import type { SampleInfo } from "../../api/types";
import { cleanMarkers } from "../../samples/waveform";
import { t } from "../../i18n";
import { MOCK_FRAME_COUNT, MOCK_SAMPLE_RATE, mockPeaks } from "../audio";
import { fail } from "../errors";
import { isOnlySound, removeTake } from "../takeRules";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** The backend returns no more envelope buckets than this. */
const MAX_BUCKETS = 20_000;
const HOME = "/home/user";

const fileName = (path: string) => path.split("/").pop() ?? "";
const stemOf = (name: string) => name.replace(/\.[^.]+$/, "");

export function libraryHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};

  h["sounds.list"] = ({ dir }) => {
    const root = w.folder();
    const where = String(dir);
    const outside = fail("SM-150", { path: where });
    if (where.split("/").includes("..")) throw outside;
    const inProject = !!root && (where === root || where.startsWith(`${root}/`));
    const linked = w.soundSources.find((s) => where === s || where.startsWith(`${s}/`));
    if (!inProject && !linked) throw outside;
    // A project always has samples/ and exports/, even if the mock "file system" has no entries for them yet.
    const fallback = !inProject ? undefined : where === root ? [{ name: "samples", isDir: true }, { name: "exports", isDir: true }]
      : where === `${root}/samples` || where === `${root}/exports` ? [] : undefined;
    const entries = (inProject ? w.projectFiles[where] : w.files[where]) ?? fallback;
    if (!entries) throw fail("SM-450", { path: where, details: "No such file or directory (os error 2)" });
    return entries.map((e) => ({ name: e.name, path: `${where}/${e.name}`, isDir: e.isDir }));
  };
  h["sounds.sources"] = () => [...w.soundSources];
  h["sounds.addSource"] = ({ path }) => {
    if (!w.files[path]) throw fail("PR-172", { path });
    if (!w.soundSources.includes(path)) w.soundSources.push(path);
  };
  h["sounds.removeSource"] = ({ path }) => { w.soundSources = w.soundSources.filter((s) => s !== path); };
  h["library.home"] = () => HOME;
  h["samples.list"] = () => w.sampleInfos();
  h["samples.remove"] = ({ id }) => {
    w.must(w.project.samples.find((s) => s.id === id), fail("SM-001", { id }));
    w.edit(t().mock.undo.removeSample, (p) => {
      p.samples = p.samples.filter((s) => s.id !== id);
      // A clip whose only sound this is goes away; a clip with other takes loses just this take.
      p.clips = p.clips.filter((c) => !isOnlySound(c.content, id));
      for (const clip of p.clips) {
        if (clip.content.type === "audio" && (clip.content.takes ?? []).includes(id)) clip.content = removeTake(clip.content, id);
      }
      for (const { instrument } of p.tracks) {
        if (instrument && instrument.params.sample === id) instrument.params.sample = 0;
      }
    });
  };

  /**
   * Like the real backend: a file outside the project folder is copied into `samples/`, the sample refers to the copy;
   * an unsaved project has nowhere to copy to, so the sample refers to the file itself.
   */
  const placeInProject = (path: string): string => {
    const folder = w.folder();
    if (!folder || path.startsWith(`${folder}/`)) return path;
    const known = w.copies.get(path);
    if (known) return known;
    const name = fileName(path);
    const ext = name.match(/\.[^.]+$/)?.[0] ?? "";
    const target = w.uniqueSamplePath(folder, stemOf(name), ext);
    w.copies.set(path, target);
    return target;
  };
  /** The file appears in the project folder listing (the sound browser will see it). */
  const listInProject = (path: string) => {
    const dir = path.slice(0, path.lastIndexOf("/"));
    const list = w.projectFiles[dir] ?? [];
    w.projectFiles[dir] = list;
    const name = path.slice(dir.length + 1);
    if (!list.some((e) => e.name === name)) list.push({ name, isDir: false });
  };
  const addSample = (path: string, meta: { frameCount: number; sampleRate: number }): SampleInfo => {
    const id = w.edit(t().mock.undo.importSample, (p) => {
      const created = w.id(p);
      p.samples.push({ id: created, name: stemOf(fileName(path)), path, markers: [] });
      return created;
    });
    listInProject(path);
    w.sampleMeta.set(id, meta);
    return w.sampleInfo(id);
  };

  h["samples.import"] = ({ path }) => {
    const target = placeInProject(path);
    return w.sampleInfos().find((s) => s.path === target) ?? addSample(target, { frameCount: MOCK_FRAME_COUNT, sampleRate: MOCK_SAMPLE_RATE });
  };
  h["samples.extract"] = ({ id, regions }) => {
    const source = w.sampleInfo(id);
    const folder = w.requireFolder(fail("SM-112"));
    if (!Array.isArray(regions) || regions.length === 0) throw fail("SM-111");
    const empty = (regions as { from: number; to: number }[]).find((r) => !(r.from < r.to) || r.from >= source.frameCount);
    if (empty) throw fail("SM-110", { from: empty.from, to: empty.to, frames: source.frameCount });
    return (regions as { from: number; to: number; name: string }[]).map((region) => {
      const clean = region.name.replace(/[/\\:*?"<>|]/g, "_").trim() || t().mock.library.defaultSoundName;
      const frameCount = Math.min(region.to, source.frameCount) - region.from;
      return addSample(w.uniqueSamplePath(folder, clean, ".wav"), { frameCount, sampleRate: source.sampleRate });
    });
  };
  h["samples.waveform"] = ({ id, from, to, buckets }) => {
    const info = w.sampleInfo(id);
    if (buckets < 1 || buckets > MAX_BUCKETS) throw fail("SM-211", { buckets, max: MAX_BUCKETS });
    const end = Math.min(to, info.frameCount);
    const start = Math.min(from, end);
    return { frameCount: info.frameCount, sampleRate: info.sampleRate, channels: 1, from: start, to: end, peaks: mockPeaks(start, end, buckets) };
  };
  h["samples.setMarkers"] = ({ id, markers }) => w.edit(t().mock.undo.sampleMarkers, (p) => {
    const sample = w.must(p.samples.find((s) => s.id === id), fail("SM-001", { id }));
    sample.markers = cleanMarkers(markers as number[], w.sampleMeta.get(id)?.frameCount ?? MOCK_FRAME_COUNT);
  });
  h["samples.exportRegion"] = ({ id, from, to, path }) => {
    const info = w.sampleInfo(id);
    if (!(from < to)) throw fail("SM-110", { from, to, frames: info.frameCount });
    if (String(path).includes("readonly")) throw fail("SM-453", { path, details: "Permission denied (os error 13)" });
  };
  h["preview.play"] = ({ id, from, to, looping }) => {
    const info = w.sampleInfo(id);
    if (!(from < to) || from >= info.frameCount) throw fail("SM-110", { from, to, frames: info.frameCount });
    w.previewState = { from, to: Math.min(to, info.frameCount), looping: !!looping, startedAt: performance.now() };
  };
  h["preview.playFile"] = ({ path }) => {
    if (!String(path).endsWith(".wav")) throw fail("SM-550", { path });
    w.previewState = { from: 0, to: MOCK_FRAME_COUNT, looping: false, startedAt: performance.now() };
  };
  h["preview.stop"] = () => { w.previewState = null; };
  return h;
}
