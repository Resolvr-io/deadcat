import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { useStore } from "../store";
import type { Notification } from "../types";

const NOTIFICATIONS_KEY = ["notifications"] as const;
const UNREAD_KEY = ["notifications", "unread"] as const;

/** Wire a single global listener for `notifications_updated` Tauri
 *  events. Invalidates both the list and the unread count so any
 *  mounted subscriber re-fetches. Called once near the app root
 *  (via `useBootstrap` or equivalent) — calling it again from other
 *  hooks is safe but wasteful. */
export function useNotificationsEventListener() {
  const qc = useQueryClient();
  useEffect(() => {
    const promise = listen("notifications_updated", () => {
      void qc.invalidateQueries({ queryKey: NOTIFICATIONS_KEY });
    });
    return () => {
      void promise.then((unlisten) => unlisten());
    };
  }, [qc]);
}

/** Most-recent-first notification list, capped at `limit` server-side
 *  (the backend retains 500 max). Disabled when signed-out — nothing
 *  meaningful to show, and the Rust store isn't initialized yet. */
export function useNotifications(limit = 30) {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  return useQuery<Notification[]>({
    queryKey: [...NOTIFICATIONS_KEY, limit, sessionPubkey ?? ""],
    queryFn: () => invoke<Notification[]>("list_notifications", { limit }),
    enabled: !!sessionPubkey,
    staleTime: 30_000,
  });
}

/** Lightweight count used to drive the red dot on the bell icon. */
export function useUnreadNotificationCount() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  return useQuery<number>({
    queryKey: [...UNREAD_KEY, sessionPubkey ?? ""],
    queryFn: () => invoke<number>("unread_notification_count"),
    enabled: !!sessionPubkey,
    staleTime: 30_000,
  });
}

/** Mark a single notification as read. Backend flushes to disk and
 *  emits `notifications_updated`; our event listener invalidates
 *  both queries so the UI updates without an extra round-trip. */
export function useMarkNotificationRead() {
  return useMutation({
    mutationFn: (eventId: string) =>
      invoke<void>("mark_notification_read", { eventId }),
  });
}

/** "Mark all as read" on the popover. Single backend call; same
 *  invalidation path via the shared event listener. */
export function useMarkAllNotificationsRead() {
  return useMutation({
    mutationFn: () => invoke<void>("mark_all_notifications_read"),
  });
}
