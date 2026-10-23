import { clamp } from "../common/math";

export interface View {
  /** First visible frame. */
  start: number;
  /** Number of visible frames. */
  span: number;
}

export const MIN_SPAN = 64;

export function fitAll(total: number): View {
  return { start: 0, span: Math.max(MIN_SPAN, total) };
}

/** Keeps the window inside the file and prevents zooming in deeper than `MIN_SPAN` frames. */
export function clampView(view: View, total: number): View {
  const span = clamp(Math.round(view.span), MIN_SPAN, Math.max(MIN_SPAN, total));
  const start = clamp(Math.round(view.start), 0, Math.max(0, total - span));
  return { start, span };
}

/** Zoom around frame `anchor`, which stays under the cursor. `factor > 1` zooms in. */
export function zoomAt(view: View, anchor: number, factor: number, total: number): View {
  const span = view.span / factor;
  const ratio = view.span === 0 ? 0.5 : (anchor - view.start) / view.span;
  return clampView({ start: anchor - ratio * span, span }, total);
}

export function panBy(view: View, deltaFrames: number, total: number): View {
  return clampView({ start: view.start + deltaFrames, span: view.span }, total);
}

export function frameAtX(x: number, width: number, view: View): number {
  return view.start + (x / Math.max(1, width)) * view.span;
}

export function xAtFrame(frame: number, width: number, view: View): number {
  return ((frame - view.start) / view.span) * width;
}

export interface Selection {
  from: number;
  to: number;
}

/** Selection between two frames in any order; clipped to the file; `null` if empty. */
export function normalizeSelection(a: number, b: number, total: number): Selection | null {
  const from = Math.max(0, Math.round(Math.min(a, b)));
  const to = Math.min(total, Math.round(Math.max(a, b)));
  return to - from >= 1 ? { from, to } : null;
}

/** Markers are sorted, deduplicated and exclude the file edges (the first and last positions are not markers). */
export function cleanMarkers(markers: number[], total: number): number[] {
  return [...new Set(markers.map(Math.round))].filter((m) => m > 0 && m < total).sort((a, b) => a - b);
}

export function addMarker(markers: number[], frame: number, total: number): number[] {
  return cleanMarkers([...markers, frame], total);
}

/** The marker nearest to `frame` within `tolerance` frames; otherwise `null`. */
export function nearestMarker(markers: number[], frame: number, tolerance: number): number | null {
  let best: number | null = null;
  for (const marker of markers) {
    const distance = Math.abs(marker - frame);
    if (distance <= tolerance && (best === null || distance < Math.abs(best - frame))) best = marker;
  }
  return best;
}

export function removeMarker(markers: number[], marker: number): number[] {
  return markers.filter((m) => m !== marker);
}

export function moveMarker(markers: number[], from: number, to: number, total: number): number[] {
  return cleanMarkers(markers.map((m) => (m === from ? to : m)), total);
}

/** Segments between markers: from the file start to the first, between markers, from the last to the end. */
export function segments(markers: number[], total: number): Selection[] {
  const bounds = [0, ...cleanMarkers(markers, total), total];
  return bounds.slice(0, -1).map((from, i) => ({ from, to: bounds[i + 1]! }));
}

/** The segment between markers that contains the frame. */
export function segmentAt(markers: number[], frame: number, total: number): Selection | null {
  return segments(markers, total).find((s) => frame >= s.from && frame < s.to) ?? null;
}

/**
 * Region -> sampler `start`/`end` parameters (fractions of the file length). Must match the backend:
 * there the frame boundary is `round(fraction × (frames − 1))`.
 */
export function regionToParams(selection: Selection, total: number): { start: number; end: number } {
  const last = Math.max(1, total - 1);
  return { start: Math.min(1, selection.from / last), end: Math.min(1, selection.to / last) };
}

export function framesToSeconds(frames: number, sampleRate: number): number {
  return frames / Math.max(1, sampleRate);
}

/** "0:01.250": minutes, seconds and milliseconds. */
export function formatTime(frames: number, sampleRate: number): string {
  const seconds = Math.max(0, framesToSeconds(frames, sampleRate));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${(seconds - minutes * 60).toFixed(3).padStart(6, "0")}`;
}

/** Time ruler step: a "round" number of seconds so labels are at least `minPixels` apart. */
export function rulerStep(span: number, sampleRate: number, width: number, minPixels = 70): number {
  const secondsPerPixel = span / Math.max(1, width) / Math.max(1, sampleRate);
  const target = secondsPerPixel * minPixels;
  const steps = [0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 30, 60, 120, 300];
  return steps.find((s) => s >= target) ?? 600;
}

/** Viewport size for scrolling via the overview strip: start and end fractions 0..1. */
export function viewFractions(view: View, total: number): { from: number; to: number } {
  const t = Math.max(1, total);
  return { from: view.start / t, to: Math.min(1, (view.start + view.span) / t) };
}
