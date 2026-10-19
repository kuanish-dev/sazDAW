// The two place sections of the sound browser: "Project" (the source of all sounds, also the drop target for files from
// packs) and "Packs" (linked external folders for preview only; ✕ removes the link, files on disk are not touched).
import { For, createSignal } from "solid-js";
import type { SampleSource } from "../tracks/dropSample";
import { t } from "../i18n";
import { registerSampleDrop } from "../common/dragSample";
import { IconClose } from "../common/icons";
import type { createSoundPlaces } from "./soundPlaces";

type Places = ReturnType<typeof createSoundPlaces>;

export function PlaceTabs(props: { places: Places; onDropToProject: (source: SampleSource) => void }) {
  const places = () => props.places;
  const [over, setOver] = createSignal(false);
  const msg = () => t().sounds;

  return (
    <div class="flex flex-col gap-1.5 border-b border-line px-2 py-1.5" role="tablist" aria-label={msg().places}>
      <div class="flex items-center gap-1.5">
        <button
          class="btn !h-6 flex-1 text-xs font-medium" classList={{ "btn-on": places().inProject(), "ring-1 ring-accent": over() }}
          role="tab" aria-selected={places().inProject()}
          ref={(el) => registerSampleDrop(el, { over: () => setOver(true), leave: () => setOver(false), drop: (source) => { setOver(false); props.onDropToProject(source); } })}
          title={msg().projectHint}
          onClick={() => void places().openProject()}
        >{msg().project}</button>
      </div>
      <div class="flex flex-wrap items-center gap-1">
        <span class="label mr-0.5" title={msg().packsHint}>{msg().packs}</span>
        <For each={places().sources()}>
          {(path) => (
            <span class="inline-flex items-center overflow-hidden rounded" classList={{ "bg-accent text-on-accent": places().place() === path, "bg-panel-2": places().place() !== path }}>
              <button class="h-6 max-w-[110px] truncate px-2 text-xs" role="tab" aria-selected={places().place() === path} title={path} onClick={() => void places().openPlace(path)}>{places().placeName(path)}</button>
              <button
                class="flex h-6 w-5 items-center justify-center opacity-60 hover:bg-danger hover:text-white hover:opacity-100"
                title={msg().removePackHint} aria-label={msg().removePackAria(places().placeName(path))}
                onClick={() => void places().removeSource(path)}
              ><IconClose size={12} /></button>
            </span>
          )}
        </For>
        <button class="btn !h-6 text-xs" title={msg().addPackHint} onClick={() => void places().addSource()}>{msg().addPack}</button>
      </div>
    </div>
  );
}
