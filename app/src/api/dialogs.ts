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

export const projectFilter = (): FileFilter => ({ name: t().project.fileTypes.project, extensions: ["dawproj"] });
export const WAV_FILTER: Readonly<FileFilter> = { name: "WAV", extensions: ["wav"] } as const;
export const FLAC_FILTER: Readonly<FileFilter> = { name: "FLAC", extensions: ["flac"] } as const;
export const MP3_FILTER: Readonly<FileFilter> = { name: "MP3", extensions: ["mp3"] } as const;
export const MIDI_FILTER: Readonly<FileFilter> = { name: "MIDI", extensions: ["mid", "midi"] } as const;
export const audioFilter = (): FileFilter => ({
  name: t().project.fileTypes.audio,
  extensions: ["wav", "mp3", "flac", "ogg", "aif", "aiff", "m4a"],
});
