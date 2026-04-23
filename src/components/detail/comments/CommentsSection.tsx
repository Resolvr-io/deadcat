import { useMemo } from "react";
import { useMarketComments } from "../../../queries/useComments";
import { useStore } from "../../../store";
import type { Market } from "../../../types";
import { friendlyError } from "../../../utils-react/friendly-error";
import { CommentForm } from "./CommentForm";
import { CommentRow } from "./CommentRow";

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
          <div className="flex items-center justify-center py-6 text-xs text-slate-500">
            Loading comments…
          </div>
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
        ) : comments.length === 0 ? (
          <p className="py-6 text-center text-xs text-slate-500">
            No comments yet. Be the first to weigh in.
          </p>
        ) : (
          <ul className="divide-y divide-slate-800/80">
            {comments.map((c) => (
              <li key={c.id}>
                <CommentRow
                  comment={c}
                  marketId={market.marketId}
                  creatorPubkey={market.creatorPubkey}
                />
              </li>
            ))}
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
