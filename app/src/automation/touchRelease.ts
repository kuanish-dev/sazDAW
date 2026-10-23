import type { Automation } from "./automation";

/** Installs the release on the window; returns the uninstall function. */
export function installTouchRelease(automation: Pick<Automation, "releaseTouches">): () => void {
  const release = () => void automation.releaseTouches();
  window.addEventListener("pointerup", release);
  window.addEventListener("keyup", release);
  return () => {
    window.removeEventListener("pointerup", release);
    window.removeEventListener("keyup", release);
  };
}
