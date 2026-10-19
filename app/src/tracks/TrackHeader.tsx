// Track header left of the timeline: reorder handle, name, instrument window or recording, delete and mixer strip.
// Aligned with the timeline row, whose height is the track's own: the bottom edge resizes it (`trackResize.ts`); a row
// too short for the mixer strip shows only the name line, with mute/solo in it. A sound dropped on an instrument track's
// header becomes its instrument.
// Right-click — the track menu (`TrackMenu.tsx`: color, folders, bounce). On the left — the track color stripe, and a glow of
// the track color that pulses with the track's level while it sounds.
import { Show, createSignal } from "solid-js";
import type { Track } from "../api/types";
import { isSource, sourceOfTrack } from "../graph/graphRules";
import { peakToMeter } from "../mixer/mixer";
import { chainBadge } from "../instruments/noteEffectRules";
import { trackHeight } from "./rowLayout";
import { t } from "../i18n";
import { useSession } from "../session";
import { menuPoint } from "../common/ContextMenu";
import { registerSampleDrop } from "../common/dragSample";
import { IconClose, IconGripper, IconSliders, IconSnowflake } from "../common/icons";
import { InlineRename } from "../common/InlineRename";
import { MuteSolo } from "../common/MuteSolo";
import { sticky } from "../common/sticky";
import { trackColorOf } from "../common/trackColors";
import { MonitorButton, RecordButton } from "./TrackInput";
import { TrackMenu } from "./TrackMenu";
import { TrackStrip } from "./TrackStrip";
import type { createTrackReorder } from "./trackReorder";
import { createTrackResize } from "./trackResize";

/** Below this height the mixer strip (knobs, meter) does not fit: the row shows only the name line. */
const STRIP_MIN_HEIGHT = 84;
/** The level glow at full scale: how strong (percent of the track color) and how far it reaches into the header. */
const PULSE_PERCENT = 35;
const PULSE_PX = 72;

/** `row` — the header's row from the top (with folders); reordering counts from it. */
export function TrackHeader(props: { track: Track; row: number; reorder: ReturnType<typeof createTrackReorder> }) {
  const session = useSession();
  const { tracks, soundPlacement } = session;
  const [renaming, setRenaming] = createSignal(false);
  /** Track index in the project: used by Alt+arrow moves and the default color. */
  const index = () => session.project.tracks.findIndex((track) => track.id === props.track.id);
  /** Where the track context menu is open (right-click on the header); `null` — closed. */
  const [menuAt, setMenuAt] = createSignal<{ x: number; y: number } | null>(null);
  /** A sound is dragged over an instrument track's header: dropping replaces its instrument. */
  const [dropOver, setDropOver] = createSignal(false);
  const source = () => sourceOfTrack(session.project.graph, props.track.id);
  /** Last known node: when the track is deleted, children don't crash on a stale value. */
  const lastSource = sticky(source);
  const height = () => trackHeight(props.track, session.appearance.rowHeight());
  const compact = () => height() < STRIP_MIN_HEIGHT;
  /** The source node's channel, for mute/solo in the name line of a compact row. */
  const channel = () => {
    const node = source();
    return node && isSource(node.kind) ? { node: node.id, mute: node.kind.mute, solo: node.kind.solo } : undefined;
  };
  /** `undefined` for the moment the track is gone from the project (a header being removed). */
  const color = () => trackColorOf(session.project.tracks, props.track.id) ?? undefined;
  /** A gradient of the track color from the left edge, as strong as the track is loud. A background, so it stays under the controls. */
  const pulse = () => {
    const node = source();
    const level = node ? session.levels[node.id] : undefined;
    const strength = level ? peakToMeter(Math.max(level.l.level, level.r.level)) : 0;
    const shade = color();
    if (strength === 0 || !shade) return undefined;
    const tint = `color-mix(in srgb, ${shade} ${Math.round(strength * PULSE_PERCENT)}%, transparent)`;
    return `linear-gradient(to right, ${tint}, transparent ${PULSE_PX}px)`;
  };
  const resize = createTrackResize({ track: () => props.track, fallback: session.appearance.rowHeight, actions: tracks });
  const monitorMode = () => {
    const kind = source()?.kind;
    return kind?.type === "audio" ? kind.monitor : null;
  };

  /** Without a mouse: Alt+↑/↓ on the header moves the track by one row. */
  const onKey = (e: KeyboardEvent) => {
    if (!e.altKey || e.target !== e.currentTarget || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    e.preventDefault();
    const target = index() + (e.key === "ArrowUp" ? -1 : 1);
    if (target >= 0 && target < session.project.tracks.length) void tracks.move(props.track.id, target);
  };

  return (
    <div
      data-track-row tabIndex={0} onKeyDown={onKey}
      aria-label={t().tracks.header.label(props.track.name)}
      class="relative flex flex-col justify-center gap-1 border-t border-line px-2"
      classList={{ "bg-select": session.selection.track() === props.track.id, "ring-2 ring-inset ring-accent": dropOver() }}
      ref={(el) => registerSampleDrop(el, {
        over: () => setDropOver(props.track.instrument !== null),
        leave: () => setDropOver(false),
        drop: (dropped) => { setDropOver(false); if (props.track.instrument) void soundPlacement.dropSample(dropped, props.track.id, 0); },
      })}
      style={{ height: `${height()}px`, "background-image": pulse() }}
      onPointerDown={() => session.selection.selectTrack(props.track.id)}
      onContextMenu={(e) => { e.preventDefault(); setMenuAt(menuPoint(e)); }}
    >
      <div class="pointer-events-none absolute inset-y-0 left-0 w-1" style={{ background: color() }} />
      <div class="flex items-center gap-1">
        <span
          class="-ml-0.5 shrink-0 cursor-grab touch-none select-none text-muted opacity-60 hover:text-text hover:opacity-100" title={t().tracks.header.dragHint} aria-hidden="true"
          onPointerDown={(e) => props.reorder.onDown(e, props.track.id, props.row)} onPointerMove={props.reorder.onMove}
          onPointerUp={props.reorder.onUp} onPointerCancel={props.reorder.onCancel}
        ><IconGripper size={14} /></span>
        <InlineRename
          name={props.track.name} editing={renaming()} class="!h-6 min-w-0 flex-1 text-sm"
          onRename={(name) => void tracks.rename(props.track.id, name)} onClose={() => setRenaming(false)}
        >
          <span class="min-w-0 flex-1 truncate text-sm" title={t().tracks.header.renameHint} onDblClick={() => setRenaming(true)}>{props.track.name}</span>
        </InlineRename>
        <Show when={props.track.instrument?.frozen != null}>
          <span class="shrink-0 text-blue" title={t().tracks.header.frozenHint} role="img" aria-label={t().tracks.header.frozen}><IconSnowflake size={14} /></span>
        </Show>
        <Show when={!props.track.instrument}>
          <RecordButton track={props.track.id} />
          <Show when={monitorMode()}>{(mode) => <MonitorButton track={props.track.id} mode={mode()} />}</Show>
        </Show>
        <Show when={props.track.instrument && chainBadge(props.track.noteEffects)}>
          {(badge) => (
            <button class="shrink-0 rounded-sm bg-accent/20 px-1 text-[10px] font-semibold leading-4 text-accent hover:bg-accent/30" title={t().tracks.header.noteEffectsHint}
              aria-label={t().tracks.header.noteEffects(badge())} onClick={() => session.ui.openInstrument(props.track.id)}>{badge()}</button>
          )}
        </Show>
        <Show when={props.track.instrument}>
          <button class="btn btn-icon !h-6 !w-6 !min-w-6" title={t().tracks.header.instrument} aria-label={t().tracks.header.instrument}
            onClick={() => session.ui.openInstrument(props.track.id)}><IconSliders size={14} /></button>
        </Show>
        <Show when={compact() && channel()} keyed>
          {(c) => <MuteSolo node={c.node} mute={c.mute} solo={c.solo} class="!h-6 !min-w-6 !px-0 text-[10px]" />}
        </Show>
        <button class="btn btn-icon !h-6 !w-6 !min-w-6 hover:!text-danger" title={t().tracks.header.delete} aria-label={t().tracks.header.delete} onClick={() => void tracks.remove(props.track.id)}><IconClose size={14} /></button>
      </div>
      <Show when={source() && !compact()}>
        <TrackStrip source={lastSource()!} />
      </Show>
      <div
        role="separator" aria-orientation="horizontal" tabIndex={0}
        aria-label={t().tracks.header.resize} title={t().tracks.header.resizeHint}
        class="absolute inset-x-0 -bottom-px z-10 h-1.5 cursor-ns-resize touch-none hover:bg-accent/40 focus-visible:bg-accent/60"
        onPointerDown={resize.onPointerDown} onPointerMove={resize.onPointerMove} onPointerUp={resize.onPointerUp}
        onPointerCancel={resize.onPointerCancel} onDblClick={resize.onDblClick} onKeyDown={resize.onKeyDown}
      />
      <Show when={menuAt()}>{(at) => <TrackMenu track={props.track} at={at()} onClose={() => setMenuAt(null)} />}</Show>
    </div>
  );
}
