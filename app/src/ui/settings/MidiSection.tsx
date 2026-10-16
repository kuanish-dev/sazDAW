// Settings → MIDI: keyboards (input devices), connecting to a track, switching the track and disconnecting; the list of
// controller knobs mapped to parameters (MIDI learn; mapped by right-clicking a knob).
import { For, Show, createSignal, onMount } from "solid-js";
import type { Id } from "../../api/types";
import { targetLabel } from "../../domain/automation";
import { instrumentTracks } from "../../domain/tracks";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { SectionTitle, SettingGroup, SettingRow } from "./SettingRow";
import { IconRefresh } from "../common/icons";

export function MidiSection() {
  const session = useSession();
  const { midiInput: midi, midiLearn } = session.actions;
  const msg = () => t().settings.midi;
  const [devices, setDevices] = createSignal<string[]>([]);
  const [device, setDevice] = createSignal("");
  /** Keyboard notes are played by an instrument: audio tracks are not offered. */
  const tracks = () => instrumentTracks(session.project);
  const [track, setTrack] = createSignal<Id | null>(session.selection.track());
  /** The selected track if it has an instrument, otherwise the first instrument track. */
  const target = () => {
    const chosen = track();
    return chosen !== null && tracks().some((t) => t.id === chosen) ? chosen : (tracks()[0]?.id ?? null);
  };
  const trackName = (id: Id) => session.project.tracks.find((t) => t.id === id)?.name ?? msg().trackFallback(id);

  const refresh = async () => {
    const found = await midi.devices();
    setDevices(found);
    if (!found.includes(device())) setDevice(midi.connection()?.device ?? found[0] ?? "");
  };
  onMount(refresh);
  const label = (target: Parameters<typeof targetLabel>[0]) => targetLabel(target, session.project, session.catalog());

  const chooseTrack = (id: Id) => {
    setTrack(id);
    // An already connected keyboard plays the chosen track right away.
    if (midi.connection()) void midi.setTarget(id);
  };

  return (
    <>
      <SectionTitle title={t().settings.sections.midi} hint={msg().hint} />
      <SettingGroup title={msg().state}>
        <SettingRow label={midi.connection() ? msg().connected : msg().notConnected}>
          <Show when={midi.connection()} fallback={<span class="text-sm text-muted">{msg().chooseBelow}</span>}>
            {(c) => (
              <>
                <span class="h-2 w-2 rounded-full bg-ok" aria-hidden="true" />
                <span class="text-sm">{msg().connection(c().device, trackName(c().track))}</span>
                <button class="btn" onClick={() => void midi.disconnect()}>{msg().disconnect}</button>
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
          <button class="btn btn-on" disabled={!device() || target() === null} onClick={() => void midi.connect(device(), target()!)}>
            {midi.connection() ? msg().reconnect : msg().connect}
          </button>
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().learn}>
        <Show when={midiLearn.learning()}>
          {(waiting) => (
            <SettingRow label={msg().waiting(label(waiting()))} hint={msg().waitingHint}>
              <button class="btn" onClick={() => void midiLearn.cancelLearn()}>{msg().stopWaiting}</button>
            </SettingRow>
          )}
        </Show>
        <For
          each={midiLearn.mappings()}
          fallback={
            <SettingRow label={msg().noMappings} hint={msg().noMappingsHint}>
              <span />
            </SettingRow>
          }
        >
          {(m) => (
            <SettingRow label={label(m.target)} hint={msg().mapping(m.cc, m.channel === undefined ? undefined : m.channel + 1)}>
              <button class="btn" onClick={() => void midiLearn.removeMapping(m.target)}>{t().common.remove}</button>
            </SettingRow>
          )}
        </For>
      </SettingGroup>
    </>
  );
}
