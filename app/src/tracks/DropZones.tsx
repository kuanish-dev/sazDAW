// What the timeline shows while a sound is dragged: the clip "ghost" (placed by `timelineDrop.ghostAt`) and the
// "+ Instrument" and "+ Ready block" zones under the last track — where it's dropped decides what the sound becomes.
import { Show, createSignal } from "solid-js";
import type { NewTrackKind, SampleSource } from "./dropSample";
import { t } from "../i18n";
import { useSession } from "../session";
import { registerSampleDrop } from "../common/dragSample";
import type { DropGhost } from "./timelineDrop";

/** Translucent ghost while dragging a sample; labeled when it would replace the instrument. */
export function DropGhostView(props: { ghost: DropGhost | null }) {
  return (
    <Show when={props.ghost}>
      {(g) => (
        <div
          class="pointer-events-none absolute rounded border-2 border-dashed border-accent bg-accent/30 px-1.5 py-1 text-[11px] font-semibold text-accent"
          style={{ left: `${g().x}px`, top: `${g().y}px`, width: `${g().w}px`, height: `${g().h}px` }}
        >
          <Show when={g().mode === "instrument"}>{t().tracks.drop.becomesInstrument}</Show>
        </div>
      )}
    </Show>
  );
}

const ZONES: readonly NewTrackKind[] = ["instrument", "block"] as const;

/** The zones sit at the left edge of the visible area. */
export function DropZones(props: { onDrop: (source: SampleSource, kind: NewTrackKind) => void }) {
  const session = useSession();
  return (
    <div class="sticky left-0 flex w-[min(100%,44rem)] gap-2 p-2" style={{ height: `${session.settings.appearance.rowHeight()}px` }}>
      {ZONES.map((kind) => {
        const [over, setOver] = createSignal(false);
        return (
          <div
            class="flex flex-1 flex-col items-center justify-center rounded border-2 border-dashed border-accent px-2 text-center text-sm"
            classList={{ "bg-accent/30": over(), "bg-accent/10": !over() }}
            ref={(el) => registerSampleDrop(el, { over: () => setOver(true), leave: () => setOver(false), drop: (source) => { setOver(false); props.onDrop(source, kind); } })}
          >
            <span class="font-semibold text-accent">{t().tracks.drop.zones[kind].title}</span>
            <span class="text-[11px] text-muted">{t().tracks.drop.zones[kind].hint}</span>
          </div>
        );
      })}
    </div>
  );
}
