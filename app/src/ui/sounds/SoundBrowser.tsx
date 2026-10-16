// Sound browser next to the timeline. The single source of sounds is the project folder; "packs" (linked folders) exist
// only for easy previewing: previewing copies nothing, a sound is copied into the project when it is taken into work.
import { For, createEffect, createSignal, on, onCleanup } from "solid-js";
import type { LibraryEntry, SampleInfo } from "../../api/types";
import type { SampleSource } from "../../domain/dropSample";
import { clamp } from "../../domain/math";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { registerSampleDrop } from "../common/dragSample";
import { IconArrowUp } from "../common/icons";
import { SampleDragLabel } from "../common/SampleDragLabel";
import { PlaceTabs } from "./PlaceTabs";
import { createSoundPlaces } from "./soundPlaces";
import { SoundRow, type SoundRowActions } from "./SoundRow";

export function SoundBrowser() {
  const session = useSession();
  const { preview, samples, soundPlacement } = session.actions;
  const places = createSoundPlaces();
  // Which project folder files are already samples (for the row's "✕"): re-read on any change of the sample list.
  const [projectSamples, setProjectSamples] = createSignal<SampleInfo[]>([]);
  createEffect(on(() => session.project.samples.map((s) => s.id).join(","), async () => setProjectSamples(await samples.samples())));
  const { dir, entries, root, load, shown } = places;
  const [selected, setSelected] = createSignal<string | null>(null);
  let list!: HTMLDivElement;

  onCleanup(() => preview.stopQuietly());
  createEffect(on(dir, () => setSelected(null), { defer: true }));
  // A new project file (a copy from a pack, an extracted sound) appears in the list without a manual refresh.
  createEffect(on(() => session.project.samples.length, () => places.inProject() && void places.reload(), { defer: true }));

  const files = () => entries().filter((e) => !e.isDir);
  const playing = () => session.previewPlayhead() !== null;

  const audition = (entry: LibraryEntry) => {
    setSelected(entry.path);
    void preview.playFile(entry.path);
  };

  /** Click: a folder opens; a file plays, and clicking the playing one again stops it. */
  const pick = (entry: LibraryEntry) => {
    if (entry.isDir) return void load(entry.path);
    if (selected() === entry.path && playing()) return void preview.stop();
    audition(entry);
  };

  const actions: SoundRowActions = {
    addTrack: (entry) => void soundPlacement.addSoundToTrack(entry.path),
    toProject: (entry) => void samples.importSample(entry.path).then(() => places.reload()),
    toTimeline: (entry) => void soundPlacement.addSoundToTimeline(entry.path),
    toEditor: (entry) => void soundPlacement.editSound(entry.path),
    remove: (sample) => void samples.removeSample(sample),
  };

  /** A pack file dropped on "Project" is copied into the project; anything else (already in the project) is ignored. */
  const dropToProject = async (source: SampleSource) => {
    if (source.kind !== "file" || places.isProjectPath(source.path)) return;
    if (!(await samples.importSample(source.path))) return;
    if (places.inProject()) await places.reload();
  };

  /** OS dialog: files from anywhere on the computer are copied into the project. */
  const importFiles = async () => {
    if ((await samples.importFiles(root() ?? undefined)).length && places.inProject()) await places.reload();
  };

  const onKey = (event: KeyboardEvent) => {
    if ((event.target as HTMLElement).closest("button")) return;
    const all = files();
    const index = all.findIndex((e) => e.path === selected());
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = all[clamp(index + (event.key === "ArrowDown" ? 1 : -1), 0, all.length - 1)];
      if (next) {
        audition(next);
        list.querySelector(`[data-path="${CSS.escape(next.path)}"]`)?.scrollIntoView({ block: "nearest" });
      }
    } else if (event.key === "Escape") {
      void preview.stop();
    } else if (event.key === " " && all[index]) {
      event.preventDefault();
      audition(all[index]!);
    } else if (event.key === "Enter" && all[index]) {
      event.preventDefault();
      actions.addTrack(all[index]!);
    }
  };

  const msg = () => t().sounds;

  return (
    <section class="panel flex h-full min-h-0 min-w-0 flex-col" aria-label={msg().browser}>
      <SampleDragLabel />
      <div class="section-header !h-[30px] !pr-1">
        <h2 class="mr-auto">{msg().title}</h2>
        <button class="btn btn-icon !h-6 !w-6 !min-w-6" title={msg().up} aria-label={msg().up} disabled={places.atRoot()} onClick={() => void places.up()}><IconArrowUp size={14} /></button>
        <button class="btn btn-ghost !h-6 !px-1.5 text-xs font-normal normal-case tracking-normal text-text" title={msg().importHint} onClick={() => void importFiles()}>{msg().import}</button>
      </div>
      <PlaceTabs places={places} onDropToProject={(source) => void dropToProject(source)} />
      <div class="truncate border-b border-line px-3 py-1 text-[11px] text-muted" title={dir()}>
        {places.inProject() ? msg().projectPath(shown()) : msg().packPath(places.placeName(places.place()), shown())}
      </div>
      <div
        data-owns-space class="min-h-0 flex-1 overflow-auto p-1 outline-none" tabIndex={0} role="listbox" aria-label={msg().files}
        onKeyDown={onKey}
        ref={(el) => { list = el; registerSampleDrop(el, { drop: (source) => places.inProject() && void dropToProject(source) }); }}
      >
        <For each={entries()} fallback={<p class="p-3 text-sm text-muted">{places.inProject() ? msg().emptyProject : msg().emptyFolder}</p>}>
          {(entry) => (
            <SoundRow
              entry={entry} project={places.isProjectPath(entry.path)} selected={selected() === entry.path}
              sample={projectSamples().find((s) => s.path === entry.path)} actions={actions} onPick={() => pick(entry)}
            />
          )}
        </For>
      </div>
    </section>
  );
}
