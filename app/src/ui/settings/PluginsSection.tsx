// Settings → Plugins: VST3 search folders, the installed table (filter by text and type, disabling a plugin), scanning and its result.
import { For, Show, createSignal, onMount } from "solid-js";
import type { PluginList, PluginType } from "../../api/types";
import { filterPlugins } from "../../domain/plugins";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { SectionTitle, SettingGroup, Toggle } from "./SettingRow";

const TYPE_FILTERS: readonly (PluginType | "all")[] = ["all", "effect", "instrument"];

function ScanReportView() {
  const { plugins } = useSession().actions;
  const msg = () => t().settings.plugins;
  return (
    <Show when={plugins.lastScan()}>
      {(report) => (
        <SettingGroup title={msg().lastScan}>
          <div class="px-4 py-3 text-sm">
            {msg().found} <b>{report().found}</b> · {msg().effects} <b>{report().effects}</b> · {msg().instruments} <b>{report().instruments}</b>
            <Show when={report().failed.length > 0} fallback={<span class="text-muted"> · {msg().noFailures}</span>}>
              <span class="text-danger"> · {msg().failedCount(report().failed.length)}</span>
            </Show>
          </div>
          <Show when={report().failed.length > 0}>
            <p class="px-4 pb-1 text-xs text-muted">
              {msg().failedHint}
            </p>
          </Show>
          <For each={report().failed}>
            {(f) => (
              <div class="flex items-start gap-2 px-4 py-2 text-xs">
                <span class="mt-0.5 shrink-0 rounded bg-danger/15 px-1.5 py-0.5 text-[10px] font-semibold text-danger">{msg().failure}</span>
                <div class="min-w-0">
                  <div class="break-all font-mono">{f.path}</div>
                  <div class="text-danger">{f.reason}</div>
                </div>
              </div>
            )}
          </For>
        </SettingGroup>
      )}
    </Show>
  );
}

export function PluginsSection() {
  const { plugins } = useSession().actions;
  const [list, setList] = createSignal<PluginList>({ folders: [], plugins: [] });
  const [text, setText] = createSignal("");
  const [type, setType] = createSignal<PluginType | "all">("all");
  const msg = () => t().settings.plugins;

  const reload = async () => {
    const next = await plugins.list();
    if (next) setList(next);
  };
  onMount(reload);

  const scan = async () => {
    await plugins.scan();
    await reload();
  };

  const shown = () => filterPlugins(list().plugins, text(), type());

  const setDisabled = async (kind: string, disabled: boolean) => {
    await plugins.setDisabled(kind, disabled);
    await reload();
  };

  return (
    <>
      <SectionTitle title={t().settings.sections.plugins} hint={msg().hint} />
      <div class="mb-5 flex items-center gap-3">
        <button class="btn btn-on" disabled={plugins.scanning()} onClick={() => void scan()}>{plugins.scanning() ? msg().scanning : msg().scan}</button>
        <span class="text-xs text-muted">{msg().scanHint}</span>
      </div>
      <ScanReportView />
      <SettingGroup title={msg().folders}>
        <For each={list().folders} fallback={<p class="px-4 py-3 text-sm text-muted">{msg().foldersUnknown}</p>}>
          {(folder) => <div class="break-all px-4 py-2 font-mono text-xs">{folder}</div>}
        </For>
      </SettingGroup>
      <div class="mb-2 flex flex-wrap items-center gap-2">
        <input class="field w-64" placeholder={msg().search} aria-label={msg().searchLabel} value={text()} onInput={(e) => setText(e.currentTarget.value)} />
        <div class="flex gap-1">
          <For each={TYPE_FILTERS}>
            {(f) => <button class="btn text-xs" classList={{ "btn-on": type() === f }} onClick={() => setType(f)}>{msg().filters[f]}</button>}
          </For>
        </div>
        <span class="ml-auto text-xs text-muted">{msg().ofTotal(shown().length, list().plugins.length)}</span>
      </div>
      <div class="panel overflow-x-auto">
        <table class="w-full text-sm">
          <thead class="border-b border-line text-left">
            <tr>
              <th class="label px-4 py-2">{msg().plugin}</th>
              <th class="label px-3 py-2">{msg().vendor}</th>
              <th class="label px-3 py-2">{msg().type}</th>
              <th class="label px-3 py-2 text-right" title={msg().paramsHint}>{msg().params}</th>
              <th class="label px-3 py-2" title={msg().disabledHint}>{msg().disabled}</th>
            </tr>
          </thead>
          <tbody>
            <For each={shown()} fallback={<tr><td colspan={5} class="px-4 py-4 text-muted">{msg().nothingFound}</td></tr>}>
              {(p) => (
                <tr class="border-t border-line align-top hover:bg-hover/60" classList={{ "opacity-60": p.disabled }} title={p.path}>
                  <td class="max-w-0 px-4 py-2">
                    <div class="flex items-center gap-2">
                      <span>{p.name}</span>
                      <Show when={p.disabled}><span class="rounded bg-danger/15 px-1.5 py-0.5 text-[10px] text-danger">{msg().disabledBadge}</span></Show>
                    </div>
                    <div class="truncate font-mono text-[11px] text-muted">{p.path}</div>
                  </td>
                  <td class="px-3 py-2 text-muted">{p.vendor || "—"}</td>
                  <td class="px-3 py-2">{msg().types[p.type]}</td>
                  <td class="whitespace-nowrap px-3 py-2 text-right font-mono" classList={{ "text-muted": p.params === p.paramsTotal }}>{msg().ofTotal(p.params, p.paramsTotal)}</td>
                  <td class="px-3 py-2">
                    <Toggle checked={p.disabled} label={msg().disable(p.name)} onChange={(checked) => void setDisabled(p.kind, checked)} />
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </>
  );
}
