// The built-in synth's picture: one cycle of the chosen waveform and the lowpass response with the cutoff marker.
// Read-only (the knobs below change the values); drawn in the same style as the envelope graph.
import { For, type JSX } from "solid-js";
import { cutoffX, filterPath, wavePath } from "../../domain/synth";
import { t } from "../../i18n";

const WIDTH = 146;
const HEIGHT = 64;
const PAD = 6;

function Panel(props: { label: string; children: JSX.Element }) {
  return (
    <div class="rounded border border-line bg-bg p-2">
      <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} class="block h-auto w-full" role="img" aria-label={props.label}>
        <For each={[0.25, 0.5, 0.75]}>
          {(f) => <line x1="0" x2={WIDTH} y1={HEIGHT * f} y2={HEIGHT * f} stroke="var(--canvas-grid)" stroke-width="1" />}
        </For>
        {props.children}
      </svg>
    </div>
  );
}

/** `waveform` is the option index, the cuts are in hertz (the synth's own parameter values). */
export function SynthGraph(props: { waveform: number; lowCut: number; highCut: number }) {
  const marker = (hz: number) => (
    <line x1={cutoffX(hz, WIDTH, PAD)} x2={cutoffX(hz, WIDTH, PAD)} y1="0" y2={HEIGHT} stroke="var(--color-accent)" stroke-opacity="0.4" stroke-width="1" />
  );
  return (
    <div class="grid grid-cols-2 gap-2">
      <Panel label={t().tracks.synthGraph.wave}>
        <path d={wavePath(props.waveform, WIDTH, HEIGHT, PAD)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
      </Panel>
      <Panel label={t().tracks.synthGraph.filter}>
        {marker(props.lowCut)}
        {marker(props.highCut)}
        <path d={filterPath(props.lowCut, props.highCut, WIDTH, HEIGHT, PAD)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
      </Panel>
    </div>
  );
}
