// The Tracks page: headers on the left (tracks, folders, automation lanes), timeline on the right, below them the clip
// panel and the sound browser.
// The page scrolls vertically as a whole; the toolbar and the ruler strip stay pinned at the top (sticky). The rows scroll
// sideways in their own scroller, so the ruler lives outside it (CSS sticky does not work inside a scroller) in a clipped
// container whose horizontal offset follows the rows'; `Timeline` renders the ruler canvas into it.
import { Index, Match, Show, Switch, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { ownedLanes } from "../automation/automationRules";
import { BAR_GRID } from "../common/time";
import { rowLayout, rowTop } from "./rowLayout";
import { planTrackDrop, trackRows, type TrackRow } from "./trackRows";
import { t } from "../i18n";
import { ClipPanel } from "../pianoroll/ClipPanel";
import { SoundBrowser } from "../sounds/SoundBrowser";
import { FolderHeader } from "./FolderHeader";
import { ClipWavesContext, createClipWaves } from "./clipWaves";
import { BusLanesHeader, LaneHeader } from "./LaneHeader";
import { RULER_HEIGHT, type Tool } from "./timelineLayout";
import { Timeline } from "./Timeline";
import { TimelineToolbar, type ToolbarState } from "./TimelineToolbar";
import { TrackHeader } from "./TrackHeader";
import { createTrackReorder } from "./trackReorder";
import { useProjectCopy } from "../core/roles";
import { useSettings } from "../settings/role";
import { useTracks } from "./role";
import { useSamples } from "../samples/role";

export function TracksView() {
  const { project } = useProjectCopy();
  const settings = useSettings();
  const tracks = useTracks();
  const [tool, setTool] = createSignal<Tool>("draw");
  const [snap, setSnap] = createSignal<number>(BAR_GRID);
  const [zoom, setZoom] = createSignal(1);
  /** Header rows are the same as the timeline's: folders, visible tracks and their automation lanes. */
  const rows = createMemo(() => trackRows(project.tracks, project.folders, ownedLanes(project)));
  const layout = createMemo(() => rowLayout(rows(), settings.appearance.rowHeight()));
  const reorder = createTrackReorder({
    layout,
    drop: (id, slot) => {
      const plan = planTrackDrop(rows(), project.tracks, id, slot);
      if (plan) void tracks.folders.place(id, plan.folder, plan.index, plan.members);
    },
  });
  const state: ToolbarState = { tool, setTool, snap, setSnap, zoom, setZoom };

  /** The toolbar wraps onto more lines in a narrow window: the ruler is pinned right below its actual height. */
  let toolbarBox!: HTMLDivElement;
  const [toolbarHeight, setToolbarHeight] = createSignal(0);
  onMount(() => {
    const observer = new ResizeObserver(() => setToolbarHeight(toolbarBox.offsetHeight));
    observer.observe(toolbarBox);
    onCleanup(() => observer.disconnect());
  });
  const [rulerHost, setRulerHost] = createSignal<HTMLDivElement>();
  const waves = createClipWaves(useSamples());

  return (
    <ClipWavesContext.Provider value={waves}>
      <div class="flex min-h-full flex-col">
        <div ref={toolbarBox} class="sticky top-0 z-30"><TimelineToolbar state={state} /></div>
        <div class="flush flex flex-col divide-y divide-line">
        <div class="panel">
          <div class="sticky z-20 flex border-b border-line bg-panel" style={{ top: `${toolbarHeight()}px` }}>
            <div class="w-80 shrink-0 border-r border-line">
              <div class="flex items-center truncate px-3 text-[11px] leading-tight text-muted" style={{ height: `${RULER_HEIGHT}px` }}
                title={t().tracks.view.rulerHintLong}>{t().tracks.view.rulerHint}</div>
            </div>
            {/* Clipped, not scrollable by the user: its offset is copied from the rows' scroller below. */}
            <div ref={setRulerHost} class="min-w-0 flex-1 overflow-hidden" />
          </div>
          <div class="overflow-x-auto" onScroll={(e) => { rulerHost()!.scrollLeft = e.currentTarget.scrollLeft; }}>
            <div class="flex min-w-max">
              <div class="sticky left-0 z-10 w-80 shrink-0 border-r border-line bg-panel">
                {/* `Index`, not `For`: rows are rebuilt on every project edit and `For` would recreate the headers
                    in the middle of a knob turn; keyed by row index the header stays the same. */}
                <Index each={rows()} fallback={<p class="p-3 text-sm text-muted">{t().tracks.view.empty}</p>}>
                  {(row, index) => (
                    <Switch>
                      <Match when={row().type === "folder" ? (row() as Extract<TrackRow, { type: "folder" }>) : null}>
                        {(f) => <FolderHeader folder={f().folder} tracks={f().tracks} />}
                      </Match>
                      <Match when={row().type === "track" ? (row() as Extract<TrackRow, { type: "track" }>) : null}>
                        {(r) => <TrackHeader track={r().track} row={index} reorder={reorder} />}
                      </Match>
                      <Match when={row().type === "lane" ? (row() as Extract<TrackRow, { type: "lane" }>) : null}>
                        {(r) => <LaneHeader lane={r().lane} />}
                      </Match>
                      <Match when={row().type === "busLanes"}><BusLanesHeader /></Match>
                    </Switch>
                  )}
                </Index>
                <Show when={reorder.slot() !== null}>
                  <div class="pointer-events-none absolute inset-x-0 z-20 h-0.5 bg-accent" style={{ top: `${rowTop(reorder.slot()!, layout()) - 1}px` }} />
                </Show>
              </div>
              <Show when={rulerHost()}>{(host) => <Timeline state={state} rulerHost={host()} />}</Show>
            </div>
          </div>
        </div>
        <p class="px-3 py-1 text-[11px] leading-relaxed text-muted">{t().tracks.view.help}</p>
        <div class="grid min-h-[32rem] divide-line lg:grid-cols-[minmax(0,1fr)_26rem] lg:divide-x">
          <ClipPanel snap={snap()} />
          <SoundBrowser />
        </div>
        </div>
      </div>
    </ClipWavesContext.Provider>
  );
}
