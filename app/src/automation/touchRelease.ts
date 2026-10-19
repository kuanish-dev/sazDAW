// Live automation writing: a control is released when the mouse button (or a key) goes up anywhere in the window — a
// drag may leave the slider, and the controls themselves know nothing about writing.
import type { AutomationActions } from "./automation";

/** Installs the release on the window; returns the uninstall function. */
export function installTouchRelease(automation: Pick<AutomationActions, "releaseTouches">): () => void {
  const release = () => void automation.releaseTouches();
  window.addEventListener("pointerup", release);
  window.addEventListener("keyup", release);
  return () => {
    window.removeEventListener("pointerup", release);
    window.removeEventListener("keyup", release);
  };
}
