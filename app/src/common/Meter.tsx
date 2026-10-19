// Two-channel level meter with a green → yellow → red scale: the bar (linear peaks after decay, gliding between the
// ~30 Hz readings), a peak-hold tick that waits a moment and then falls, and a clip light on top that stays lit for a
// while after a peak at or above 0 dBFS. The ballistics are `mixer/mixer.ts: stepMeter`, applied in `mixer/levels.ts`.
import { type MeterChannel, type MeterLevel, peakToMeter } from "../mixer/mixer";

const SCALE = "linear-gradient(to top, var(--color-ok) 0%, var(--color-ok) 60%, #e9d24c 80%, var(--color-danger) 100%)";

export function Meter(props: { level: MeterLevel; height?: number }) {
  const height = () => props.height ?? 150;
  const bar = (channel: () => MeterChannel) => {
    const hold = () => peakToMeter(channel().hold);
    return (
      <div class="relative h-full w-1.5 overflow-hidden bg-panel-2">
        <div
          class="absolute bottom-0 w-full transition-[height] duration-[60ms] ease-linear"
          style={{
            height: `${peakToMeter(channel().level) * 100}%`,
            background: SCALE,
            "background-size": `100% ${height()}px`,
            "background-position": "bottom",
          }}
        />
        <div class="absolute h-px w-full bg-text" style={{ bottom: `${hold() * 100}%`, visibility: hold() > 0 ? "visible" : "hidden" }} />
        <div class="absolute inset-x-0 top-0 h-0.5" classList={{ "bg-danger shadow-[0_0_4px_var(--color-danger)]": channel().clip > 0 }} />
      </div>
    );
  };
  return (
    <div class="flex gap-[2px]" style={{ height: `${height()}px` }} aria-hidden="true">
      {bar(() => props.level.l)}
      {bar(() => props.level.r)}
    </div>
  );
}
