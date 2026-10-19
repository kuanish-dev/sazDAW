// Decoding the binary replies of the methods read many times a second (`RpcClient.callBinary`): the sample windows of
// the live pictures. The format is written by `crates/daw-rpc/src/binary.rs` (and by the browser mock, `mock/wire.ts`):
// little-endian, every field 4 bytes (`end` 8), so each run of samples starts at a multiple of 4 and is viewed in place
// as a `Float32Array`, with no copy and no number parsing. The view reads in the machine's byte order: little-endian on
// every Mac the app runs on.
import type { MasterWindow, NodeWindow } from "./types";

/** `sampleRate: u32`, `frames: u32`, then `frames` samples. */
export function decodeMasterWindow(buffer: ArrayBuffer): MasterWindow {
  const view = new DataView(buffer);
  return { sampleRate: view.getUint32(0, true), samples: new Float32Array(buffer, 8, view.getUint32(4, true)) };
}

/** `count: u32`, then per window `node: u32`, `watch: u32`, `end: u64`, `sampleRate: u32`, `frames: u32`, `frames`
 *  samples of the input and `frames` of the output. */
export function decodeNodeWindows(buffer: ArrayBuffer): NodeWindow[] {
  const view = new DataView(buffer);
  const windows: NodeWindow[] = [];
  let at = 4;
  for (let left = view.getUint32(0, true); left > 0; left--) {
    const frames = view.getUint32(at + 20, true);
    windows.push({
      node: view.getUint32(at, true),
      watch: view.getUint32(at + 4, true),
      // A frame count stays far below 2^53, so the two halves make an exact number.
      end: view.getUint32(at + 8, true) + view.getUint32(at + 12, true) * 2 ** 32,
      sampleRate: view.getUint32(at + 16, true),
      input: new Float32Array(buffer, at + 24, frames),
      output: new Float32Array(buffer, at + 24 + 4 * frames, frames),
    });
    at += 24 + 8 * frames;
  }
  return windows;
}
