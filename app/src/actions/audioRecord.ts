// Audio recording from the input onto audio tracks: arming and track inputs, one take on all armed tracks at once,
// the transport's shared record button. Backend errors ("no input", "place occupied") are shown as toasts.
import type { Id, InputChannels } from "../api/types";
import { t } from "../i18n";
import type { Backend, Calls, RecordingFollow, View } from "./context";
import type { NoteRecordActions } from "./noteRecord";

export interface AudioRecordActions {
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

export function createAudioRecordActions(ctx: Backend & Calls & View & RecordingFollow, notes: Pick<NoteRecordActions, "setArmed">): AudioRecordActions {
  const { api } = ctx;

  /** Starts a take on all armed tracks from the current position (a stopped transport starts). */
  const start = async () => {
    await ctx.attempt(api.audioRecord.start());
    // Poll even after a refusal: if recording was already running, the banner must show it.
    ctx.followRecording();
  };

  const stop = async () => {
    // Read dropouts before stopping: afterwards the recording state is reset.
    const before = await ctx.run(api.audioRecord.status());
    const clips = await ctx.run(api.audioRecord.stop());
    ctx.followRecording();
    // A failed stop has already re-read the project (`run`), so the tracks that did record are shown.
    if (!clips) return;
    await ctx.showCreated({ clips: clips.map((c) => c.id) });
    ctx.toast("info", t().tracks.recordingAdded(clips.length));
    if (before && before.droppedFrames > 0) {
      ctx.toast("error", t().tracks.recordingDropped);
    }
  };

  return {
    async setArmed(track, armed) {
      await ctx.exec(api.audioRecord.setArmed(track, armed));
      ctx.followRecording();
    },
    async setInput(track, input) {
      await ctx.exec(api.audioRecord.setInput(track, input));
      ctx.followRecording();
    },
    stop,
    async pressRecord(notesArmed) {
      const status = await ctx.run(api.audioRecord.status());
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
