// The recording role as the session holds it: what recording shows (`recordingStatus.ts`: the take, track inputs,
// monitoring problems), audio takes on the armed tracks (`audioRecord.ts`) and note recording (`noteRecord.ts`).
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import { type AudioRecordActions, createAudioRecordActions } from "./audioRecord";
import { createNoteRecordActions, type NoteRecordActions } from "./noteRecord";
import { createRecordingStatus, type RecordingStatus } from "./recordingStatus";

export type RecordingRole = RecordingStatus & {
  readonly audioRecord: AudioRecordActions;
  /** Note recording: arming and the record quantize. */
  readonly noteRecord: NoteRecordActions;
};

export function createRecordingRole(core: Core, views: Views): RecordingRole {
  const status = createRecordingStatus(core.api.audioRecord.status, core.showFailure);
  const noteRecord = createNoteRecordActions(core);
  return { ...status, noteRecord, audioRecord: createAudioRecordActions(core, views, status, noteRecord) };
}
