// Live automation writing: a control is released when the mouse button (or a key) goes up anywhere in the window — a
// drag may leave the slider, and the controls themselves know nothing about writing.
import type { Session } from "./session";

/** Installs the release on the window; returns the uninstall function. */
export function installTouchRelease(session: Session): () => void {
  const release = () => void session.actions.automation.releaseTouches();
  window.addEventListener("pointerup", release);
  window.addEventListener("keyup", release);
  return () => {
    window.removeEventListener("pointerup", release);
    window.removeEventListener("keyup", release);
  };
}
