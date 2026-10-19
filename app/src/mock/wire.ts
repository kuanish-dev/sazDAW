// The mock's side of the binary replies (`RpcClient.callBinary`): encodes the live sound windows in the format of
// `crates/daw-rpc/src/binary.rs`, which `api/wire.ts` decodes. The mock builds the windows from plain arrays.
import type { Id } from "../api/types";

/** A node window as the mock builds it. */
export interface MockNodeWindow {
  node: Id;
  watch: number;
  end: number;
  input: ArrayLike<number>;
  output: ArrayLike<number>;
  sampleRate: number;
}

export function encodeMasterWindow(samples: ArrayLike<number>, sampleRate: number): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + 4 * samples.length);
  const view = new DataView(buffer);
  view.setUint32(0, sampleRate, true);
  view.setUint32(4, samples.length, true);
  new Float32Array(buffer, 8, samples.length).set(samples);
  return buffer;
}

export function encodeNodeWindows(windows: MockNodeWindow[]): ArrayBuffer {
  const size = windows.reduce((sum, w) => sum + 24 + 8 * w.input.length, 4);
  const buffer = new ArrayBuffer(size);
  const view = new DataView(buffer);
  view.setUint32(0, windows.length, true);
  let at = 4;
  for (const w of windows) {
    const frames = w.input.length;
    view.setUint32(at, w.node, true);
    view.setUint32(at + 4, w.watch, true);
    view.setUint32(at + 8, w.end % 2 ** 32, true);
    view.setUint32(at + 12, Math.floor(w.end / 2 ** 32), true);
    view.setUint32(at + 16, w.sampleRate, true);
    view.setUint32(at + 20, frames, true);
    new Float32Array(buffer, at + 24, frames).set(w.input);
    new Float32Array(buffer, at + 24 + 4 * frames, frames).set(w.output);
    at += 24 + 8 * frames;
  }
  return buffer;
}
