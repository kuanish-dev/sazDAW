import { For, Show, createMemo, createSignal } from "solid-js";
import type { DrumPattern, PatternRole } from "../api/types";
import { ROLES, laneLabel, stylesOf } from "./drumPart";
import { t } from "../i18n";

/** Width of the drum label in the text grid of a pattern, characters. */
const LANE_LABEL_WIDTH = 10;

/** The pattern as text: one line per drum with its grid. */
function Grid(props: { pattern: DrumPattern }) {
  return (
    <pre class="mt-1 overflow-x-auto font-mono text-[10px] leading-4 text-muted">
      <For each={props.pattern.lanes}>{(l) => `${laneLabel(l.lane).padEnd(LANE_LABEL_WIDTH)} ${l.grid}\n`}</For>
    </pre>
  );
}

export function PatternBrowser(props: {
  patterns: DrumPattern[];
  /** Id of the pattern playing now. */
  playing: string | null;
  onAdd: (pattern: string) => void;
  onListen: (pattern: string) => void;
}) {
  const [role, setRole] = createSignal<PatternRole>("groove");
  const [style, setStyle] = createSignal("");
  const styles = createMemo(() => stylesOf(props.patterns, role()));
  const shown = createMemo(() => props.patterns.filter((p) => p.role === role() && (style() === "" || p.style === style())));

  const pickRole = (next: PatternRole) => {
    setRole(next);
    setStyle("");
  };

  const msg = () => t().drums;

  return (
    <div class="min-w-0">
      <div class="mb-2 flex flex-wrap items-center gap-1">
        <For each={ROLES}>{(r) => <button class="btn !h-7 text-xs" classList={{ "btn-on": role() === r }} title={msg().roles[r].hint} onClick={() => pickRole(r)}>{msg().roles[r].label}</button>}</For>
        <Show when={styles().length > 1}>
          <select class="field !h-7 ml-auto text-xs" aria-label={msg().genre} value={style()} onChange={(e) => setStyle(e.currentTarget.value)}>
            <option value="">{msg().allGenres}</option>
            <For each={styles()}>{(s) => <option value={s}>{s}</option>}</For>
          </select>
        </Show>
      </div>
      <div class="flex min-h-[320px] max-h-[52vh] flex-col gap-2 overflow-y-auto pr-1">
        <For each={shown()}>
          {(p) => (
            <div class="flex items-start gap-3 rounded border border-line bg-panel p-2 hover:border-field-line">
              <div class="min-w-0 flex-1">
                <div class="flex items-baseline gap-2"><span class="text-sm font-medium">{p.name}</span><span class="label">{p.style} · {msg().bars(p.bars)}</span></div>
                <div class="text-xs text-muted">{p.description}</div>
                <Grid pattern={p} />
              </div>
              <div class="flex shrink-0 flex-col gap-1">
                <button class="btn !h-7 text-xs" title={msg().addHint} onClick={() => props.onAdd(p.id)}>{msg().add}</button>
                <button class="btn !h-7 text-xs" classList={{ "btn-on": props.playing === p.id }} title={msg().listenHint} onClick={() => props.onListen(p.id)}>
                  {props.playing === p.id ? msg().stop : msg().listen}
                </button>
              </div>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
