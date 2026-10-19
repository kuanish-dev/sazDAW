// Settings → MIDI: keyboards (input devices), connecting to a track, switching the track and disconnecting; the list of
// controller knobs mapped to parameters (MIDI learn; mapped by right-clicking a knob).
import { For, Show, createSignal, onMount } from "solid-js";
import type { Id } from "../api/types";
import { targetLabel } from "../automation/automationRules";
import { instrumentTracks } from "../tracks/trackRules";
import { t } from "../i18n";
import { SectionTitle, SettingGroup, SettingRow } from "./SettingRow";
import { IconRefresh } from "../common/icons";
import { useProjectCopy } from "../core/roles";
import { useMidi } from "../midi/role";
import { useSelection } from "../selection/role";

export function MidiSection() {
  const { catalog, project } = useProjectCopy();
  const midi = useMidi();
  const selection = useSelection();
  
  const msg = () => t().settings.midi;
  const [devices, setDevices] = createSignal<string[]>([]);
  const [device, setDevice] = createSignal("");
  /** Keyboard notes are played by an instrument: audio tracks are not offered. */
  const tracks = () => instrumentTracks(project);
  const [track, setTrack] = createSignal<Id | null>(selection.track());
  /** The selected track if it has an instrument, otherwise the first instrument track. */
  const target = () => {
    const chosen = track();
    return chosen !== null && tracks().some((t) => t.id === chosen) ? chosen : (tracks()[0]?.id ?? null);
  };
  const trackName = (id: Id) => project.tracks.find((t) => t.id === id)?.name ?? msg().trackFallback(id);

  const refresh = async () => {
    const found = await midi.input.devices();
    setDevices(found);
    if (!found.includes(device())) setDevice(midi.input.connection()?.device ?? found[0] ?? "");
  };
  onMount(refresh);
  const label = (target: Parameters<typeof targetLabel>[0]) => targetLabel(target, project, catalog());

  const chooseTrack = (id: Id) => {
    setTrack(id);
    // An already connected keyboard plays the chosen track right away.
    if (midi.input.connection()) void midi.input.setTarget(id);
  };

  return (
    <>
      <SectionTitle title={t().settings.sections.midi} hint={msg().hint} />
      <SettingGroup title={msg().state}>
        <SettingRow label={midi.input.connection() ? msg().connected : msg().notConnected}>
          <Show when={midi.input.connection()} fallback={<span class="text-sm text-muted">{msg().chooseBelow}</span>}>
            {(c) => (
              <>
                <span class="h-2 w-2 rounded-full bg-ok" aria-hidden="true" />
                <span class="text-sm">{msg().connection(c().device, trackName(c().track))}</span>
                <button class="btn" onClick={() => void midi.input.disconnect()}>{msg().disconnect}</button>
              </>
            )}
          </Show>
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().connecting}>
        <SettingRow label={msg().inputDevice} hint={devices().length === 0 ? msg().noDevices : undefined}>
          <select class="field w-60" aria-label={msg().inputDevice} value={device()} disabled={devices().length === 0} onChange={(e) => setDevice(e.currentTarget.value)}>
            <For each={devices()}>{(d) => <option value={d}>{d}</option>}</For>
          </select>
          <button class="btn btn-icon" title={msg().refreshDevices} aria-label={msg().refreshDevices} onClick={() => void refresh()}><IconRefresh /></button>
        </SettingRow>
        <SettingRow label={msg().track} hint={tracks().length === 0 ? msg().noTracks : msg().trackHint}>
          <select
            class="field w-60" aria-label={msg().track} value={target() ?? ""} disabled={tracks().length === 0}
            onChange={(e) => chooseTrack(Number(e.currentTarget.value))}
          >
            <For each={tracks()}>{(t) => <option value={t.id}>{t.name}</option>}</For>
          </select>
        </SettingRow>
        <SettingRow label="">
          <button class="btn btn-on" disabled={!device() || target() === null} onClick={() => void midi.input.connect(device(), target()!)}>
            {midi.input.connection() ? msg().reconnect : msg().connect}
          </button>
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().learn}>
        <Show when={midi.learn.learning()}>
          {(waiting) => (
            <SettingRow label={msg().waiting(label(waiting()))} hint={msg().waitingHint}>
              <button class="btn" onClick={() => void midi.learn.cancelLearn()}>{msg().stopWaiting}</button>
            </SettingRow>
          )}
        </Show>
        <For
          each={midi.learn.mappings()}
          fallback={
            <SettingRow label={msg().noMappings} hint={msg().noMappingsHint}>
              <span />
            </SettingRow>
          }
        >
          {(m) => (
            <SettingRow label={label(m.target)} hint={msg().mapping(m.cc, m.channel === undefined ? undefined : m.channel + 1)}>
              <button class="btn" onClick={() => void midi.learn.removeMapping(m.target)}>{t().common.remove}</button>
            </SettingRow>
          )}
        </For>
      </SettingGroup>
    </>
  );
}
