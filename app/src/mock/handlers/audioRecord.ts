// Audio input recording in the browser without Rust: track arming and inputs, time from the clock, levels as a pseudo-sine
// (while any track is armed or a take runs); on stop each armed track gets a "Recording …" sample and an audio clip
// at the start position, or a new take of the clip the recording covered by half or lies within (one undo step). A take
// that played around the loop region becomes one sample per pass, all takes of one clip on the loop (`loopPasses.ts`).
import type { Clip, Id, InputChannels, TrackRecordState } from "../../api/types";
import { clamp } from "../../domain/math";
import { secondsAt, tickAtSeconds } from "../../domain/tempoMap";
import { t } from "../../i18n";
import { isBackendError } from "../../i18n/errors";
import { MOCK_SAMPLE_RATE } from "../audio";
import { checkLimit, checkPlacement, MAX_TICK, newClip } from "../clipRules";
import { fail } from "../errors";
import { loopPasses } from "../loopPasses";
import { activateTake, addTake, coveredClip } from "../takeRules";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** Anything shorter is not a recording (stop right after start). */
const MIN_SECONDS = 0.2;
const INPUTS: InputChannels[] = ["stereo", "left", "right"];

export function audioRecordHandlers(w: MockWorld): HandlerSet {
  /** Per-track arming and input are session state, as in the backend. */
  const arming = new Map<Id, { armed: boolean; input: InputChannels }>();
  /** The running take; `loop` is the loop region at its start: playback jumps back from its end. */
  let take: { tracks: Id[]; startTick: number; startedAt: number; loop: [number, number] | null } | null = null;
  const seconds = () => (take ? (performance.now() - take.startedAt) / 1000 : 0);

  const audioTrack = (track: Id) => {
    const found = w.track(w.project, track);
    if (found.instrument) throw fail("AU-141", { name: found.name });
    if (take) throw fail("AU-343");
    return arming.get(track) ?? { armed: false, input: "stereo" as InputChannels };
  };
  const remember = (track: Id, state: { armed: boolean; input: InputChannels }) => {
    if (state.armed || state.input !== "stereo") arming.set(track, state);
    else arming.delete(track);
  };
  /** Armed tracks that still exist; deleted ones are forgotten. */
  const armedTracks = () => {
    for (const id of [...arming.keys()]) {
      if (!w.project.tracks.some((t) => t.id === id && !t.instrument)) arming.delete(id);
    }
    return [...arming].filter(([, s]) => s.armed).map(([id]) => id);
  };

  const h: HandlerSet = {};
  h["audioRecord.setArmed"] = ({ track, armed }: { track: Id; armed: boolean }) => {
    remember(track, { ...audioTrack(track), armed });
    return null;
  };
  h["audioRecord.setInput"] = ({ track, input }: { track: Id; input: InputChannels }) => {
    if (!INPUTS.includes(input)) throw fail("RP-101", { method: "audioRecord.setInput", details: `unknown variant \`${input}\`` });
    remember(track, { ...audioTrack(track), input });
    return null;
  };

  h["audioRecord.start"] = () => {
    if (take) throw fail("AU-344");
    const tracks = armedTracks();
    if (tracks.length === 0) throw fail("AU-345");
    w.requireFolder(fail("AU-346"));
    take = { tracks, startTick: Math.floor(w.position), startedAt: performance.now(), loop: w.project.loopRegion };
    w.recordingAudio = true;
    if (w.state !== "playing") {
      w.state = "playing";
      w.needTicks();
    }
    return null;
  };

  h["audioRecord.status"] = () => {
    const now = performance.now();
    // The input is "open" while a track is armed or a take runs.
    const open = take !== null || [...arming.values()].some((s) => s.armed);
    const left = open ? 0.1 + 0.5 * Math.abs(Math.sin(now / 250)) : 0;
    const right = open ? 0.1 + 0.5 * Math.abs(Math.sin(now / 380 + 1)) : 0;
    const tracks: TrackRecordState[] = [...arming].map(([track, s]) => ({
      track, armed: s.armed, input: s.input,
      left: s.input === "right" ? right : left,
      right: s.input === "left" ? left : right,
    }));
    return { recording: take !== null, full: false, seconds: seconds(), left, right, droppedFrames: 0, tracks, monitorProblem: null };
  };

  h["audioRecord.stop"] = (): Clip[] => {
    if (!take) throw fail("AU-347");
    const done = take;
    const length = seconds();
    take = null;
    w.recordingAudio = false;
    if (length < MIN_SECONDS) throw fail("AU-348");
    const map = w.project.tempoMap;
    const ticks = clamp(Math.round(tickAtSeconds(map, secondsAt(map, done.startTick) + length) - done.startTick), 1, MAX_TICK - done.startTick);
    const folder = w.requireFolder(fail("AU-346"));
    const passes = loopPasses(map, done.startTick, length, done.loop);
    const failures: string[] = [];
    const clips = w.edit(t().mockEdits.history.recordAudio, (p): Clip[] => {
      /** A "recorded" sample: a unique name in the project folder and its length. */
      const newSample = (base: string, seconds: number): Id => {
        let name = base;
        for (let n = 2; p.samples.some((s) => s.name === name); n++) name = `${base} ${n}`;
        const sample = w.id(p);
        p.samples.push({ id: sample, name, path: `${folder}/samples/${name}.wav`, markers: [] });
        w.sampleMeta.set(sample, { frameCount: Math.round(seconds * MOCK_SAMPLE_RATE), sampleRate: MOCK_SAMPLE_RATE });
        return sample;
      };
      /** Loop recording: every pass is a take of the clip on the loop (or a new one at its start); the last whole pass plays. */
      const placePasses = (track: Id, trackName: string, loop: [number, number]): Clip => {
        const samples = passes.map((pass, i) => ({ id: newSample(t().mockEdits.recording.passName(trackName, i + 1), pass.seconds), whole: pass.reachedEnd }));
        let clip = coveredClip(p.clips, track, loop[0], loop[1]);
        for (const sample of samples) {
          if (clip) {
            clip.content = addTake(clip.content, sample.id);
            continue;
          }
          checkLimit(p);
          checkPlacement(p, track, loop[0], loop[1] - loop[0]);
          clip = newClip(w.id(p), track, loop[0], loop[1] - loop[0], { type: "audio", sample: sample.id });
          p.clips.push(clip);
        }
        const playing = samples.filter((s) => s.whole).pop();
        if (clip && playing) clip.content = activateTake(clip.content, playing.id);
        return clip!;
      };
      const placed: Clip[] = [];
      for (const track of done.tracks) {
        const trackName = p.tracks.find((x) => x.id === track)?.name ?? t().mockEdits.recording.trackFallback(track);
        try {
          if (passes.length > 1 && done.loop) {
            placed.push(placePasses(track, trackName, done.loop));
            continue;
          }
          const sample = newSample(t().mockEdits.recording.sampleName(trackName), length);
          const covered = coveredClip(p.clips, track, done.startTick, done.startTick + ticks);
          if (covered) {
            covered.content = addTake(covered.content, sample);
            placed.push(covered);
            continue;
          }
          checkLimit(p);
          checkPlacement(p, track, done.startTick, ticks);
          const added = newClip(w.id(p), track, done.startTick, ticks, { type: "audio", sample });
          p.clips.push(added);
          placed.push(added);
        } catch (error) {
          const details = isBackendError(error) ? error.message : String(error);
          failures.push(`"${trackName}": ${fail("AU-449", { details }).message}`);
        }
      }
      return placed;
    });
    if (failures.length > 0) throw fail("AU-448", { details: failures.join("; ") });
    return clips;
  };
  return h;
}
