import { useMemo, useState } from "react";
import type { ReactionStats } from "../../../queries/useCommentReactions";
import { useDeleteMarketComment } from "../../../queries/useComments";
import { useNostrProfileByPubkey } from "../../../queries/useNostrProfileByPubkey";
import { useStore } from "../../../store";
import type { MarketComment } from "../../../types";
import { generateAvatarDataUri } from "../../../utils-react/avatar";
import { formatTimeAgo } from "../../../utils-react/format";
import { friendlyError } from "../../../utils-react/friendly-error";
import { showToast } from "../../shared/Toast";
import { CommentBody } from "./CommentBody";
import { CommentProfileDialog } from "./CommentProfileDialog";
import { CommentReactions } from "./CommentReactions";
import { CommentRowMenu } from "./CommentRowMenu";
import { ConfirmDialog } from "./ConfirmDialog";
import { ReplyIcon, ZapIcon } from "./icons";
import { ZapDialog } from "./ZapDialog";

function shortPubkey(hex: string): string {
  if (hex.length <= 14) return hex;
  return `${hex.slice(0, 8)}…${hex.slice(-6)}`;
}

function PlaceholderAction({
  icon,
  title,
  trailing,
}: {
  icon: React.ReactNode;
  title: string;
  trailing?: string;
}) {
  return (
    <button
      type="button"
      disabled
      title={title}
      className="flex cursor-default items-center gap-1 rounded-full px-2 py-1.5 text-slate-500 opacity-60"
    >
      {icon}
      {trailing !== undefined && <span className="text-xs">{trailing}</span>}
    </button>
  );
}

export function CommentRow({
  comment,
  marketId,
  creatorPubkey,
  zapCount,
  zapSats,
  reactions,
}: {
  comment: MarketComment;
  marketId: string;
  creatorPubkey: string;
  /** Number of kind:9735 zap receipts seen for this comment. */
  zapCount: number;
  /** Total sats zapped across those receipts (floor of msats/1000). */
  zapSats: number;
  /** NIP-25 reactions aggregated per emoji; empty when none seen. */
  reactions: ReactionStats[];
}) {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const { data: profile } = useNostrProfileByPubkey(comment.author_pubkey);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [profileOpen, setProfileOpen] = useState(false);
  const [zapOpen, setZapOpen] = useState(false);
  const deleteMutation = useDeleteMarketComment(marketId, creatorPubkey);

  const avatarSrc = useMemo(
    () => profile?.picture || generateAvatarDataUri(comment.author_pubkey),
    [profile?.picture, comment.author_pubkey],
  );

  const displayName =
    profile?.display_name?.trim() ||
    profile?.name?.trim() ||
    shortPubkey(comment.author_pubkey);

  const isOwn = sessionPubkey === comment.author_pubkey;
  const tsIso = new Date(comment.created_at * 1000).toISOString();
  const tsRel = formatTimeAgo(comment.created_at);

  const handleConfirmDelete = () => {
    setConfirmOpen(false);
    deleteMutation.mutate(comment.id, {
      onError: (e) => {
        showToast(friendlyError(String(e)), "error");
      },
    });
  };

  return (
    <div className="flex items-start gap-3 py-3">
      <button
        type="button"
        onClick={() => setProfileOpen(true)}
        title="View profile"
        className="shrink-0"
      >
        <img
          src={avatarSrc}
          alt=""
          className="h-8 w-8 rounded-full border border-slate-800 bg-slate-900 object-cover transition hover:border-slate-600"
        />
      </button>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <button
            type="button"
            onClick={() => setProfileOpen(true)}
            title={profile?.nip05 || comment.author_pubkey}
            className="truncate text-left text-sm font-semibold text-slate-100 transition hover:text-white hover:underline"
          >
            {displayName}
          </button>
          <span className="shrink-0 text-xs text-slate-500" title={tsIso}>
            {tsRel}
          </span>
        </div>
        <div className="mt-1">
          <CommentBody text={comment.content} />
        </div>
        {/* Reaction row renders above the action row so the emoji
            pills read naturally as part of the comment body. Hidden
            when there are no reactions yet and the viewer can't add
            one (signed-out readers see an empty row until someone
            reacts). */}
        {(reactions.length > 0 ||
          (sessionPubkey && sessionPubkey !== comment.author_pubkey)) && (
          <CommentReactions
            commentEventId={comment.id}
            commentAuthorPubkey={comment.author_pubkey}
            stats={reactions}
          />
        )}
        <div className="mt-2 flex items-center gap-1 text-slate-500">
          <PlaceholderAction
            icon={<ReplyIcon className="h-[18px] w-[18px]" />}
            title="Replies coming soon"
          />
          {(() => {
            // Show total sats when zaps exist (matches Primal / Jumble
            // convention); fall back to the count when nothing's been
            // zapped so the icon still has a number next to it.
            const trailing =
              zapCount > 0 ? zapSats.toLocaleString() : String(zapCount);
            const title =
              zapCount > 0
                ? `${zapSats.toLocaleString()} sats from ${zapCount} zap${zapCount === 1 ? "" : "s"}`
                : "Send a zap";
            if (isOwn) {
              return (
                <PlaceholderAction
                  icon={<ZapIcon className="h-[18px] w-[18px]" />}
                  title="You can't zap your own comment"
                  trailing={trailing}
                />
              );
            }
            // NIP-57 requires a zap request signed by the sender's
            // key — without an identity there's nothing to sign with.
            // Render the count as a disabled placeholder so the
            // visual slot stays consistent, and nudge toward sign-in
            // via the hover title.
            if (!sessionPubkey) {
              return (
                <PlaceholderAction
                  icon={<ZapIcon className="h-[18px] w-[18px]" />}
                  title="Sign in to zap"
                  trailing={trailing}
                />
              );
            }
            return (
              <button
                type="button"
                onClick={() => setZapOpen(true)}
                title={title}
                className="flex items-center gap-1 rounded-full px-2 py-1.5 text-slate-500 transition hover:bg-amber-400/10 hover:text-amber-300"
              >
                <ZapIcon className="h-[18px] w-[18px]" />
                <span className="text-xs">{trailing}</span>
              </button>
            );
          })()}
          <CommentRowMenu
            commentBody={comment.content}
            authorPubkeyHex={comment.author_pubkey}
            isOwn={isOwn}
            onDelete={() => setConfirmOpen(true)}
            deletePending={deleteMutation.isPending}
          />
        </div>
      </div>
      <ConfirmDialog
        open={confirmOpen}
        title="Delete comment?"
        body="This publishes a deletion request to relays. Already-propagated copies may remain visible on some clients."
        confirmLabel="Delete"
        destructive
        onConfirm={handleConfirmDelete}
        onClose={() => setConfirmOpen(false)}
      />
      <CommentProfileDialog
        pubkeyHex={comment.author_pubkey}
        open={profileOpen}
        onClose={() => setProfileOpen(false)}
      />
      {sessionPubkey && (
        <ZapDialog
          recipientPubkeyHex={comment.author_pubkey}
          eventIdHex={comment.id}
          open={zapOpen}
          onClose={() => setZapOpen(false)}
        />
      )}
    </div>
  );
}
