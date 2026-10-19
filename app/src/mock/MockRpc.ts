// In-memory backend for developing the interface in a browser without Rust (implements `RpcClient`). Mirrors the shape
// of `daw-rpc` answers, with simplified checks. State lives in `MockWorld`, methods by area in `handlers/`; here only
// assembly, the call and the `tick` stream.
import type { RpcClient } from "../api/rpc";
import type { Tick } from "../api/types";
import { secondsAt, signatureAt, tempoAt, tickAtSeconds } from "../common/tempoMap";
import { fillDemo } from "./demo";
import { fail } from "./errors";
import { audioHandlers } from "./handlers/audio";
import { audioRecordHandlers } from "./handlers/audioRecord";
import { automationHandlers } from "./handlers/automation";
import { bounceHandlers } from "./handlers/bounce";
import { clipGroupsHandlers } from "./handlers/clipGroups";
import { clipsHandlers } from "./handlers/clips";
import { chordsHandlers } from "./handlers/chords";
import { drumsHandlers } from "./handlers/drums";
import { foldersHandlers } from "./handlers/folders";
import { graphHandlers } from "./handlers/graph";
import { libraryHandlers } from "./handlers/library";
import { logHandlers } from "./handlers/log";
import { markersHandlers } from "./handlers/markers";
import { midiHandlers } from "./handlers/midi";
import { noteEffectsHandlers } from "./handlers/noteEffects";
import { notesHandlers } from "./handlers/notes";
import { pluginsHandlers } from "./handlers/plugins";
import { preferencesHandlers } from "./handlers/preferences";
import { presetsHandlers } from "./handlers/presets";
import { projectHandlers } from "./handlers/project";
import { recordHandlers } from "./handlers/record";
import { stageHandlers } from "./handlers/stage";
import { templatesHandlers } from "./handlers/templates";
import { tempoHandlers } from "./handlers/tempo";
import { tracksHandlers } from "./handlers/tracks";
import { transportHandlers } from "./handlers/transport";
import type { HandlerSet } from "./handlers/types";
import { mockBandGains, mockMeters, mockReductions } from "./meters";
import { wrapRegion } from "./songLoop";
import { capturing } from "./take";
import { MockWorld } from "./world";

/** About 30 ticks a second, like the backend. */
const TICK_MS = 33;
/** Fake engine load: about 8 %, slightly wobbling. */
const CPU_BASE = 0.08;
const CPU_SWING = 0.03;
const CPU_PERIOD_MS = 900;

export class MockRpc implements RpcClient {
  private world: MockWorld;
  private handlers: HandlerSet;
  private tickListeners = new Set<(t: Tick) => void>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private lastTime = 0;

  constructor(options: { withProject?: boolean } = {}) {
    this.world = new MockWorld();
    this.world.needTicks = () => this.ensureTimer();
    const w = this.world;
    this.handlers = {
      ...transportHandlers(w), ...tempoHandlers(w), ...recordHandlers(w), ...projectHandlers(w), ...tracksHandlers(w),
      ...clipsHandlers(w), ...clipGroupsHandlers(w), ...notesHandlers(w), ...graphHandlers(w), ...stageHandlers(w), ...automationHandlers(w),
      ...libraryHandlers(w), ...midiHandlers(w), ...logHandlers(), ...audioHandlers(w), ...audioRecordHandlers(w),
      ...pluginsHandlers(w), ...preferencesHandlers(w),
      ...bounceHandlers(w), ...templatesHandlers(w), ...presetsHandlers(w), ...markersHandlers(w), ...foldersHandlers(w),
      ...noteEffectsHandlers(w),
    };
    // The drum and chord part builders and the demo beat use the other methods, just as the backend uses its contracts.
    Object.assign(this.handlers, drumsHandlers(w, () => this.handlers), chordsHandlers(w, () => this.handlers));
    this.handlers["project.fillDemo"] = () => fillDemo(w, this.handlers);
    if (options.withProject !== false) {
      fillDemo(w, this.handlers);
      w.openStartProject();
    }
  }

  async call<T = void>(method: string, params?: unknown): Promise<T> {
    const handler = this.handlers[method];
    if (!handler) throw fail("RP-001", { method });
    // Always answer asynchronously, like the real RPC.
    await Promise.resolve();
    return handler(params ?? {}) as T;
  }

  /** Like `RpcTable::call_binary`: only the methods whose handlers answer bytes (the live sound windows, `wire.ts`). */
  async callBinary(method: string, params?: unknown): Promise<ArrayBuffer> {
    const reply = await this.call<unknown>(method, params);
    if (!(reply instanceof ArrayBuffer)) throw fail("RP-502", { method });
    return reply;
  }

  onTick(listener: (t: Tick) => void) {
    this.tickListeners.add(listener);
    this.ensureTimer();
    return () => this.tickListeners.delete(listener);
  }

  onProject(listener: (r: number) => void) {
    this.world.projectListeners.add(listener);
    return () => this.world.projectListeners.delete(listener);
  }

  private ensureTimer() {
    if (this.timer) return;
    this.lastTime = performance.now();
    this.timer = setInterval(() => this.step(), TICK_MS);
    // In Node the timer must not keep the process alive.
    (this.timer as unknown as { unref?: () => void }).unref?.();
  }

  private step() {
    const w = this.world;
    const now = performance.now();
    const dt = (now - this.lastTime) / 1000;
    this.lastTime = now;
    const wrap = wrapRegion(w);
    if (w.state === "playing") {
      // The position follows the tempo map: tempo changes and ramps as in the engine clock.
      const before = w.position;
      w.position = tickAtSeconds(w.project.tempoMap, secondsAt(w.project.tempoMap, w.position) + dt);
      // As in the engine: only playback that crosses the end jumps back, a playhead already past it runs on.
      if (wrap && before < wrap[1] && w.position >= wrap[1]) w.position = wrap[0];
    }
    const tick: Tick = {
      state: w.state, positionTicks: Math.floor(w.position), cue: w.cue, tempo: tempoAt(w.project.tempoMap, w.position),
      signature: signatureAt(w.project.tempoMap, w.position).signature,
      wrap, songLoop: w.songLoop, metronome: w.metronome, mono: w.mono, automationWrite: w.automationWrite, cpu: CPU_BASE + CPU_SWING * Math.sin(now / CPU_PERIOD_MS), latencyMs: 0,
      meters: mockMeters(w, now),
      reductions: mockReductions(w, now),
      bands: mockBandGains(w, now),
      record: { armed: w.armed, capturing: capturing(w), takeNotes: w.takeNotes, quantizeTicks: w.recordQuantize },
      preview: w.previewPosition(),
    };
    for (const listener of this.tickListeners) listener(tick);
  }
}
