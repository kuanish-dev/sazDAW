// Settings → Project: name and path of the open project, tempo, time signature, metronome, note recording snap and a summary.
// Tempo and signature here are the song start (points at tick 0; changes along the song live on the timeline tempo lane);
// metronome and note recording snap are the same actions as in the top bar.
import { For, Show, createMemo } from "solid-js";
import { projectStats } from "./projectStats";
import { MAX_NUMERATOR, MAX_TEMPO, MIN_TEMPO, RECORD_SNAPS, SIGNATURE_DENOMINATORS, formatDuration } from "../common/time";
import { t } from "../i18n";
import { NumberField } from "../common/NumberField";
import { CopyButton, SectionTitle, SettingGroup, SettingRow, Toggle } from "./SettingRow";
import { usePlayback, useProjectCopy } from "../core/roles";
import { useRecording } from "../recording/role";
import { useTransport } from "../transport/role";

export function ProjectSection() {
  const { tick } = usePlayback();
  const { hasProject, project, status } = useProjectCopy();
  const recording = useRecording();
  const transport = useTransport();
const { tempo } = transport;
const { noteRecord } = recording;
  const stats = createMemo(() => projectStats(project));
  const msg = () => t().settings.project;
  const baseTempo = () => project.tempoMap.tempos[0]?.bpm ?? MIN_TEMPO;
  const baseSignature = () => project.tempoMap.signatures[0]?.signature ?? { numerator: 4, denominator: 4 };

  const facts = () => {
    const labels = msg().facts;
    return [
      { label: labels.tracks, value: stats().tracks },
      { label: labels.clips, value: stats().clips },
      { label: labels.notes, value: stats().notes },
      { label: labels.samples, value: stats().samples },
      { label: labels.graphNodes, value: stats().graphNodes },
      { label: labels.length, value: msg().length(stats().bars, formatDuration(stats().seconds)) },
    ];
  };

  return (
    <>
      <SectionTitle title={t().settings.sections.project} hint={msg().hint} />
      <Show when={hasProject()} fallback={<p class="panel p-4 text-sm text-muted">{msg().noProject}</p>}>
        <SettingGroup title={msg().file}>
          <SettingRow label={msg().name}>
            <span class="text-sm">{status().name}</span>
            <Show when={status().dirty}><span class="text-xs text-accent">{msg().unsaved}</span></Show>
          </SettingRow>
          <SettingRow label={msg().path}>
            <span class="max-w-80 truncate font-mono text-xs text-muted" title={status().path ?? undefined}>{status().path}</span>
            <CopyButton text={() => status().path ?? ""} />
          </SettingRow>
        </SettingGroup>
        <SettingGroup title={msg().time}>
          <SettingRow label={msg().startTempo} hint={msg().startTempoHint}>
            <NumberField value={baseTempo()} min={MIN_TEMPO} max={MAX_TEMPO} step={1} decimals={1} class="w-24" label={msg().tempo} onCommit={(v) => void tempo.setTempoAt(0, v)} />
          </SettingRow>
          <SettingRow label={msg().startSignature}>
            <NumberField value={baseSignature().numerator} min={1} max={MAX_NUMERATOR} class="w-14" label={msg().numerator} onCommit={(v) => void tempo.setSignatureAt(0, { numerator: v, denominator: baseSignature().denominator })} />
            <span class="text-muted">/</span>
            <select class="field w-16" aria-label={msg().denominator} value={baseSignature().denominator} onChange={(e) => void tempo.setSignatureAt(0, { numerator: baseSignature().numerator, denominator: Number(e.currentTarget.value) })}>
              <For each={SIGNATURE_DENOMINATORS}>{(d) => <option value={d}>{d}</option>}</For>
            </select>
          </SettingRow>
          <SettingRow label={msg().metronome}>
            <Toggle label={msg().metronome} checked={tick().metronome} onChange={(on) => void transport.setMetronome(on)} />
          </SettingRow>
          <SettingRow label={msg().recordSnap} hint={msg().recordSnapHint}>
            <select class="field w-36" aria-label={msg().recordSnapLabel} value={tick().record.quantizeTicks} onChange={(e) => void noteRecord.setQuantize(Number(e.currentTarget.value))}>
              <For each={RECORD_SNAPS}>{(snap) => <option value={snap.ticks}>{snap.label}</option>}</For>
            </select>
          </SettingRow>
        </SettingGroup>
        <SettingGroup title={msg().summary}>
          <div class="grid grid-cols-2 gap-px bg-line sm:grid-cols-3">
            <For each={facts()}>
              {(fact) => (
                <div class="bg-panel px-4 py-3">
                  <div class="label">{fact.label}</div>
                  <div class="mt-0.5 font-mono text-lg">{fact.value}</div>
                </div>
              )}
            </For>
          </div>
        </SettingGroup>
      </Show>
    </>
  );
}
