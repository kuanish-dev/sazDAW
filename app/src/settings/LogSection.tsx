// Settings → Log & about: version, error log file path, recent entries and Copy all.
import { For, Show, createSignal, onMount } from "solid-js";
import type { LogEntry } from "../api/types";
import { language, t } from "../i18n";
import { useSession } from "../session";
import { CopyButton, SectionTitle, SettingGroup, SettingRow } from "./SettingRow";

const LIMIT = 200;

const stamp = (ms: number) => new Date(ms).toLocaleString(language());

export function LogSection() {
  const session = useSession();
  const [entries, setEntries] = createSignal<LogEntry[]>([]);
  const [file, setFile] = createSignal<string | null>(null);
  const [version, setVersion] = createSignal("…");
  const reload = async () => setEntries(await session.settings.log.recent(LIMIT));

  onMount(async () => {
    void session.settings.about.version().then(setVersion);
    setFile(await session.settings.log.file());
    await reload();
  });

  const asText = () =>
    entries()
      .map((e) => `${stamp(e.timeMs)} [${e.level}] ${e.source}: ${e.message}${e.details ? `\n${e.details}` : ""}`)
      .join("\n\n");

  return (
    <>
      <SectionTitle title={t().settings.sections.log} />
      <SettingGroup title={t().settings.log.about}>
        <SettingRow label={t().settings.log.version}><span class="font-mono text-sm">{version()}</span></SettingRow>
        <SettingRow label={t().settings.log.file} hint={t().settings.log.fileHint}>
          <span class="max-w-96 break-all font-mono text-xs text-muted">{file() ?? t().settings.log.fileUnknown}</span>
        </SettingRow>
      </SettingGroup>
      <div class="mb-1.5 flex items-center justify-between px-1">
        <h3 class="label">{t().settings.log.recent}</h3>
        <div class="flex gap-2">
          <button class="btn !h-7 text-xs" onClick={() => void reload()}>{t().settings.log.refresh}</button>
          <CopyButton text={asText} label={t().settings.log.copyAll} disabled={entries().length === 0} />
        </div>
      </div>
      <div class="panel max-h-[55vh] overflow-auto">
        <For each={[...entries()].reverse()} fallback={<p class="p-4 text-sm text-muted">{t().settings.log.empty}</p>}>
          {(e) => (
            <details class="border-b border-line px-3 py-1.5 text-xs last:border-b-0">
              <summary class="cursor-pointer">
                <span class="text-muted">{stamp(e.timeMs)}</span>{" "}
                <span classList={{ "text-danger": e.level === "error", "text-accent": e.level === "warn" }}>{e.level}</span>{" "}
                <span class="text-muted">{e.source}</span> · {e.message}
              </summary>
              <Show when={e.details}><pre class="mt-1 whitespace-pre-wrap break-all font-mono text-[11px] text-muted">{e.details}</pre></Show>
            </details>
          )}
        </For>
      </div>
    </>
  );
}
