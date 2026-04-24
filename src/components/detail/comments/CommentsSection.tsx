import { useEffect, useMemo, useState } from "react";
import {
  type ReactionStats,
  reactionStatsFor,
  useCommentReactions,
} from "../../../queries/useCommentReactions";
import { useMarketComments } from "../../../queries/useComments";
import { useCommentZaps, zapStatsFor } from "../../../queries/useCommentZaps";
import { useStore } from "../../../store";
import type { MarketComment } from "../../../types";
import { friendlyError } from "../../../utils-react/friendly-error";
import { CommentForm } from "./CommentForm";
import { CommentRow } from "./CommentRow";
import { CommentRowSkeleton } from "./CommentRowSkeleton";

/** Minimal market shape required to render comments. Satisfied by both
 *  binary `Market` and multi-outcome `MarketGroup`. */
export type CommentableMarket = {
  marketId: string;
  creatorPubkey: string;
  nostrEventJson: string | null | undefined;
};

/** Duration of the ring-pulse highlight applied after a notification
 *  deep-link lands on its target row. Matches the
 *  `deadcat-notification-pulse` keyframe in style.css. */
const PULSE_MS = 2000;
const PULSE_CLASS = "animate-notification-pulse";

/**
 * Flatten nested replies into a single list keyed by the top-level
 * ancestor. The backend's parent_id chain can go arbitrarily deep;
 * for MVP we render replies-to-replies as siblings of the immediate
 * reply rather than deeper indentation — keeps the thread readable
 * on a narrow column and matches how prediction-market discussions
 * actually flow. Matches the rule documented in
 * docs/nostr-social-features-plan.md § C.
 */
type ThreadedComment = {
  parent: MarketComment;
  replies: MarketComment[];
};

function buildThread(comments: MarketComment[]): ThreadedComment[] {
  // Synthesize tombstone placeholders for parents referenced by
  // replies but missing from the result set — typically because the
  // relay honored a kind:5 deletion and removed the event entirely.
  // Without this, orphaned replies would float to the top-level list
  // and appear as standalone comments, stripping them of their
  // original thread context. Showing a "[deleted]" tombstone in the
  // parent's slot preserves the flow ("someone said something; this
  // was a reply to it") without identifying the deleted author.
  const byId = new Map(comments.map((c) => [c.id, c]));
  const marketId = comments[0]?.market_id ?? "";
  const syntheticTombstones = new Map<string, MarketComment>();
  for (const c of comments) {
    if (!c.parent_id) continue;
    if (byId.has(c.parent_id)) continue;
    if (syntheticTombstones.has(c.parent_id)) continue;
    syntheticTombstones.set(c.parent_id, {
      id: c.parent_id,
      author_pubkey: "",
      content: "",
      // 0 flags "synthetic tombstone — we have no source timestamp"
      // so the row can hide the time-ago label.
      created_at: 0,
      market_id: marketId,
      parent_id: null,
      deleted: true,
    });
  }
  const allComments: MarketComment[] = [
    ...comments,
    ...syntheticTombstones.values(),
  ];
  for (const t of syntheticTombstones.values()) byId.set(t.id, t);

  const ancestorOf = (c: MarketComment): MarketComment => {
    let cursor = c;
    // Bounded walk — a runaway chain shouldn't loop forever.
    for (let i = 0; i < 64 && cursor.parent_id; i += 1) {
      const parent = byId.get(cursor.parent_id);
      if (!parent) break;
      cursor = parent;
    }
    return cursor;
  };
  const groups = new Map<string, ThreadedComment>();
  for (const c of allComments) {
    const ancestor = ancestorOf(c);
    const entry =
      groups.get(ancestor.id) ??
      ({ parent: ancestor, replies: [] } as ThreadedComment);
    if (ancestor.id !== c.id) entry.replies.push(c);
    groups.set(ancestor.id, entry);
  }

  // Output order: real top-level parents keep their newest-first
  // position from the source list; synthetic tombstones slot in at
  // the bottom (their created_at is 0). Replies inside each thread
  // are sorted oldest-first so the conversation reads forward.
  const seen = new Set<string>();
  const out: ThreadedComment[] = [];
  for (const c of comments) {
    if (c.parent_id) continue;
    if (seen.has(c.id)) continue;
    seen.add(c.id);
    const entry = groups.get(c.id);
    if (!entry) continue;
    entry.replies.sort((a, b) => a.created_at - b.created_at);
    out.push(entry);
  }
  for (const t of syntheticTombstones.values()) {
    if (seen.has(t.id)) continue;
    const entry = groups.get(t.id);
    if (!entry) continue;
    entry.replies.sort((a, b) => a.created_at - b.created_at);
    out.push(entry);
  }
  return out;
}

/**
 * Render a single top-level thread plus its flattened replies. Owns
 * the reply-target state so the compose box always renders once per
 * thread at the reply-indent level, regardless of which row the
 * user clicked — matches the visual rule that replies-to-replies
 * land as siblings of the first reply, not as a deeper indent.
 */
function Thread({
  parent,
  replies,
  market,
  marketEventId,
  zapStats,
  reactionStats,
}: {
  parent: MarketComment;
  replies: MarketComment[];
  market: CommentableMarket;
  marketEventId: string | null;
  zapStats: Map<string, { count: number; totalSats: number }>;
  reactionStats: Map<string, ReactionStats[]>;
}) {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const [replyTargetId, setReplyTargetId] = useState<string | null>(null);

  const replyTarget = useMemo<MarketComment | null>(() => {
    if (!replyTargetId) return null;
    if (parent.id === replyTargetId) return parent;
    return replies.find((r) => r.id === replyTargetId) ?? null;
  }, [replyTargetId, parent, replies]);

  const toggleReply = (commentId: string) => {
    setReplyTargetId((cur) => (cur === commentId ? null : commentId));
  };
  const canReply = !!sessionPubkey && !!marketEventId && !parent.deleted;

  const parentStats = zapStatsFor(zapStats, parent.id);
  const parentReactions = reactionStatsFor(reactionStats, parent.id);

  return (
    <li>
      <CommentRow
        comment={parent}
        marketId={market.marketId}
        creatorPubkey={market.creatorPubkey}
        zapCount={parentStats.count}
        zapSats={parentStats.totalSats}
        reactions={parentReactions}
        isReplyTarget={replyTargetId === parent.id}
        onToggleReply={canReply ? () => toggleReply(parent.id) : undefined}
      />
      {replies.map((r) => {
        const rStats = zapStatsFor(zapStats, r.id);
        const rReactions = reactionStatsFor(reactionStats, r.id);
        return (
          <div key={r.id} className="border-t border-slate-800/40">
            <CommentRow
              comment={r}
              marketId={market.marketId}
              creatorPubkey={market.creatorPubkey}
              zapCount={rStats.count}
              zapSats={rStats.totalSats}
              reactions={rReactions}
              isReplyTarget={replyTargetId === r.id}
              onToggleReply={
                canReply && !r.deleted ? () => toggleReply(r.id) : undefined
              }
            />
          </div>
        );
      })}
      {replyTarget && marketEventId && (
        <div className="border-t border-slate-800/40 py-3">
          <CommentForm
            marketId={market.marketId}
            creatorPubkey={market.creatorPubkey}
            marketEventId={marketEventId}
            parentEventId={replyTarget.id}
            parentAuthorPubkey={replyTarget.author_pubkey}
            onCancel={() => setReplyTargetId(null)}
            onPosted={() => setReplyTargetId(null)}
            autoFocus
          />
        </div>
      )}
    </li>
  );
}

/** Parse the raw market event JSON to recover the hex event id. */
function extractMarketEventId(market: CommentableMarket): string | null {
  if (!market.nostrEventJson) return null;
  try {
    const parsed = JSON.parse(market.nostrEventJson) as { id?: string };
    return parsed.id ?? null;
  } catch {
    return null;
  }
}

export function CommentsSection({ market }: { market: CommentableMarket }) {
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const focusCommentId = useStore((s) => s.focusCommentId);
  const marketEventId = useMemo(() => extractMarketEventId(market), [market]);

  const {
    data: comments = [],
    isLoading,
    isError,
    error,
    refetch,
  } = useMarketComments(market.marketId, market.creatorPubkey);

  const threaded = useMemo(() => buildThread(comments), [comments]);

  // Notification deep-link lands here. `focusCommentId` is set by
  // the notification bell alongside the market navigation; we wait
  // for the comments query to resolve and the target row to mount,
  // then scroll into view + pulse. Owning the scroll inside
  // `CommentsSection` (instead of a shared hook at the page level)
  // means we don't race with the React Query fetch — we know when
  // the row exists in the rendered list.
  useEffect(() => {
    if (!focusCommentId) return;
    // Only scroll once the target is actually in our data set. If
    // the notification points at a comment that isn't returned for
    // this market (bad id, network mismatch), clear and bail so
    // future notifications aren't blocked.
    if (isLoading) return;
    const present = comments.some((c) => c.id === focusCommentId);
    if (!present) {
      useStore.setState({ focusCommentId: null });
      return;
    }
    const selector = `[data-comment-id="${CSS.escape(focusCommentId)}"]`;
    // Wait one RAF so the newly-rendered row has its position
    // resolved by layout before we scroll — otherwise the scroll
    // snaps to the old position that existed pre-render.
    const raf = window.requestAnimationFrame(() => {
      const el = document.querySelector<HTMLElement>(selector);
      if (!el) {
        // Retry once more on the next frame; this catches the edge
        // where the row's still committing to the DOM.
        window.requestAnimationFrame(() => {
          const retry = document.querySelector<HTMLElement>(selector);
          if (retry) {
            retry.scrollIntoView({ behavior: "smooth", block: "center" });
            retry.classList.add(PULSE_CLASS);
            window.setTimeout(
              () => retry.classList.remove(PULSE_CLASS),
              PULSE_MS,
            );
          }
          useStore.setState({ focusCommentId: null });
        });
        return;
      }
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      el.classList.add(PULSE_CLASS);
      window.setTimeout(() => el.classList.remove(PULSE_CLASS), PULSE_MS);
      useStore.setState({ focusCommentId: null });
    });
    return () => window.cancelAnimationFrame(raf);
  }, [focusCommentId, comments, isLoading]);
  const commentIds = useMemo(() => comments.map((c) => c.id), [comments]);
  const zapStats = useCommentZaps(
    market.marketId,
    market.creatorPubkey,
    commentIds,
  );
  const reactionStats = useCommentReactions(
    market.marketId,
    market.creatorPubkey,
    commentIds,
  );

  const canPost = !!nostrPubkey && !!marketEventId;

  return (
    <section>
      <div className="mb-4 flex items-baseline justify-between gap-3">
        <h3 className="text-base font-semibold text-slate-100">
          Comments <span className="text-slate-500">({comments.length})</span>
        </h3>
      </div>

      {nostrPubkey ? (
        canPost ? (
          <CommentForm
            marketId={market.marketId}
            creatorPubkey={market.creatorPubkey}
            marketEventId={marketEventId}
          />
        ) : (
          <p className="rounded-xl border border-slate-800 bg-slate-950/40 px-4 py-3 text-xs text-slate-500">
            Comments are unavailable for this market — its Nostr event id
            couldn&apos;t be resolved.
          </p>
        )
      ) : (
        <button
          type="button"
          onClick={() => {
            useStore.setState({
              setupRequires: "identity+wallet",
              onboardingStep: "nostr",
              setupModalOpen: true,
            });
          }}
          className="w-full rounded-xl border border-slate-800 bg-slate-950/40 px-4 py-3 text-sm text-slate-300 transition hover:border-slate-700 hover:bg-slate-900/60"
        >
          Sign in to comment
        </button>
      )}

      <div className="mt-5">
        {isLoading ? (
          <ul className="divide-y divide-slate-800/50">
            <li>
              <CommentRowSkeleton variant={2} />
            </li>
            <li>
              <CommentRowSkeleton variant={1} />
            </li>
            <li>
              <CommentRowSkeleton variant={0} />
            </li>
          </ul>
        ) : isError ? (
          <div className="flex flex-col items-center gap-2 py-6 text-xs text-rose-300">
            <span>
              Couldn&apos;t load comments: {friendlyError(String(error))}
            </span>
            <button
              type="button"
              onClick={() => void refetch()}
              className="rounded-md border border-slate-700 px-3 py-1 text-slate-300 transition hover:bg-slate-800"
            >
              Retry
            </button>
          </div>
        ) : threaded.length === 0 ? (
          <p className="py-6 text-center text-xs text-slate-500">
            No comments yet. Be the first to weigh in.
          </p>
        ) : (
          <ul className="divide-y divide-slate-800/50">
            {threaded.map(({ parent, replies }) => (
              <Thread
                key={parent.id}
                parent={parent}
                replies={replies}
                market={market}
                marketEventId={marketEventId}
                zapStats={zapStats}
                reactionStats={reactionStats}
              />
            ))}
          </ul>
        )}
      </div>

      {comments.length > 0 && (
        <div className="mt-4 flex justify-center">
          <button
            type="button"
            onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })}
            className="flex items-center gap-1.5 rounded-full border border-slate-800 px-3 py-1.5 text-xs text-slate-500 transition hover:border-slate-700 hover:text-slate-300"
          >
            <svg
              aria-hidden="true"
              className="h-3.5 w-3.5"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M12 19V5" />
              <path d="M5 12l7-7 7 7" />
            </svg>
            Back to top
          </button>
        </div>
      )}
    </section>
  );
}
