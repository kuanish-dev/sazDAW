import type { Id, InputChannels } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import type { RecordingStatus } from "./recordingStatus";
import type { NoteRecord } from "./noteRecord";

export interface AudioRecord {
  /** Arms or disarms recording on an audio track (an armed track opens its input: levels, "auto" monitoring). */
  setArmed(track: Id, armed: boolean): Promise<void>;
  /** Which input channels the track records and monitors. */
  setInput(track: Id, input: InputChannels): Promise<void>;
  /** Stops the take and selects the created clips. */
  stop(): Promise<void>;
  /**
   * The transport's "● Record" button: a take is running — stop it and note recording; there are armed audio tracks —
   * arm note recording and start a take (notes record while the transport plays); otherwise just toggle note recording.
   */
  pressRecord(notesArmed: boolean): Promise<void>;
}

export function createAudioRecord(core: Core, views: Views, recording: Pick<RecordingStatus, "followRecording">, notes: Pick<NoteRecord, "setArmed">): AudioRecord {
  const { api } = core;

  /** Starts a take on all armed tracks from the current position (a stopped transport starts). */
  const start = async () => {
    await core.calls.attempt(api.audioRecord.start());
    recording.followRecording();
  };

  const stop = async () => {
    const before = await core.calls.run(api.audioRecord.status());
    const clips = await core.calls.run(api.audioRecord.stop());
    recording.followRecording();
    if (!clips) return;
    await views.showCreated({ clips: clips.map((c) => c.id) });
    core.notifications.toast("info", t().tracks.recordingAdded(clips.length));
    if (before && before.droppedFrames > 0) {
      core.notifications.toast("error", t().tracks.recordingDropped);
    }
  };

  return {
    async setArmed(track, armed) {
      await core.calls.exec(api.audioRecord.setArmed(track, armed));
      recording.followRecording();
    },
    async setInput(track, input) {
      await core.calls.exec(api.audioRecord.setInput(track, input));
      recording.followRecording();
    },
    stop,
    async pressRecord(notesArmed) {
      const status = await core.calls.run(api.audioRecord.status());
      if (status?.recording) {
        await stop();
        if (notesArmed) await notes.setArmed(false);
        return;
      }
      if (!status?.tracks.some((t) => t.armed)) {
        await notes.setArmed(!notesArmed);
        return;
      }
      if (!notesArmed) await notes.setArmed(true);
      await start();
    },
  };
}
