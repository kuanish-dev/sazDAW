// Live spectrum of the sound coming out of a node, drawn inside the EQ and filter graphs behind their curve and handles
// (FabFilter Pro-Q style). The sound comes from `listenToNode`; when playback stops the node goes silent and the levels
// fall, so the picture fades out by itself. The math is `effects/spectrum.ts`.
import { Show, createSignal } from "solid-js";
import type { Id } from "../api/types";
import type { PlotView } from "../common/plot";
import { smoothLevels, spectrumLevels, spectrumPath } from "./spectrum";
import { listenToNode } from "./nodeSound";

export function LiveSpectrum(props: { node: Id; view: PlotView }) {
  const [path, setPath] = createSignal("");
  let shown: number[] = [];

  listenToNode(
    () => props.node,
    (sound) => {
      shown = smoothLevels(shown, spectrumLevels(sound.output, sound.sampleRate), sound.dt);
      setPath(spectrumPath(shown, props.view));
    },
    () => {
      shown = [];
      setPath("");
    },
  );

  return (
    <Show when={path()}>
      {(d) => (
        <path d={d()} fill="var(--color-muted)" fill-opacity="0.22" stroke="var(--color-muted)" stroke-opacity="0.55" stroke-width="1" stroke-linejoin="round" pointer-events="none" />
      )}
    </Show>
  );
}
