// Contract of the system dialogs (picking files and folders, confirmations) and shared file filters.
// Implementations: `tauriDialogs.ts` (app) and `mock/MockDialogs.ts` (browser); `composition.ts` picks one.
import { t } from "../i18n";

export interface FileFilter {
  name: string;
  extensions: string[];
}

interface PickOptions {
  title: string;
  filters?: FileFilter[];
  defaultPath?: string;
}

export interface SystemDialogs {
  openFile(options: PickOptions): Promise<string | null>;
  openFiles(options: PickOptions): Promise<string[]>;
  openDirectory(options: Pick<PickOptions, "title" | "defaultPath">): Promise<string | null>;
  saveFile(options: PickOptions): Promise<string | null>;
  /** A yes/no question; `buttons` replace the default "OK"/"Cancel" labels (`true` means `ok` was pressed). */
  confirm(message: string, title: string, buttons?: ConfirmButtons): Promise<boolean>;
}

export interface ConfirmButtons {
  ok: string;
  cancel: string;
}

// Filters with a translated name are functions: the name follows the current language.
export const projectFilter = (): FileFilter => ({ name: t().project.fileTypes.project, extensions: ["dawproj"] });
export const WAV_FILTER: FileFilter = { name: "WAV", extensions: ["wav"] };
export const FLAC_FILTER: FileFilter = { name: "FLAC", extensions: ["flac"] };
export const MP3_FILTER: FileFilter = { name: "MP3", extensions: ["mp3"] };
export const MIDI_FILTER: FileFilter = { name: "MIDI", extensions: ["mid", "midi"] };
export const audioFilter = (): FileFilter => ({
  name: t().project.fileTypes.audio,
  extensions: ["wav", "mp3", "flac", "ogg", "aif", "aiff", "m4a"],
});
