import { createEffect, createResource, createSignal, For, on, Show } from "solid-js";
import type { HistorySteps } from "../api/types";
import { t } from "../i18n";
import { IconHistory, IconSparkle } from "../common/icons";
import { closeOnOutside } from "../common/pointer";
import { useProjectCopy } from "../core/roles";
import { useHistory } from "./role";

/** How the AI labels its squashed requests (the MCP instructions in `daw-mcp/src/lib.rs`: "AI: drum part"). */
const AI_PREFIX = "AI:";

/** A timeline row: a step, or the start of the history (`label: null`). `offset` is the walk that makes it the current
 *  point: undo steps when negative, redo steps when positive, 0 — the project is here. */
interface Row {
  label: string | null;
  offset: number;
}

/** Newest first: undone steps (the furthest redo on top), done steps, and the start under them all. */
function timeline(steps: HistorySteps): Row[] {
  const undone = steps.undone.map((label, i) => ({ label, offset: i + 1 }));
  const done = steps.done.map((label, i) => ({ label, offset: i + 1 - steps.done.length }));
  return [...undone.reverse(), ...done.reverse(), { label: null, offset: -steps.done.length }];
}

export function HistoryButton() {
  const [open, setOpen] = createSignal(false);
  return (
    <div class="relative">
      <button
        class="btn btn-icon" classList={{ "btn-on": open() }} aria-expanded={open()}
        title={t().project.history.open} aria-label={t().project.history.open} onClick={() => setOpen(!open())}
      ><IconHistory /></button>
      <Show when={open()}><HistoryPanel onClose={() => setOpen(false)} /></Show>
    </div>
  );
}

function HistoryPanel(props: { onClose: () => void }) {
  const { status } = useProjectCopy();
  const history = useHistory();
  const [steps] = createResource(status, () => history.steps());
  const rows = () => {
    const latest = steps.latest;
    return latest ? timeline(latest) : [];
  };
  const [walking, setWalking] = createSignal(false);
  let root!: HTMLDivElement;
  let list: HTMLDivElement | undefined;

  closeOnOutside(() => root, props.onClose);
  createEffect(on(rows, () => queueMicrotask(() => list?.querySelector('[aria-current="step"]')?.scrollIntoView({ block: "nearest" }))));

  const go = async (offset: number) => {
    if (offset === 0 || walking()) return;
    setWalking(true);
    await history.travel(offset);
    setWalking(false);
  };

  const msg = () => t().project.history;
  const hintOf = (row: Row) => {
    if (row.offset === 0) return msg().here;
    const walk = row.offset > 0 ? msg().redoTo(row.offset) : msg().undoTo(-row.offset);
    return row.label === null ? `${msg().startHint}. ${walk}` : walk;
  };
  const isEmpty = () => (steps.latest?.done.length ?? 0) + (steps.latest?.undone.length ?? 0) === 0;

  return (
    <div ref={root} class="anim-pop absolute right-0 top-8 z-40 flex w-80 origin-top-right flex-col rounded border border-field-line bg-bg shadow-xl" role="dialog" aria-label={msg().title}>
      <div class="section-header justify-between border-b border-line !h-8 !px-3">
        <span class="flex items-center gap-1.5"><IconHistory size={13} />{msg().title}</span>
        <Show when={steps.latest}>{(s) => <span class="font-normal normal-case tracking-normal">{msg().count(s().done.length, s().undone.length)}</span>}</Show>
      </div>
      <Show
        when={!isEmpty()}
        fallback={<p class="px-4 py-6 text-center text-xs leading-relaxed text-muted">{msg().empty}</p>}
      >
        <p class="px-3 pt-2 text-[11px] text-muted">{msg().hint}</p>
        <div ref={list} class="max-h-[min(70vh,520px)] overflow-y-auto px-1.5 py-1.5" classList={{ "pointer-events-none opacity-80": walking() }}>
          <For each={rows()}>{(row, i) => <HistoryRow row={row} top={i() === 0} hint={hintOf(row)} onPick={() => void go(row.offset)} />}</For>
        </div>
      </Show>
    </div>
  );
}

function HistoryRow(props: { row: Row; top: boolean; hint: string; onPick: () => void }) {
  const msg = () => t().project.history;
  const undone = () => props.row.offset > 0;
  const current = () => props.row.offset === 0;
  const start = () => props.row.label === null;
  const ai = () => props.row.label?.startsWith(AI_PREFIX) === true;
  const text = () => {
    const label = props.row.label;
    if (label === null) return msg().start;
    return (ai() && label.slice(AI_PREFIX.length).trim()) || label;
  };
  const segment = (dashed: boolean) => (dashed ? "border-l border-dashed border-muted/60" : "border-l-2 border-accent/45");

  return (
    <button
      class="relative flex h-8 w-full items-center gap-2.5 rounded-sm pl-1 pr-2 text-left text-sm transition-[background-color,opacity] duration-100"
      classList={{
        "bg-select font-medium": current(),
        "hover:bg-hover": !current(),
        "opacity-50 hover:opacity-100": undone(),
        "bg-ai/10": ai() && !current(),
      }}
      aria-current={current() ? "step" : undefined}
      title={props.hint}
      onClick={() => props.onPick()}
    >
      <span class="relative flex w-5 shrink-0 self-stretch items-center justify-center">
        <Show when={!props.top}>
          <span class={`absolute left-1/2 top-0 h-1/2 -translate-x-1/2 ${segment(props.row.offset >= 0)}`} />
        </Show>
        <Show when={!start()}>
          <span class={`absolute bottom-0 left-1/2 h-1/2 -translate-x-1/2 ${segment(undone())}`} />
        </Show>
        <Dot ai={ai()} current={current()} undone={undone()} start={start()} />
      </span>
      <Show when={ai()}>
        <span class="shrink-0 rounded-sm bg-ai/20 px-1 text-[10px] font-bold leading-4 text-ai">{msg().ai}</span>
      </Show>
      <span class="min-w-0 flex-1 truncate" classList={{ "italic text-muted": start() }}>{text()}</span>
      <Show when={current()}>
        <span class="anim-toggle shrink-0 rounded-sm bg-accent px-1.5 text-[10px] font-bold uppercase leading-4 tracking-wide text-on-accent">{msg().now}</span>
      </Show>
    </button>
  );
}

function Dot(props: { ai: boolean; current: boolean; undone: boolean; start: boolean }) {
  return (
    <span class="relative z-10 flex items-center justify-center">
      <Show
        when={props.ai}
        fallback={
          <span
            class="block rounded-full"
            classList={{
              "h-3 w-3 bg-accent ring-4 ring-accent/25 anim-toggle": props.current && !props.start,
              "h-2.5 w-2.5 border border-muted bg-bg": props.undone,
              "h-2 w-2 bg-accent/80": !props.current && !props.undone && !props.start,
              "h-2.5 w-2.5 rotate-45 !rounded-[1px] border border-muted bg-bg": props.start && !props.current,
              "h-3 w-3 rotate-45 !rounded-[1px] bg-accent ring-4 ring-accent/25": props.start && props.current,
            }}
          />
        }
      >
        <span class="flex rounded-full bg-bg" classList={{ "ring-4 ring-ai/25 anim-toggle": props.current }}>
          <IconSparkle size={props.current ? 15 : 13} class="text-ai" />
        </span>
      </Show>
    </span>
  );
}
