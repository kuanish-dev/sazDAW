/* @refresh reload */
// Entry point: applies the cached look, wires the error reporter and renders the app inside a crash boundary.
import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";
import { createApi } from "./api/api";
import { App } from "./App";
import { createDialogs, createRpc, createShell } from "./composition";
import "./index.css";
import { createErrorReporter } from "./log/errorReporter";
import { applyCachedLook } from "./settings/appearance";
import { CrashScreen } from "./common/CrashScreen";

const rpc = createRpc();
// In development the connection is reachable from the browser console: `await __rpc.call("project.get")`.
if (import.meta.env.DEV) Object.assign(window, { __rpc: rpc });

// All interface errors go to the log on disk (through the same RPC) for later investigation. Installed first, so even
// a failure of the look cache below reaches the log.
const reporter = createErrorReporter(createApi(rpc).log.write);
reporter.install(window);

// The previous run's theme before the first frame; the real preferences arrive from the backend a bit later.
applyCachedLook();

render(
  () => (
    <ErrorBoundary
      fallback={(error, reset) => {
        reporter.report("error", "frontend", error, "ErrorBoundary: interface crashed");
        return <CrashScreen error={error} onRetry={reset} />;
      }}
    >
      <App rpc={rpc} dialogs={createDialogs()} shell={createShell()} />
    </ErrorBoundary>
  ),
  document.getElementById("root")!,
);
