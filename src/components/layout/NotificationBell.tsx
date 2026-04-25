import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useNostrProfileByPubkey } from "../../queries/useNostrProfileByPubkey";
import {
  useMarkAllNotificationsRead,
  useMarkNotificationRead,
  useNotifications,
  useUnreadNotificationCount,
} from "../../queries/useNotifications";
import { useStore } from "../../store";
import type { Notification } from "../../types";
import { generateAvatarDataUri } from "../../utils-react/avatar";
import { formatTimeAgo } from "../../utils-react/format";

/**
 * Bell-icon trigger in the top shell with an unread dot. Clicking
 * toggles a dropdown listing the most recent notifications. Rows
 * deep-link to the originating market with the triggering comment
 * scrolled into view + pulsed via `useScrollToComment`.
 *
 * Only renders when the user is signed in — no notifications can
 * target a pubkey that doesn't exist, and the Rust store isn't
 * initialized until the node is up.
 */
export function NotificationBell() {
  const open = useStore((s) => s.notificationsOpen);
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const { data: unreadCount = 0 } = useUnreadNotificationCount();
  const wrapperRef = useRef<HTMLDivElement>(null);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        useStore.setState({ notificationsOpen: false });
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  // Close on Escape.
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") useStore.setState({ notificationsOpen: false });
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  if (!sessionPubkey) return null;

  return (
    <div ref={wrapperRef} className="relative">
      <button
        type="button"
        onClick={() =>
          useStore.setState((s) => ({
            notificationsOpen: !s.notificationsOpen,
          }))
        }
        title="Notifications"
        aria-haspopup="menu"
        aria-expanded={open}
        className="relative flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200"
      >
        <svg
          aria-hidden="true"
          className="h-4 w-4"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" />
          <path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />
        </svg>
        {unreadCount > 0 && (
          <span className="absolute top-1 right-1 flex h-2 w-2 items-center justify-center rounded-full bg-rose-500" />
        )}
      </button>

      {open && <NotificationPopover />}
    </div>
  );
}

function NotificationPopover() {
  const { data: items = [], isLoading } = useNotifications(30);
  const markAll = useMarkAllNotificationsRead();
  const hasUnread = items.some((n) => !n.read);

  return (
    <div
      role="menu"
      className="absolute right-0 top-full z-30 mt-2 max-h-[70vh] w-[360px] overflow-hidden rounded-xl border border-slate-700 bg-slate-900 shadow-2xl"
    >
      <div className="flex items-center justify-between border-b border-slate-800 px-4 py-3">
        <h3 className="text-sm font-semibold text-slate-100">Notifications</h3>
        {hasUnread && (
          <button
            type="button"
            onClick={() => markAll.mutate()}
            disabled={markAll.isPending}
            className="text-xs text-slate-400 transition hover:text-slate-200 disabled:opacity-50"
          >
            Mark all read
          </button>
        )}
      </div>
      <div className="max-h-[60vh] overflow-y-auto">
        {isLoading ? (
          <p className="py-6 text-center text-xs text-slate-500">Loading…</p>
        ) : items.length === 0 ? (
          <p className="mx-auto max-w-[280px] px-6 py-10 text-center text-xs leading-relaxed text-slate-500">
            Nothing new yet. Replies, reactions, and zaps on your comments show
            up here.
          </p>
        ) : (
          <ul className="divide-y divide-slate-800">
            {items.map((n) => (
              <li key={n.eventId}>
                <NotificationRow notification={n} />
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function shortPubkey(hex: string): string {
  if (hex.length <= 14) return hex;
  return `${hex.slice(0, 8)}…${hex.slice(-6)}`;
}

function NotificationRow({ notification }: { notification: Notification }) {
  const markRead = useMarkNotificationRead();
  const { data: profile } = useNostrProfileByPubkey(notification.authorPubkey);
  const queryClient = useQueryClient();

  const displayName =
    profile?.display_name?.trim() ||
    profile?.name?.trim() ||
    shortPubkey(notification.authorPubkey);
  const avatarSrc =
    profile?.picture || generateAvatarDataUri(notification.authorPubkey);

  const verb =
    notification.kind === "reply"
      ? "replied to your comment"
      : notification.kind === "reaction"
        ? `reacted ${notification.emoji ?? ""}`.trim()
        : notification.kind === "zap"
          ? `zapped you ${
              notification.amountMsats
                ? `${Math.floor(notification.amountMsats / 1000).toLocaleString()} sats`
                : ""
            }`.trim()
          : "mentioned you";

  const handleClick = () => {
    if (!notification.read) markRead.mutate(notification.eventId);
    if (notification.marketId) {
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    }
    useStore.setState({
      notificationsOpen: false,
      ...(notification.marketId
        ? {
            selectedMarketId: notification.marketId,
            view: "detail" as const,
            focusCommentId: notification.commentId ?? null,
          }
        : {}),
    });
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      className={`flex w-full items-start gap-3 px-4 py-3 text-left transition hover:bg-slate-800 ${
        notification.read ? "opacity-70" : ""
      }`}
    >
      <img
        src={avatarSrc}
        alt=""
        className="h-8 w-8 shrink-0 rounded-full border border-slate-800 bg-slate-900 object-cover"
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <p className="truncate text-sm text-slate-200">
            <span className="font-semibold">{displayName}</span>{" "}
            <span className="text-slate-400">{verb}</span>
          </p>
          <span
            className="shrink-0 text-[10px] text-slate-500"
            title={new Date(notification.createdAt * 1000).toISOString()}
          >
            {formatTimeAgo(notification.createdAt)}
          </span>
        </div>
        {notification.bodyPreview && (
          <p className="mt-0.5 line-clamp-2 text-xs text-slate-400">
            {notification.bodyPreview}
          </p>
        )}
      </div>
      {!notification.read && (
        <span className="mt-2 h-2 w-2 shrink-0 rounded-full bg-rose-500" />
      )}
    </button>
  );
}
