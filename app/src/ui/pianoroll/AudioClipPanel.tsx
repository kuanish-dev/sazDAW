// Panel of the selected audio clip: "Warp to tempo" (the sound's source tempo), pitch in semitones without changing length,
// and takes: recording over a clip keeps earlier recordings as takes (position, length, warp and fades are shared); which
// take plays on each part of the clip (the comp) is chosen in the take lanes `TakeLanes`.
// Clip volume and fades are on the panel above the timeline. All edits go through `session.actions.clips`.
import { Show } from "solid-js";
import type { Clip, Track } from "../../api/types";
import { clipLabel } from "../../domain/clips";
import { barsBetween, tempoAt } from "../../domain/tempoMap";
import { MAX_BPM, MAX_PITCH_SEMITONES, MIN_BPM, formatBpm } from "../../domain/warp";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { NumberField } from "../common/NumberField";
import { TakeLanes } from "./TakeLanes";

/** `snap` is the timeline snap grid (ticks): take parts are chosen on it. */
export function AudioClipPanel(props: { clip: Clip; track: Track | undefined; snap: number }) {
  const session = useSession();
  const { clips } = session.actions;
  const warp = () => props.clip.warp;
  /** Tempo at the clip start: if the tempo changes inside the clip, the sound stretches along the clip (as in the backend). */
  const projectTempo = () => tempoAt(session.project.tempoMap, props.clip.startTick);
  return (
    <div class="flex min-h-0 flex-1 flex-col">
      <div class="flex min-h-9 shrink-0 flex-wrap items-center gap-3 border-b border-line px-3 py-1">
        <h2 class="label">{props.track?.name ?? t().pianoRoll.trackFallback}</h2>
        <span class="text-xs text-muted">{clipLabel(props.clip, session.project)} · {t().pianoRoll.clipBars(Math.round(barsBetween(session.project.tempoMap, props.clip.startTick, props.clip.startTick + props.clip.lengthTicks) * 100) / 100)}</span>
      </div>
      <div class="flex flex-col gap-4 p-4 text-sm">
        <label class="flex items-center gap-2" title={t().pianoRoll.warpHint}>
          <input type="checkbox" checked={warp() !== null} onChange={(e) => void clips.setWarp(props.clip.id, e.currentTarget.checked)} />
          {t().pianoRoll.warp}
        </label>
        <Show when={warp()}>
          {(w) => (
            <div class="flex items-center gap-2 pl-6">
              <span>{t().pianoRoll.sourceTempo}</span>
              <NumberField
                value={w().sourceBpm} min={MIN_BPM} max={MAX_BPM} step={1} decimals={2} label={t().pianoRoll.sourceTempoField}
                onCommit={(bpm) => void clips.setSourceBpm(props.clip.id, bpm)}
              />
              <span class="text-xs text-muted">{t().pianoRoll.warpInfo(formatBpm(projectTempo()), formatBpm(projectTempo() / w().sourceBpm))}</span>
            </div>
          )}
        </Show>
        <div class="flex items-center gap-2">
          <span>{t().pianoRoll.pitch}</span>
          <NumberField
            value={props.clip.pitchSemitones} min={-MAX_PITCH_SEMITONES} max={MAX_PITCH_SEMITONES} step={1} decimals={2} label={t().pianoRoll.pitch}
            onCommit={(semitones) => void clips.setPitch(props.clip.id, semitones)}
          />
          <span class="text-xs text-muted">{t().pianoRoll.pitchHint}</span>
        </div>
        <TakeLanes clip={props.clip} snap={props.snap} />
        <p class="text-xs text-muted">{t().pianoRoll.crossfadeHint}</p>
      </div>
    </div>
  );
}
