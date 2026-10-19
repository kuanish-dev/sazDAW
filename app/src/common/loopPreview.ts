// Audition that loops until pressed again, without touching the project (the drum part and chord part dialogs):
// remembers what is playing under a key, replays an edited request in place, and silences it when the dialog closes.
import { createSignal, onCleanup } from "solid-js";

/** What can loop a request: the `drums` or `chords` actions. */
export interface LoopPlayer<R> {
  /** Starts looping `request` (replacing what plays); `false` — it failed (the action has shown why). */
  preview(request: R): Promise<boolean>;
  stopPreview(): Promise<void>;
}

export function createLoopPreview<R>(player: LoopPlayer<R>) {
  /** Key of what is playing (a pattern id, the whole part); `null` — silence. */
  const [playing, setPlaying] = createSignal<string | null>(null);

  const stop = async () => {
    if (playing() === null) return;
    setPlaying(null);
    await player.stopPreview();
  };

  /** Plays `request` under `key`; pressing the playing key again stops it. */
  const toggle = async (key: string, request: R) => {
    if (playing() === key) return stop();
    setPlaying(null);
    if (await player.preview(request)) setPlaying(key);
  };

  /** While something plays, plays the edited `request` in its place (the backend replaces the running preview). */
  const replay = async (request: R) => {
    if (playing() !== null && !(await player.preview(request))) setPlaying(null);
  };

  onCleanup(() => void stop());
  return { playing, toggle, replay, stop };
}
