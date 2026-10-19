// Audio recording state for the UI (audio track headers, banner): arming and track inputs, levels, take progress.
// Polling starts on `followRecording()` (after any recording action and at startup) and stops itself when no take is running
// and no track is armed: then the input is closed and levels do not change. The last state stays visible.
import { createSignal } from "solid-js";
import type { AudioRecordStatus, Id, TrackRecordState } from "../api/types";
import { createPoller } from "./poller";

/** Poll ~15 times per second: recording time and levels look live. */
const POLL_MS = 66;

/** A track missing from the response: not armed, "stereo" input. */
const IDLE_TRACK = (track: Id): TrackRecordState => ({ track, armed: false, input: "stereo", left: 0, right: 0 });

/** What the UI reads of audio recording. */
export interface RecordingReads {
  /** The audio recording take in progress (null — none). */
  recordingStatus(): AudioRecordStatus | null;
  /** Audio track input: arming, channels, levels. */
  trackInput(track: Id): TrackRecordState;
  /** Why input monitoring is silent (null — all fine). */
  monitorProblem(): string | null;
}

export function createRecordingStatus(fetch: () => Promise<AudioRecordStatus>, onError: (error: unknown) => void) {
  const [status, setStatus] = createSignal<AudioRecordStatus | null>(null);

  const poller = createPoller(async () => {
    try {
      const next = await fetch();
      setStatus(next);
      if (!next.recording && !next.tracks.some((t) => t.armed)) poller.stop();
    } catch (error) {
      poller.stop();
      onError(error);
    }
  }, POLL_MS);

  return {
    recordingStatus: () => (status()?.recording ? status() : null),
    trackInput: (track: Id) => status()?.tracks.find((t) => t.track === track) ?? IDLE_TRACK(track),
    monitorProblem: () => status()?.monitorProblem ?? null,
    /** Polls until no take runs and no track is armed. */
    followRecording: poller.start,
    dispose: poller.stop,
  };
}
