import { useMemo } from "react";
import {
  reactionStatsFor,
  useCommentReactions,
} from "../../../queries/useCommentReactions";
import { useMarketComments } from "../../../queries/useComments";
import { useCommentZaps, zapStatsFor } from "../../../queries/useCommentZaps";
import { useStore } from "../../../store";
import type { Market, MarketComment } from "../../../types";
import { friendlyError } from "../../../utils-react/friendly-error";
import { CommentForm } from "./CommentForm";
import { CommentRow } from "./CommentRow";
import { CommentRowSkeleton } from "./CommentRowSkeleton";

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
  const byId = new Map(comments.map((c) => [c.id, c]));
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
  for (const c of comments) {
    const ancestor = ancestorOf(c);
    const entry =
      groups.get(ancestor.id) ??
      ({ parent: ancestor, replies: [] } as ThreadedComment);
    if (ancestor.id !== c.id) entry.replies.push(c);
    groups.set(ancestor.id, entry);
  }
  // Preserve top-level parent order from the source list (already
  // sorted newest-first upstream). Replies inside each thread are
  // sorted oldest-first so the conversation reads forward.
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
  return out;
}

/** Parse the raw market event JSON to recover the hex event id. */
function extractMarketEventId(market: Market): string | null {
  if (!market.nostrEventJson) return null;
  try {
    const parsed = JSON.parse(market.nostrEventJson) as { id?: string };
    return parsed.id ?? null;
  } catch {
    return null;
  }
}

export function CommentsSection({ market }: { market: Market }) {
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const marketEventId = useMemo(() => extractMarketEventId(market), [market]);

  const {
    data: comments = [],
    isLoading,
    isError,
    error,
    refetch,
  } = useMarketComments(market.marketId, market.creatorPubkey);

  const threaded = useMemo(() => buildThread(comments), [comments]);
  /** Hide deleted top-level comments that carry no replies — nothing
   *  to preserve the thread context for, so the tombstone would be
   *  pure noise. Matches the rule in the plan doc. */
  const visibleThreads = useMemo(
    () => threaded.filter((t) => !t.parent.deleted || t.replies.length > 0),
    [threaded],
  );
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
    <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[28px]">
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
          <ul className="divide-y divide-slate-800/80">
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
        ) : visibleThreads.length === 0 ? (
          <p className="py-6 text-center text-xs text-slate-500">
            No comments yet. Be the first to weigh in.
          </p>
        ) : (
          <ul className="divide-y divide-slate-800/80">
            {visibleThreads.map(({ parent, replies }) => {
              const parentStats = zapStatsFor(zapStats, parent.id);
              const parentReactions = reactionStatsFor(
                reactionStats,
                parent.id,
              );
              return (
                <li key={parent.id}>
                  <CommentRow
                    comment={parent}
                    marketId={market.marketId}
                    creatorPubkey={market.creatorPubkey}
                    marketEventId={marketEventId ?? ""}
                    zapCount={parentStats.count}
                    zapSats={parentStats.totalSats}
                    reactions={parentReactions}
                  />
                  {replies.length > 0 && (
                    <ul className="ml-11 border-l border-slate-800/60 pl-3">
                      {replies.map((r) => {
                        const rStats = zapStatsFor(zapStats, r.id);
                        const rReactions = reactionStatsFor(
                          reactionStats,
                          r.id,
                        );
                        return (
                          <li key={r.id}>
                            <CommentRow
                              comment={r}
                              marketId={market.marketId}
                              creatorPubkey={market.creatorPubkey}
                              marketEventId={marketEventId ?? ""}
                              zapCount={rStats.count}
                              zapSats={rStats.totalSats}
                              reactions={rReactions}
                            />
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>

      {comments.length > 0 && (
        <div className="mt-4 flex justify-center">
          <button
            type="button"
            onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })}
            className="flex items-center gap-1.5 rounded-full border border-slate-800 bg-slate-900/40 px-3 py-1.5 text-xs text-slate-400 transition hover:border-slate-700 hover:bg-slate-900 hover:text-slate-200"
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
