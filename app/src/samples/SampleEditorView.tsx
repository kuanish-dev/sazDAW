import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { MIDDLE_C } from "../common/pitch";
import { zoomAt, type Selection } from "./waveform";
import { t } from "../i18n";
import { claimSpace } from "../shell/globalKeys";
import { ExtractPanel } from "./ExtractPanel";
import { Overview } from "./Overview";
import { SampleList } from "./SampleList";
import { SampleToolbar } from "./SampleToolbar";
import { createSampleWave } from "./sampleWave";
import { WaveformCanvas } from "./WaveformCanvas";
import { usePlayback, useProjectCopy } from "../core/roles";
import { useSamples } from "./role";
import { useSelection } from "../selection/role";

const START_WIDTH = 800;

export function SampleEditorView() {
  const { previewPlayhead } = usePlayback();
  const { project } = useProjectCopy();
  const selection = useSelection();
  const { preview, editor } = useSamples();

  const sample = createMemo(() => project.samples.find((s) => s.id === selection.sample()));
  const [width, setWidth] = createSignal(START_WIDTH);
  const wave = createSampleWave({ sample: () => sample()?.id, width });
  const [region, setRegion] = createSignal<Selection | null>(null);
  const [cursor, setCursor] = createSignal<number | null>(null);
  const [loop, setLoop] = createSignal(false);
  const [name, setName] = createSignal("");
  const [root, setRoot] = createSignal(MIDDLE_C);

  const markers = () => sample()?.markers ?? [];
  const trackName = () => name().trim() || sample()?.name || "";

  createEffect(() => {
    if (selection.sample() === null || !sample()) selection.selectSample(project.samples[0]?.id ?? null);
  });

  createEffect(on(() => sample()?.id, () => {
    setRegion(null);
    setCursor(null);
    preview.stopQuietly();
    setName(sample()?.name ?? "");
  }));
  onCleanup(() => preview.stopQuietly());

  const playing = () => previewPlayhead() !== null;
  const play = async () => {
    const id = sample()?.id;
    if (id === undefined) return;
    const sel = region();
    await preview.play(id, sel?.from ?? cursor() ?? 0, sel?.to ?? wave.total(), loop());
  };
  const togglePlay = () => (playing() ? void preview.stop() : void play());

  onMount(() => onCleanup(claimSpace(togglePlay)));

  const saveMarkers = (next: number[]) => {
    const id = sample()?.id;
    if (id !== undefined) void editor.setMarkers(id, next);
  };

  /** Where to add a marker: the cursor (click on the waveform) or the selection start; `null` — no place chosen. */
  const markerPlace = () => cursor() ?? region()?.from ?? null;
  const spec = (id: number) => ({ sample: id, totalFrames: wave.total(), root: root() });

  return (
    <div class="flex h-full min-h-[560px]">
      <SampleList />
      <Show
        when={sample() && wave.meta()}
        fallback={<section class="grid min-w-0 flex-1 place-items-center p-6 text-center text-sm text-muted">{t().samples.noSample}</section>}
      >
        <section class="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
          <SampleToolbar
            playing={playing()} onTogglePlay={togglePlay} loop={loop()} onLoop={setLoop}
            onZoom={(factor) => wave.updateView(zoomAt(wave.view(), wave.view().start + wave.view().span / 2, factor, wave.total()))}
            onFitAll={wave.fitAll}
            onFitSelection={region() ? () => { const s = region()!; wave.updateView({ start: s.from, span: s.to - s.from }); } : null}
            onAddMarker={markerPlace() !== null ? () => saveMarkers([...markers(), markerPlace()!]) : null}
            markerCount={markers().length} onClearMarkers={() => saveMarkers([])}
            total={wave.total()} rate={wave.rate()}
          />

          <div class="flush flex flex-col divide-y divide-line">
            <WaveformCanvas
              wave={wave.wave()} total={wave.total()} sampleRate={wave.rate()} view={wave.view()} selection={region()}
              markers={markers()} playhead={previewPlayhead()}
              onView={wave.updateView} onSelection={setRegion} onCursor={setCursor} onMarkers={saveMarkers} onWidth={setWidth}
            />
            <Overview wave={wave.overview()} total={wave.total()} view={wave.view()} selection={region()} onPan={(start) => wave.updateView({ start, span: wave.view().span })} />
            <p class="text-[11px] leading-relaxed text-muted">
              {t().samples.help}
            </p>
          </div>

          <ExtractPanel
            selection={region()} rate={wave.rate()} name={name()} onName={setName} root={root()} onRoot={setRoot} markerCount={markers().length}
            onExtractSelection={() => {
              const s = sample();
              const sel = region();
              if (s && sel) void editor.extractSelection(spec(s.id), sel, trackName());
            }}
            onExtractSlices={() => {
              const s = sample();
              if (s && markers().length > 0) void editor.extractSlices(spec(s.id), markers(), trackName());
            }}
            onSaveRegion={() => {
              const s = sample();
              if (s) void editor.saveRegion(s.id, region() ?? { from: 0, to: wave.total() }, trackName());
            }}
          />
        </section>
      </Show>
    </div>
  );
}
