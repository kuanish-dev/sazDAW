// "Places" for browsing sounds: the project folder and linked folders. The browser never goes above a place's root,
// so the whole computer is not visible. "Project" is the source of all sounds, linked folders ("packs") are for preview only.
import { createEffect, createSignal, on } from "solid-js";
import type { LibraryEntry } from "../../api/types";
import { basename, joinPath, parentDir } from "../../domain/format";
import { t } from "../../i18n";
import { useSession } from "../../state/session";

export function createSoundPlaces() {
  const session = useSession();
  const lib = session.actions.soundBrowser;
  const [dir, setDir] = createSignal("/");
  const [entries, setEntries] = createSignal<LibraryEntry[]>([]);
  const [sources, setSources] = createSignal<string[]>([]);
  /** Root of the current place (the project folder or a link). */
  const [place, setPlace] = createSignal("");

  const root = () => session.status().folder;
  const samplesFolder = () => (root() ? joinPath(root()!, "samples") : undefined);

  const load = async (path: string): Promise<boolean> => {
    const list = await lib.listSounds(path);
    if (!list) return false;
    setEntries(list);
    setDir(path);
    return true;
  };

  const loadSources = async () => setSources(await lib.soundSources());

  /** Opens a place: the project folder (straight into samples/, which every project folder has) or a link. */
  const openPlace = async (path: string, start = path) => {
    if (await load(start)) setPlace(path);
  };

  // The root is the project folder: when another project opens, the browser moves to its samples/.
  createEffect(on(root, (folder) => {
    if (folder) void openPlace(folder, joinPath(folder, "samples"));
  }));
  void loadSources();

  const placeName = (path: string) => (path === root() ? t().sounds.projectName : basename(path) || path);

  /** Path inside the place, for the caption. */
  const shown = () => {
    const base = place();
    const path = dir();
    return base && path.startsWith(base) ? path.slice(base.length).replace(/^\//, "") || placeName(base) : path;
  };

  const addSource = async () => {
    const folder = await lib.addSoundSource(place());
    if (!folder) return;
    await loadSources();
    await openPlace(folder);
  };

  const removeSource = async (path: string) => {
    await lib.removeSoundSource(path);
    await loadSources();
    if (place() === path) await openPlace(root() ?? "", samplesFolder());
  };

  /** The project (not a pack) is open now. */
  const inProject = () => !!root() && place() === root();
  /** The file is in the project folder (not in a pack). */
  const isProjectPath = (path: string) => !!root() && path.startsWith(`${root()}/`);

  return {
    inProject, isProjectPath, reload: () => load(dir()),
    dir, entries, sources, place, root, load, openPlace, addSource, removeSource, placeName, shown,
    openProject: () => root() && openPlace(root()!, samplesFolder()),
    up: () => load(parentDir(dir())),
    atRoot: () => dir() === place(),
  };
}
