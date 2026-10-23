import type { DawApi } from "../api/api";

/**
 * The user's home folder. A failure does not block the dialog: it is logged (console.error -> errorReporter)
 * and the dialog starts at the disk root.
 */
export async function homeDir(api: DawApi): Promise<string> {
  try {
    return await api.library.home();
  } catch (error) {
    console.error("library.home", error);
    return "/";
  }
}
