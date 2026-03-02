import { tauriApi } from "../api/tauri.ts";

export function setupActivityTracking(): () => void {
  let activityTimer: ReturnType<typeof setTimeout> | null = null;
  const events = ["click", "keydown", "mousemove", "scroll"] as const;

  function reportActivity(): void {
    if (activityTimer) return; // throttle: at most once per 30s
    activityTimer = setTimeout(() => {
      activityTimer = null;
    }, 30_000);
    void tauriApi.recordActivity();
  }

  for (const evt of events) {
    window.addEventListener(evt, reportActivity, { passive: true });
  }

  return () => {
    if (activityTimer) {
      clearTimeout(activityTimer);
      activityTimer = null;
    }
    for (const evt of events) {
      window.removeEventListener(evt, reportActivity);
    }
  };
}
