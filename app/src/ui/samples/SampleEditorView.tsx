// The Samples tab: manually extracting sounds from a file onto new tracks. Assembly only: sound choice, selection,
// cursor and preview; waveform — `sampleWave`, screen parts — `SampleList`, `SampleToolbar`, `WaveformCanvas`, `ExtractPanel`.
import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { MIDDLE_C } from "../../domain/pitch";
import { zoomAt, type Selection } from "../../domain/waveform";
import { t } from "../../i18n";
import { claimSpace } from "../../state/globalKeys";
import { useSession } from "../../state/session";
import { ExtractPanel } from "./ExtractPanel";
import { Overview } from "./Overview";
import { SampleList } from "./SampleList";
import { SampleToolbar } from "./SampleToolbar";
import { createSampleWave } from "./sampleWave";
import { WaveformCanvas } from "./WaveformCanvas";

const START_WIDTH = 800;

export function SampleEditorView() {
  const session = useSession();
  const { preview, sampleEditor } = session.actions;
  const { selection: chosen } = session;

  const sample = createMemo(() => session.project.samples.find((s) => s.id === chosen.sample()));
  const [width, setWidth] = createSignal(START_WIDTH);
  const wave = createSampleWave({ sample: () => sample()?.id, width });
  const [selection, setSelection] = createSignal<Selection | null>(null);
  const [cursor, setCursor] = createSignal<number | null>(null);
  const [loop, setLoop] = createSignal(false);
  const [name, setName] = createSignal("");
  const [root, setRoot] = createSignal(MIDDLE_C);

  const markers = () => sample()?.markers ?? [];
  const trackName = () => name().trim() || sample()?.name || "";

  // The first sample is selected automatically if nothing is selected.
  createEffect(() => {
    if (chosen.sample() === null || !sample()) chosen.selectSample(session.project.samples[0]?.id ?? null);
  });

  // On sample change the selection, cursor and preview belong to the previous file, so reset them.
  createEffect(on(() => sample()?.id, () => {
    setSelection(null);
    setCursor(null);
    preview.stopQuietly();
    setName(sample()?.name ?? "");
  }));
  onCleanup(() => preview.stopQuietly());

  const playing = () => session.previewPlayhead() !== null;
  const play = async () => {
    const id = sample()?.id;
    if (id === undefined) return;
    const sel = selection();
    await preview.play(id, sel?.from ?? cursor() ?? 0, sel?.to ?? wave.total(), loop());
  };
  const togglePlay = () => (playing() ? void preview.stop() : void play());

  // Space here means preview, not transport, wherever the focus is (one of the three key-ownership ways, `state/globalKeys.ts`).
  onMount(() => onCleanup(claimSpace(togglePlay)));

  const saveMarkers = (next: number[]) => {
    const id = sample()?.id;
    if (id !== undefined) void sampleEditor.setMarkers(id, next);
  };

  /** Where to add a marker: the cursor (click on the waveform) or the selection start; `null` — no place chosen. */
  const markerPlace = () => cursor() ?? selection()?.from ?? null;
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
            onFitSelection={selection() ? () => { const s = selection()!; wave.updateView({ start: s.from, span: s.to - s.from }); } : null}
            onAddMarker={markerPlace() !== null ? () => saveMarkers([...markers(), markerPlace()!]) : null}
            markerCount={markers().length} onClearMarkers={() => saveMarkers([])}
            total={wave.total()} rate={wave.rate()}
          />

          <div class="flush flex flex-col divide-y divide-line">
            <WaveformCanvas
              wave={wave.wave()} total={wave.total()} sampleRate={wave.rate()} view={wave.view()} selection={selection()}
              markers={markers()} playhead={session.previewPlayhead()}
              onView={wave.updateView} onSelection={setSelection} onCursor={setCursor} onMarkers={saveMarkers} onWidth={setWidth}
            />
            <Overview wave={wave.overview()} total={wave.total()} view={wave.view()} selection={selection()} onPan={(start) => wave.updateView({ start, span: wave.view().span })} />
            <p class="text-[11px] leading-relaxed text-muted">
              {t().samples.help}
            </p>
          </div>

          <ExtractPanel
            selection={selection()} rate={wave.rate()} name={name()} onName={setName} root={root()} onRoot={setRoot} markerCount={markers().length}
            onExtractSelection={() => {
              const s = sample();
              const sel = selection();
              if (s && sel) void sampleEditor.extractSelection(spec(s.id), sel, trackName());
            }}
            onExtractSlices={() => {
              const s = sample();
              if (s && markers().length > 0) void sampleEditor.extractSlices(spec(s.id), markers(), trackName());
            }}
            onSaveRegion={() => {
              const s = sample();
              if (s) void sampleEditor.saveRegion(s.id, selection() ?? { from: 0, to: wave.total() }, trackName());
            }}
          />
        </section>
      </Show>
    </div>
  );
}
