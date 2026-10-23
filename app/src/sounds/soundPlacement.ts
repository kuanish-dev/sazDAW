import { STEP_TICKS, type Id, type SampleInfo } from "../api/types";
import { planDrop, sampleLengthTicks, type NewTrackKind, type SampleSource } from "../tracks/dropSample";
import { DEFAULT_VELOCITY, MIDDLE_C } from "../common/pitch";
import { samplerTrack } from "../tracks/samplerTracks";
import { barStart } from "../common/tempoMap";
import { firstAudioTrack, trackById } from "../tracks/trackRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import { addTracks } from "../tracks/newTracks";
import type { Samples } from "../samples/samples";

export interface SoundPlacement {
  /** Audio clip on the first audio track (created if missing) at the current bar. */
  addSoundToTimeline(path: string): Promise<void>;
  /** Sound → new sampler track. Returns the track id. */
  addSoundToTrack(path: string): Promise<Id | undefined>;
  /** Opens the Samples tab with this sound. */
  editSound(path: string): Promise<void>;
  /**
   * Sound dropped onto an existing track. Audio track: a clip at `tick` (occupied — nothing).
   * Instrument track: the sound becomes its instrument and name, notes stay.
   */
  dropSample(source: SampleSource, track: Id, tick: number): Promise<void>;
  /**
   * Sound dropped onto the "new track" zone. `instrument` — a sampler track named after the sound with a one-bar clip
   * holding one note (the sound plays right away, notes are added later); `block` — an audio track with the whole clip.
   * The clip goes to the bar of the playhead. Selects the created track.
   */
  dropAsNewTrack(source: SampleSource, kind: NewTrackKind): Promise<void>;
}

export function createSoundPlacement(core: Core, views: Views, samples: Pick<Samples, "importQuietly" | "sampleInfo">,
): SoundPlacement {
  const { api } = core;
  /** Bar of the playhead: new clips go there. */
  const currentBar = () => barStart(core.copy.project.tempoMap, core.playback.tick().positionTicks);

  /** Project sample for a drop source (a file from disk is imported). */
  const resolveSample = async (source: SampleSource): Promise<SampleInfo | undefined> => {
    if (source.kind === "file") return samples.importQuietly(source.path);
    const info = await samples.sampleInfo(source.id);
    if (!info) core.notifications.toast("error", t().tracks.sampleNotFound);
    return info;
  };

  /** The sound becomes the track's instrument (sampler) and the track takes its name: notes stay, the root note is the track's step input pitch. */
  const replaceInstrument = async (track: Id, sample: SampleInfo) => {
    const root = trackById(core.copy.project, track)?.defaultPitch ?? MIDDLE_C;
    if (!(await core.calls.attempt(api.tracks.setInstrument(track, "sampler", { sample: sample.id, root }, sample.name)))) return;
    await views.showCreated({ track });
    core.notifications.toast("info", t().tracks.playsSound(sample.name));
  };

  /** New track for the sound: audio for a ready block, sampler for an instrument. */
  const newTrackFor = async (info: SampleInfo, kind: NewTrackKind) =>
    kind === "block" ? core.calls.run(api.tracks.add(info.name, null, MIDDLE_C)) : (await core.calls.run(api.tracks.addMany([samplerTrack(info)])))?.[0];

  /** Clip for the sound on the new track: the whole audio clip or a one-bar note clip with one note. */
  const clipFor = async (info: SampleInfo, kind: NewTrackKind, track: Id, start: number) => {
    if (kind === "block") return core.calls.run(api.clips.addAudio(track, start, info.id));
    const note = { pitch: MIDDLE_C, startTick: 0, lengthTicks: STEP_TICKS, velocity: DEFAULT_VELOCITY };
    return core.calls.run(api.clips.addNote({ track, startTick: start, notes: [note] }));
  };

  return {
    async addSoundToTimeline(path) {
      const sample = await samples.importQuietly(path);
      if (!sample) return;
      const track = firstAudioTrack(core.copy.project)?.id ?? (await core.calls.run(api.tracks.add(t().tracks.audioName, null, MIDDLE_C)));
      if (track === undefined) return;
      if (!(await core.calls.run(api.clips.addAudio(track, currentBar(), sample.id)))) return;
      await views.showCreated({ track }, "tracks");
    },

    async addSoundToTrack(path) {
      const info = await samples.importQuietly(path);
      return info ? (await addTracks(core, views, [samplerTrack(info)]))?.[0] : undefined;
    },

    async editSound(path) {
      const sample = await samples.importQuietly(path);
      if (sample) await views.showCreated({ sample: sample.id }, "samples");
    },

    async dropSample(source, track, tick) {
      const info = await resolveSample(source);
      if (!info) return;
      const start = Math.max(0, tick);
      const length = sampleLengthTicks(info.frameCount, info.sampleRate, core.copy.project.tempoMap, start);
      const plan = planDrop(core.copy.project.tracks, core.copy.project.clips, track, start, length);
      if (!plan) return;
      if (plan.instrument) return replaceInstrument(plan.track, info);
      const clip = await core.calls.run(api.clips.addAudio(plan.track, plan.startTick, info.id));
      if (clip) await views.showCreated({ clips: [clip.id] }, "tracks");
    },

    async dropAsNewTrack(source, kind) {
      const info = await resolveSample(source);
      if (!info) return;
      const start = currentBar();
      const track = await newTrackFor(info, kind);
      if (track === undefined) return;
      const clip = await clipFor(info, kind, track, start);
      if (!clip) return core.calls.exec(api.tracks.remove(track));
      await views.showCreated({ clips: [clip.id] }, "tracks");
    },
  };
}
