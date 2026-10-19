// "What plays each drum": the built-in sound, your track, your sample or "don't play". The choice lives in the session.
import { For, Show } from "solid-js";
import { AUTO_CHOICE, SKIP_CHOICE, effectiveChoice, laneLabel, sampleChoice, trackChoice } from "./drumPart";
import { instrumentTracks } from "../tracks/trackRules";
import { t } from "../i18n";
import { useSession } from "../session";

export function DrumKitPicker(props: { lanes: string[] }) {
  const session = useSession();
  const { project, drumKit } = session;
  const targets = () => instrumentTracks(project);
  const choiceOf = (lane: string) => effectiveChoice(drumKit.choices(), lane, project);
  const msg = () => t().drums.kit;

  return (
    <>
      <div class="label mt-1">{msg().title}</div>
      <div class="flex max-h-[24vh] flex-col gap-1 overflow-y-auto">
        <For each={props.lanes}>
          {(lane) => (
            <label class="flex items-center gap-2 text-xs">
              <span class="w-20 shrink-0">{laneLabel(lane)}</span>
              <select class="field !h-6 min-w-0 flex-1 text-xs" aria-label={msg().sound(laneLabel(lane))} onChange={(e) => drumKit.pick(lane, e.currentTarget.value)}>
                <option value={AUTO_CHOICE} selected={choiceOf(lane) === AUTO_CHOICE}>{msg().builtIn}</option>
                <option value={SKIP_CHOICE} selected={choiceOf(lane) === SKIP_CHOICE}>{msg().skip}</option>
                <Show when={targets().length > 0}>
                  <optgroup label={msg().yourTracks}>
                    <For each={targets()}>{(track) => <option value={trackChoice(track.id)} selected={choiceOf(lane) === trackChoice(track.id)}>{track.name}</option>}</For>
                  </optgroup>
                </Show>
                <Show when={project.samples.length > 0}>
                  <optgroup label={msg().yourSamples}>
                    <For each={project.samples}>{(s) => <option value={sampleChoice(s.id)} selected={choiceOf(lane) === sampleChoice(s.id)}>{s.name}</option>}</For>
                  </optgroup>
                </Show>
              </select>
            </label>
          )}
        </For>
      </div>
      <div class="text-[11px] text-muted">{msg().hint}</div>
    </>
  );
}
