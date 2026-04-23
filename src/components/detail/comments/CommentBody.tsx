import { useState } from "react";
import { parseCommentBody } from "../../../utils-react/comment-body";
import { LinkConfirmDialog } from "./LinkConfirmDialog";

/**
 * Renders a comment body as a mix of plain text and URL buttons.
 * URLs are NEVER auto-opened — clicking one opens LinkConfirmDialog so
 * the user sees exactly where they're about to go before leaving the
 * app.
 */
export function CommentBody({ text }: { text: string }) {
  const [pendingUrl, setPendingUrl] = useState<string | null>(null);
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
        })}
      </p>
      <LinkConfirmDialog
        url={pendingUrl ?? ""}
        open={pendingUrl !== null}
        onClose={() => setPendingUrl(null)}
      />
    </>
  );
}
