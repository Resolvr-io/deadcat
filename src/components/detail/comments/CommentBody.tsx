import { useState } from "react";
import { parseCommentBody } from "../../../utils-react/comment-body";
import { CommentProfileDialog } from "./CommentProfileDialog";
import { LinkConfirmDialog } from "./LinkConfirmDialog";
import { NostrMention } from "./NostrMention";

/**
 * Renders body text as a mix of plain-text, URL buttons, Nostr
 * mentions, and inert Nostr-entity chips. URLs are never auto-opened
 * — clicking surfaces `LinkConfirmDialog` so the user sees the
 * destination before leaving the app. Nostr mentions resolve to
 * `@<display_name>` via a cached profile fetch and open the author's
 * profile dialog on click.
 */
export function CommentBody({ text }: { text: string }) {
  const [pendingUrl, setPendingUrl] = useState<string | null>(null);
  const [openProfileHex, setOpenProfileHex] = useState<string | null>(null);
  const segments = parseCommentBody(text);

  return (
    <>
      <p className="whitespace-pre-wrap break-words text-sm leading-relaxed text-slate-200">
        {segments.map((seg, idx) => {
          if (seg.kind === "text") {
            return (
              <span key={`t-${idx}-${seg.value.length}`}>{seg.value}</span>
            );
          }
          if (seg.kind === "url") {
            return (
              <button
                key={`u-${idx}-${seg.value}`}
                type="button"
                onClick={() => setPendingUrl(seg.value)}
                className="break-all text-sky-300 underline decoration-sky-400/40 underline-offset-2 transition hover:text-sky-200 hover:decoration-sky-300"
              >
                {seg.value}
              </button>
            );
          }
          if (seg.kind === "nostrMention") {
            return (
              <NostrMention
                key={`m-${idx}-${seg.pubkeyHex}`}
                pubkeyHex={seg.pubkeyHex}
                onOpen={() => setOpenProfileHex(seg.pubkeyHex)}
              />
            );
          }
          // Nostr entity (nevent / note / naddr) — no surface to open
          // yet; render as a truncated, muted chip so the line stays
          // readable.
          const short = `${seg.entity.slice(0, 10)}…${seg.entity.slice(-6)}`;
          return (
            <span
              key={`e-${idx}-${seg.entity}`}
              title={seg.raw}
              className="mono inline-flex items-center rounded-md bg-slate-800/60 px-1.5 py-0.5 text-xs text-slate-400"
            >
              {short}
            </span>
          );
        })}
      </p>
      <LinkConfirmDialog
        url={pendingUrl ?? ""}
        open={pendingUrl !== null}
        onClose={() => setPendingUrl(null)}
      />
      <CommentProfileDialog
        pubkeyHex={openProfileHex}
        open={openProfileHex !== null}
        onClose={() => setOpenProfileHex(null)}
      />
    </>
  );
}
