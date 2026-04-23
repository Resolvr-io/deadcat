import { useMemo, useState } from "react";
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
import { CommentRowMenu } from "./CommentRowMenu";
import { ConfirmDialog } from "./ConfirmDialog";
import { HeartIcon, ReplyIcon, ZapIcon } from "./icons";
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
}: {
  comment: MarketComment;
  marketId: string;
  creatorPubkey: string;
}) {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const { data: profile } = useNostrProfileByPubkey(comment.author_pubkey);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [profileOpen, setProfileOpen] = useState(false);
  const [zapOpen, setZapOpen] = useState(false);
  const [zapCount, setZapCount] = useState(0);
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
        <div className="mt-2 flex items-center gap-1 text-slate-500">
          <PlaceholderAction
            icon={<ReplyIcon className="h-[18px] w-[18px]" />}
            title="Replies coming soon"
          />
          <PlaceholderAction
            icon={<HeartIcon className="h-[18px] w-[18px]" />}
            title="Reactions coming soon"
          />
          {isOwn ? (
            <PlaceholderAction
              icon={<ZapIcon className="h-[18px] w-[18px]" />}
              title="You can't zap your own comment"
              trailing={String(zapCount)}
            />
          ) : (
            <button
              type="button"
              onClick={() => setZapOpen(true)}
              title="Send a zap"
              className="flex items-center gap-1 rounded-full px-2 py-1.5 text-slate-500 transition hover:bg-amber-400/10 hover:text-amber-300"
            >
              <ZapIcon className="h-[18px] w-[18px]" />
              <span className="text-xs">{zapCount}</span>
            </button>
          )}
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
      <ZapDialog
        recipientPubkeyHex={comment.author_pubkey}
        eventIdHex={comment.id}
        open={zapOpen}
        onClose={() => setZapOpen(false)}
        onZapped={() => setZapCount((n) => n + 1)}
      />
    </div>
  );
}
