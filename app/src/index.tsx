/* @refresh reload */
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
if (import.meta.env.DEV) Object.assign(window, { __rpc: rpc });

const reporter = createErrorReporter(createApi(rpc).log.write);
reporter.install(window);

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
