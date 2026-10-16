// The track timeline: a canvas with the clips of all tracks (rows — tracks, folders and automation lanes,
// `domain/trackRows` — match the headers on the left) and the ruler strip above them. The ruler is a separate canvas rendered into `rulerHost`, a container that
// `TracksView` keeps on screen while the page scrolls and scrolls sideways together with the rows. This is the assembly:
// gesture drafts, playhead, cue and wheel zoom; drawing is `timelineRender` (rows), `activityRender` (what is sounding: lit notes
// over the rows, redrawn every frame while playing) and `rulerRender` (ruler), the mouse is
// `lanePointer` on automation lane rows, `timelinePointer` on the other rows and, on the ruler, `tempoPointer` (signature flags), `markerPointer` (marker
// flags) and `rulerPointer` (cue, loop); sound drop — `timelineDrop`, the audio clip menu — `ClipMenu`.
import { Show, createEffect, createMemo, createSignal } from "solid-js";
import { Portal } from "solid-js/web";
import type { Clip, Marker } from "../../api/types";
import { autoClipEnd, ownedLanes, targetLabel, targetRange } from "../../domain/automation";
import { clipLabel } from "../../domain/clips";
import { mutedTracks } from "../../domain/graph";
import { rowLayout, timelineHeight } from "../../domain/rowLayout";
import { lanesOf, trackRows } from "../../domain/trackRows";
import { useSession } from "../../state/session";
import { isZoomWheel, localPoint } from "../common/pointer";
import { createScrollWindow } from "../common/scrollWindow";
import { InlineRename } from "../common/InlineRename";
import { palette } from "../common/theme";
import { clipColorOf } from "../common/trackColors";
import { drawActivity } from "./activityRender";
import { useClipWaves } from "./clipWaves";
import { createLanePointer, type LaneDraft } from "./lanePointer";
import type { LaneRowView } from "./laneRender";
import { DropGhostView, DropZones } from "./DropZones";
import { MARKER_TOP } from "./markerLayout";
import { createMarkerPointer } from "./markerPointer";
import { drawRuler } from "./rulerRender";
import { createRulerPointer } from "./rulerPointer";
import { TempoMenu } from "./TempoMenu";
import { createTempoPointer } from "./tempoPointer";
import {
  BASE_PX_PER_TICK, RULER_HEIGHT, anchoredScroll, applyDraft, tickAtX, visibleBars, zoomByWheel, type ClipDraft, type PixelRect,
} from "./timelineLayout";
import { createTimelineDrop } from "./timelineDrop";
import { createTimelinePointer, type ClipMenu as Menu, type DragTip } from "./timelinePointer";
import { ClipMenu } from "./ClipMenu";
import { drawTimeline, timelineWidth } from "./timelineRender";
import type { ToolbarState } from "./TimelineToolbar";

export function Timeline(props: { state: ToolbarState; rulerHost: HTMLElement }) {
  const session = useSession();
  const { project, selection } = session;
  const { soundPlacement } = session.actions;
  const [draft, setDraft] = createSignal<ClipDraft | null>(null);
  const [laneDraft, setLaneDraft] = createSignal<LaneDraft | null>(null);
  const [marquee, setMarquee] = createSignal<PixelRect | null>(null);
  const [loopDraft, setLoopDraft] = createSignal<[number, number] | null>(null);
  const [tip, setTip] = createSignal<DragTip | null>(null);
  const [clipMenu, setClipMenu] = createSignal<Menu | null>(null);
  let canvas!: HTMLCanvasElement;
  let activity!: HTMLCanvasElement;
  let surface!: HTMLDivElement;
  let rulerCanvas!: HTMLCanvasElement;
  let rulerSurface!: HTMLDivElement;
  /** The ruler is as wide as the rows and scrolled with them, so both canvases draw the same window. */
  const viewport = createScrollWindow(() => surface);
  const cue = createMemo(() => session.tick().cue);

  const pxPerTick = () => BASE_PX_PER_TICK * props.state.zoom();
  const clips = createMemo<Clip[]>(() => applyDraft(project.clips, draft()));
  const selected = createMemo(() => new Set(selection.clips()));
  const waves = useClipWaves();
  const rows = createMemo(() => trackRows(project.tracks, project.folders, ownedLanes(project)));
  const lanes = createMemo(() => lanesOf(rows()));
  /** Rows differ in height: a track's own or the appearance setting. */
  const layout = createMemo(() => rowLayout(rows(), session.appearance.rowHeight()));
  const clipColor = (clip: Clip) => clipColorOf(project.tracks, clip);

  const tempo = createTempoPointer({ surface: () => rulerSurface, map: () => project.tempoMap, pxPerTick });

  const markers = createMarkerPointer({
    surface: () => rulerSurface,
    markers: () => project.markers,
    pxPerTick,
    tempoMap: () => project.tempoMap,
    actions: session.actions.markers,
    seek: (tick) => void session.actions.transport.seek(tick),
  });
  /** Markers with the drag draft applied. */
  const shownMarkers = createMemo<Marker[]>(() => {
    const moving = markers.draft();
    if (!moving) return project.markers;
    return project.markers.map((m) => (m.id === moving.id ? { ...m, tick: moving.tick } : m)).sort((a, b) => a.tick - b.tick);
  });

  const ruler = createRulerPointer({
    surface: () => rulerSurface,
    pxPerTick,
    snap: props.state.snap,
    tempoMap: () => project.tempoMap,
    cue,
    cueAt: (tick) => void session.actions.transport.cueAt(tick),
    clearCue: () => void session.actions.transport.clearCue(),
    setLoop: (region) => void session.actions.transport.setLoopRegion(region),
    loopDraft,
    setLoopDraft,
  });

  const pointer = createTimelinePointer({
    surface: () => surface,
    tracks: lanes,
    clips: () => project.clips,
    tool: props.state.tool,
    snap: props.state.snap,
    tempoMap: () => project.tempoMap,
    pxPerTick,
    layout,
    actions: session.actions.clips,
    selected: selection.clips,
    select: (ids, track) => selection.selectClips(ids, track ?? undefined),
    draft,
    setDraft,
    setMarquee,
    sampleRate: (clip) => (clip.content.type === "audio" ? waves.sampleRateOf(clip.content.sample) : undefined),
    setTip,
    openMenu: setClipMenu,
  });

  const lanePointer = createLanePointer({
    surface: () => surface,
    rows,
    snap: props.state.snap,
    tempoMap: () => project.tempoMap,
    pxPerTick,
    layout,
    actions: session.actions.automation,
    draft: laneDraft,
    setDraft: setLaneDraft,
  });

  /** Automation lane rows to draw, with the drag draft applied. */
  const automationRows = (window: { left: number; width: number }): LaneRowView[] => {
    const moving = laneDraft();
    const chosen = selection.automation();
    return rows().flatMap((row, index) => {
      if (row.type !== "lane") return [];
      const { lane } = row;
      return [{
        lane,
        row: index,
        clips: moving?.lane === lane.id ? lane.clips.map((c) => (c.id === moving.clip.id ? moving.clip : c)) : lane.clips,
        selected: chosen?.lane === lane.id ? chosen.clip : null,
        range: targetRange(lane.target, project, session.catalog()),
        label: targetLabel(lane.target, project, session.catalog()),
        pxPerTick: pxPerTick(),
        layout: layout(),
        window,
      }];
    });
  };
  /** Where the last automation clip ends: the timeline is at least that long. */
  const automationEnd = () => Math.max(0, ...project.automation.flatMap((l) => l.clips.map(autoClipEnd)));
  const bars = () => visibleBars(project.clips, automationEnd());

  const drop = createTimelineDrop({
    tracks: lanes,
    clips: () => project.clips,
    snap: props.state.snap,
    tempoMap: () => project.tempoMap,
    pxPerTick,
    layout,
    drop: (source, track, tick) => void soundPlacement.dropSample(source, track, tick),
  });

  const scroller = () => surface.closest<HTMLElement>(".overflow-x-auto");
  /** Ctrl/Cmd + wheel over the timeline zooms; the point under the cursor stays in place. */
  const onWheel = (event: WheelEvent) => {
    if (!isZoomWheel(event)) return;
    event.preventDefault();
    // The ruler and the rows share horizontal coordinates, so the rows surface measures for both.
    const tick = tickAtX(localPoint(surface, event).x, pxPerTick());
    props.state.setZoom(zoomByWheel(props.state.zoom(), event.deltaY));
    const rows = scroller();
    if (!rows) return;
    const scrollerLeft = rows.getBoundingClientRect().left;
    const canvasLeftInContent = surface.getBoundingClientRect().left - scrollerLeft + rows.scrollLeft;
    rows.scrollLeft = anchoredScroll(tick, event.clientX - scrollerLeft, pxPerTick(), canvasLeftInContent);
  };

  /** The ruler itself does not scroll (`TracksView` scrolls it with the rows): a sideways wheel over it scrolls the rows. */
  const onRulerWheel = (event: WheelEvent) => {
    if (isZoomWheel(event)) return onWheel(event);
    const rows = scroller();
    if (!rows || event.deltaX === 0) return;
    event.preventDefault();
    rows.scrollLeft += event.deltaX;
  };

  /** The playhead is a separate element (one on the rows, one on the ruler): it moves without redrawing the canvases. */
  let playheadEl!: HTMLDivElement;
  let rulerPlayheadEl!: HTMLDivElement;
  createEffect(() => {
    const position = session.smoothPlayhead();
    // Step in device pixels: on a high-density screen the playhead moves in steps finer than a CSS pixel.
    const ratio = window.devicePixelRatio || 1;
    const shift = position === null ? "" : `translateX(${Math.round(position * pxPerTick() * ratio) / ratio}px)`;
    for (const el of [playheadEl, rulerPlayheadEl]) {
      el.style.display = position === null ? "none" : "block";
      el.style.transform = shift;
    }
  });

  const width = createMemo(() => timelineWidth({ bars: bars(), pxPerTick: pxPerTick() }));
  /** The part of the timeline on screen: all three canvases (rows, glow layer, ruler) cover only it. */
  const shownWindow = createMemo(() => {
    const win = viewport();
    return { left: win.left, width: Math.max(1, Math.min(win.width, width() - win.left)) };
  });
  const muted = createMemo(() => mutedTracks(project.tracks, project.graph));

  createEffect(() => {
    const shown = shownWindow();
    const view = {
      rows: rows(),
      lanes: lanes(),
      clips: clips(),
      clipColor,
      tempoMap: project.tempoMap,
      markers: shownMarkers(),
      selected: selected(),
      marquee: marquee(),
      loop: loopDraft() ?? project.loopRegion,
      cue: cue(),
      pxPerTick: pxPerTick(),
      layout: layout(),
      bars: bars(),
      mutedTracks: muted(),
      clipName: (clip: Clip) => clipLabel(clip, project),
      wave: (clip: Clip, w: number) => waves.columnsFor(clip, w, project.tempoMap),
      window: shown,
      automation: automationRows(shown),
    };
    canvas.style.transform = `translateX(${view.window.left}px)`;
    drawTimeline(canvas, view);
  });

  // The glow layer follows the playhead on every screen frame, but draws only the clips under it.
  createEffect(() => {
    const shown = shownWindow();
    activity.style.transform = `translateX(${shown.left}px)`;
    drawActivity(activity, {
      clips: clips(),
      lanes: lanes(),
      clipColor,
      muted: muted(),
      pxPerTick: pxPerTick(),
      layout: layout(),
      window: shown,
      position: session.smoothPlayhead(),
    });
  });

  createEffect(() => {
    const shown = shownWindow();
    rulerCanvas.style.transform = `translateX(${shown.left}px)`;
    drawRuler(rulerCanvas, {
      tempoMap: project.tempoMap,
      markers: shownMarkers(),
      loop: loopDraft() ?? project.loopRegion,
      cue: cue(),
      pxPerTick: pxPerTick(),
      bars: bars(),
      window: shown,
    });
  });

  // A fragment, not a child of the rows surface: events bubble from a portal to where it stands, and the ruler's
  // presses must not reach the rows' pointer.
  return (
    <>
      <Portal mount={props.rulerHost}>
        <div ref={rulerSurface} class="relative touch-none" style={{ width: `${width()}px`, height: `${RULER_HEIGHT}px` }}
          onPointerDown={(e) => markers.onDown(e) || ruler.onDown(e)}
          onPointerMove={(e) => markers.onMove(e) || ruler.onMove(e)}
          onPointerUp={() => markers.onUp() || ruler.onUp()}
          onDblClick={(e) => tempo.onDouble(e) || markers.onDouble(e)}
          onContextMenu={(e) => tempo.onContext(e) || ruler.onCueContext(e) || markers.onContext(e) || ruler.onContext(e)}
          onWheel={onRulerWheel}>
          <canvas ref={rulerCanvas} class="absolute left-0 top-0 block" />
          <Show when={tempo.menu()} keyed>
            {(menu) => <TempoMenu menu={menu} onClose={tempo.closeMenu} />}
          </Show>
          {/* A marker name is edited in a field right on its flag. */}
          <Show when={markers.renaming()} keyed>
            {(marker) => (
              <div class="absolute z-10" style={{ left: `${marker.tick * pxPerTick()}px`, top: `${MARKER_TOP - 2}px` }}>
                <InlineRename name={marker.name} editing class="!h-4 w-36 !px-1 text-[10px]"
                  onRename={(name) => void session.actions.markers.rename(marker.id, name)} onClose={markers.stopRenaming}>{null}</InlineRename>
              </div>
            )}
          </Show>
          <div ref={rulerPlayheadEl} class="playhead playhead-cap pointer-events-none absolute left-0 top-0 h-full w-px will-change-transform" style={{ background: palette().ink, display: "none" }} />
        </div>
      </Portal>
      <div ref={(el) => { surface = el; drop.attach(el); }} class="relative touch-none"
        style={{ width: `${width()}px` }}
        onPointerDown={(e) => lanePointer.onDown(e) || pointer.onDown(e)}
        onPointerMove={(e) => lanePointer.onMove(e) || pointer.onMove(e)}
        onPointerUp={() => lanePointer.onUp() || void pointer.onUp()}
        onDblClick={(e) => lanePointer.onDouble(e) || pointer.onDouble(e)}
        onContextMenu={(e) => lanePointer.onContext(e) || pointer.onContext(e)} onWheel={onWheel}>
        <canvas ref={canvas} class="absolute left-0 top-0 block" />
        <canvas ref={activity} class="pointer-events-none absolute left-0 top-0 block" aria-hidden="true" />
        {/* The canvas covers only the visible window; this block reserves the full height of the rows. */}
        <div style={{ height: `${timelineHeight(layout())}px` }} />
        <div ref={playheadEl} class="playhead pointer-events-none absolute left-0 top-0 h-full w-px will-change-transform" style={{ background: palette().ink, display: "none" }} />
        <Show when={tip()} keyed>
          {(hint) => (
            <div class="pointer-events-none absolute z-10 rounded border border-field-line bg-bg px-1.5 py-0.5 text-[11px] tabular-nums shadow-md" style={{ left: `${hint.x}px`, top: `${hint.y}px` }}>
              {hint.text}
            </div>
          )}
        </Show>
        <Show when={clipMenu()} keyed>
          {(menu) => <ClipMenu menu={menu} onClose={() => setClipMenu(null)} />}
        </Show>
        <DropGhostView ghost={drop.ghost()} />
        {/* While a sample is dragged, two zones under the last track — "instrument" and "ready block" — create a new track. */}
        <Show when={drop.active()}><DropZones onDrop={(source, kind) => void soundPlacement.dropAsNewTrack(source, kind)} /></Show>
      </div>
    </>
  );
}
