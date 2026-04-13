import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export function useActivityTracking(): void {
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;

    function reportActivity(): void {
      if (timer) return;
      timer = setTimeout(() => {
        timer = null;
      }, 30_000);
      void invoke("record_activity");
    }

    const events = ["click", "keydown", "mousemove", "scroll"] as const;
    for (const evt of events) {
      window.addEventListener(evt, reportActivity, { passive: true });
    }

    return () => {
      for (const evt of events) {
        window.removeEventListener(evt, reportActivity);
      }
      if (timer) clearTimeout(timer);
    };
  }, []);
}
