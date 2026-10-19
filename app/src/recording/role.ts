// The recording role: what recording shows (`recordingStatus.ts`: the take, track inputs, monitoring problems), audio
// takes on the armed tracks (`audioRecord.ts`) and note recording (`noteRecord.ts`, with the "Recorded N notes" notice).
import { onCleanup } from "solid-js";
import { createTakeNotice } from "../core/notifications";
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { type AudioRecord, createAudioRecord } from "./audioRecord";
import { createNoteRecord, type NoteRecord } from "./noteRecord";
import { createRecordingStatus, type RecordingStatus } from "./recordingStatus";

export type RecordingRole = RecordingStatus & {
  readonly audioRecord: AudioRecord;
  /** Note recording: arming and the record quantize. */
  readonly noteRecord: NoteRecord;
};

export const recordingRole = defineRole("recording", ({ core, use }): RecordingRole => {
  const status = createRecordingStatus(core.api.audioRecord.status, core.notifications.showFailure);
  onCleanup(status.dispose);
  // Track arming lives in the backend longer than the window: fetch it right away (polling sleeps if nothing is armed).
  status.followRecording();
  createTakeNotice(() => core.playback.tick().record, core.notifications.toast);
  const noteRecord = createNoteRecord(core);
  return { ...status, noteRecord, audioRecord: createAudioRecord(core, use(shellRole), status, noteRecord) };
});

export const useRecording = () => useRole(recordingRole);
