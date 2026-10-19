// Gain reduction meter of a compressor, gate or limiter: a bar growing from the left with the value in dB.
import { REDUCTION_SCALE_DB, reductionFraction } from "./dynamicsCurves";
import { t } from "../i18n";

export function ReductionMeter(props: { db: number }) {
  return (
    <div class="flex items-center gap-2" title={t().graph.effect.reduction}>
      <div class="relative h-1.5 flex-1 overflow-hidden rounded-sm bg-panel-3" role="meter" aria-label={t().graph.effect.reduction} aria-valuemin={0} aria-valuemax={REDUCTION_SCALE_DB} aria-valuenow={props.db}>
        <div class="absolute inset-y-0 left-0 bg-accent" style={{ width: `${reductionFraction(props.db) * 100}%` }} />
      </div>
      <span class="w-16 shrink-0 text-right font-mono text-[10px] text-muted">{t().graph.effect.reductionDb(props.db.toFixed(1))}</span>
    </div>
  );
}
