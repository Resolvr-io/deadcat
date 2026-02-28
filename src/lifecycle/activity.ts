import { tauriApi } from "../api/tauri.ts";

export function setupActivityTracking(): void {
  let activityTimer: ReturnType<typeof setTimeout> | null = null;

  function reportActivity(): void {
    if (activityTimer) return; // throttle: at most once per 30s
    activityTimer = setTimeout(() => {
      activityTimer = null;
    }, 30_000);
    void tauriApi.recordActivity();
  }

  for (const evt of ["click", "keydown", "mousemove", "scroll"] as const) {
    window.addEventListener(evt, reportActivity, { passive: true });
  }
}
