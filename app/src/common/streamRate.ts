// The audio stream's sample rate, for showing a channel's time alignment in samples. Read once when a view mounts: the
// rate changes only on the settings page, and leaving it mounts the views again. Until it is read, the engine's default.
import { type Accessor, createSignal, onMount } from "solid-js";
import { useSession } from "../session";

/** `StreamConfig::DEFAULT` in Rust: the engine runs at this rate until a device opens. */
const DEFAULT_SAMPLE_RATE = 48_000;

export function createStreamRate(): Accessor<number> {
  const session = useSession();
  const [rate, setRate] = createSignal(DEFAULT_SAMPLE_RATE);
  onMount(async () => {
    const status = await session.settings.audio.status();
    if (status && status.sampleRate > 0) setRate(status.sampleRate);
  });
  return rate;
}
