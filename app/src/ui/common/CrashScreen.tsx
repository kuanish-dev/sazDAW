// Screen for when the whole interface crashed (shown by `ErrorBoundary` in `index.tsx`): the cause is already in
// the log, the user can try again.
import { t } from "../../i18n";

export function CrashScreen(props: { error: unknown; onRetry: () => void }) {
  const text = () => (props.error instanceof Error ? `${props.error.name}: ${props.error.message}` : String(props.error));
  return (
    <div class="flex h-full items-center justify-center bg-bg p-6">
      <div class="w-[520px] max-w-full rounded border border-field-line bg-panel p-6" role="alert">
        <h1 class="mb-2 text-lg font-semibold">{t().shell.crash.title}</h1>
        <p class="mb-3 text-sm text-muted">{t().shell.crash.text}</p>
        <pre class="mb-4 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-bg p-2 font-mono text-[11px] text-danger">{text()}</pre>
        <button class="btn btn-primary" onClick={props.onRetry}>{t().shell.crash.retry}</button>
      </div>
    </div>
  );
}
