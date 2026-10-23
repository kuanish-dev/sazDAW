import { For, Show, createSignal, onMount } from "solid-js";
import { createStore } from "solid-js/store";
import type { AudioSettingsForm } from "./audio";
import type { AudioDevice, AudioDevices, AudioStatus, InputLevel } from "../api/types";
import { MAX_RECORD_LATENCY_MS, bufferChoices, bufferLatencyMs, parseBufferFrames } from "./audioSettings";
import { plainLevel } from "../mixer/mixer";
import { t } from "../i18n";
import { errorText } from "../i18n/errors";
import { createWatchedPoll } from "../core/poller";
import { Meter } from "../common/Meter";
import { SectionTitle, SettingGroup, SettingRow } from "./SettingRow";
import { useSettings } from "./role";

/** Poll the input level ~15 times a second: the meter looks alive. */
const LEVEL_POLL_MS = 66;
const LEVEL_METER_HEIGHT = 32;

/** Device select; the empty value is "Default (<name>)". */
function DeviceSelect(props: { label: string; devices: AudioDevice[]; value: string; onChange: (value: string) => void }) {
  const defaultName = () => props.devices.find((d) => d.isDefault)?.name;
  return (
    <select class="field w-72" aria-label={props.label} value={props.value} onChange={(e) => props.onChange(e.currentTarget.value)}>
      <option value="">{defaultName() ? t().settings.audio.defaultDevice(defaultName()!) : t().settings.audio.default}</option>
      <For each={props.devices}>{(d) => <option value={d.name}>{d.name}</option>}</For>
    </select>
  );
}

export function AudioSection() {
  const { audio } = useSettings();
  const [devices, setDevices] = createSignal<AudioDevices>({ outputs: [], inputs: [] });
  const [status, setStatus] = createSignal<AudioStatus | null>(null);
  const [form, setForm] = createStore<AudioSettingsForm>({ output: "", input: "", buffer: "", recordLatency: "" });
  const [applying, setApplying] = createSignal(false);
  const [level, setLevel] = createSignal<InputLevel | null>(null);
  const msg = () => t().settings.audio;

  onMount(async () => {
    const [found, settings, current] = await Promise.all([audio.devices(), audio.settings(), audio.status()]);
    if (found) setDevices(found);
    if (settings) {
      setForm({
        output: settings.output ?? "", input: settings.input ?? "",
        buffer: settings.bufferFrames === null ? "" : String(settings.bufferFrames),
        recordLatency: settings.recordLatencyMs === null ? "" : String(settings.recordLatencyMs),
      });
    }
    if (current) setStatus(current);
  });

  const levelCheck = createWatchedPoll({
    watch: audio.watchInput, read: audio.inputLevel, show: setLevel, onStop: () => setLevel(null),
  }, LEVEL_POLL_MS);
  const toggleWatching = () => (levelCheck.running() ? levelCheck.stop() : levelCheck.start());

  const apply = async () => {
    setApplying(true);
    try {
      const next = await audio.apply({ ...form });
      if (next) setStatus(next);
    } finally {
      setApplying(false);
    }
  };

  const choices = () => bufferChoices(status()?.bufferRange ?? null);
  const latency = () => bufferLatencyMs(parseBufferFrames(form.buffer), status()?.sampleRate ?? 0);

  return (
    <>
      <SectionTitle title={t().settings.sections.audio} hint={msg().hint} />
      <SettingGroup title={msg().devices}>
        <SettingRow label={msg().output}><DeviceSelect label={msg().output} devices={devices().outputs} value={form.output} onChange={(v) => setForm("output", v)} /></SettingRow>
        <SettingRow label={msg().input}><DeviceSelect label={msg().input} devices={devices().inputs} value={form.input} onChange={(v) => setForm("input", v)} /></SettingRow>
        <SettingRow label={msg().bufferSize} hint={latency() !== null ? msg().bufferLatency(latency()!.toFixed(1)) : msg().bufferHint}>
          <select class="field w-40" aria-label={msg().bufferSize} value={form.buffer} onChange={(e) => setForm("buffer", e.currentTarget.value)}>
            <option value="">{msg().default}</option>
            <For each={choices()}>{(n) => <option value={String(n)}>{n}</option>}</For>
          </select>
        </SettingRow>
        <SettingRow label={msg().recordLatency} hint={msg().recordLatencyHint}>
          <input
            class="field w-40" type="number" min={0} max={MAX_RECORD_LATENCY_MS} step={1} placeholder={msg().auto} aria-label={msg().recordLatencyLabel}
            value={form.recordLatency} onInput={(e) => setForm("recordLatency", e.currentTarget.value)}
          />
        </SettingRow>
      </SettingGroup>
      <div class="mb-5 flex items-center justify-between gap-3">
        <Show when={status()} fallback={<span />}>
          {(s) => (
            <div class="text-xs text-muted">
              <p>{msg().status(s().sampleRate, s().channels)}</p>
              <Show when={s().error} fallback={<p>{s().running ? msg().running(s().outputName ?? msg().defaultDeviceName) : msg().stopped}</p>}>
                <p class="text-danger">{errorText(s().error)}</p>
              </Show>
            </div>
          )}
        </Show>
        <button class="btn btn-on" disabled={applying()} onClick={() => void apply()}>{applying() ? msg().applying : t().common.apply}</button>
      </div>
      <SettingGroup title={msg().inputCheck}>
        <SettingRow label={msg().inputLevel} hint={msg().inputLevelHint}>
          <Show when={level()}>{(l) => <Meter level={plainLevel(l().left, l().right)} height={LEVEL_METER_HEIGHT} />}</Show>
          <button class="btn" onClick={toggleWatching}>{level() ? msg().stopCheck : msg().checkInput}</button>
        </SettingRow>
      </SettingGroup>
    </>
  );
}
