// Settings → Keyboard shortcuts: read-only help. Rows come from the same tables the keys work by
// (window, clips, piano roll, keyboard piano, graph, automation pattern), so the help never drifts from the behavior.
import { For } from "solid-js";
import { keyboardPianoHelp } from "../../domain/keyboard";
import type { ShortcutHelp } from "../../domain/shortcuts";
import { t, type Messages } from "../../i18n";
import { clipKeysHelp } from "../../state/clipKeys";
import { globalKeysHelp } from "../../state/globalKeys";
import { PATTERN_KEYS_HELP } from "../automation/PatternEditor";
import { GRAPH_KEYS_HELP } from "../graph/GraphCanvas";
import { PIANO_ROLL_KEYS_HELP } from "../pianoroll/editing";
import { SectionTitle, SettingGroup } from "./SettingRow";

const GROUPS: { id: keyof Messages["shortcuts"]["groups"]; keys: () => readonly ShortcutHelp[] }[] = [
  { id: "global", keys: globalKeysHelp },
  { id: "clips", keys: clipKeysHelp },
  { id: "pianoRoll", keys: () => PIANO_ROLL_KEYS_HELP },
  { id: "keyboardPiano", keys: keyboardPianoHelp },
  { id: "graph", keys: () => GRAPH_KEYS_HELP },
  { id: "pattern", keys: () => PATTERN_KEYS_HELP },
];

export function ShortcutsSection() {
  return (
    <>
      <SectionTitle title={t().settings.sections.shortcuts} hint={t().shortcuts.hint} />
      <For each={GROUPS}>
        {(group) => (
          <SettingGroup title={`${t().shortcuts.groups[group.id].title} — ${t().shortcuts.groups[group.id].hint}`}>
            <For each={group.keys()}>
              {(k) => (
                <div class="flex items-center justify-between gap-4 px-4 py-2">
                  <span class="text-sm">{k.label}</span>
                  <kbd class="shrink-0 rounded border border-panel-3 bg-bg px-2 py-0.5 font-mono text-xs">{k.keys}</kbd>
                </div>
              )}
            </For>
          </SettingGroup>
        )}
      </For>
    </>
  );
}
