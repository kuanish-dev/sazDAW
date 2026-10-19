// The recording role: what recording shows (`recordingStatus.ts`: the take, track inputs, monitoring problems), audio
// takes on the armed tracks (`audioRecord.ts`) and note recording (`noteRecord.ts`, with the "Recorded N notes" notice).
import { onCleanup } from "solid-js";
import { createTakeNotice } from "../core/notifications";
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { type AudioRecordActions, createAudioRecordActions } from "./audioRecord";
import { createNoteRecordActions, type NoteRecordActions } from "./noteRecord";
import { createRecordingStatus, type RecordingStatus } from "./recordingStatus";

export type RecordingRole = RecordingStatus & {
  readonly audioRecord: AudioRecordActions;
  /** Note recording: arming and the record quantize. */
  readonly noteRecord: NoteRecordActions;
};

export const recordingRole = defineRole("recording", ({ core, use }): RecordingRole => {
  const status = createRecordingStatus(core.api.audioRecord.status, core.showFailure);
  onCleanup(status.dispose);
  // Track arming lives in the backend longer than the window: fetch it right away (polling sleeps if nothing is armed).
  status.followRecording();
  createTakeNotice(() => core.tick().record, core.toast);
  const noteRecord = createNoteRecordActions(core);
  return { ...status, noteRecord, audioRecord: createAudioRecordActions(core, use(shellRole), status, noteRecord) };
});

export const useRecording = () => useRole(recordingRole);
