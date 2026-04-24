import { openUrl } from "@tauri-apps/plugin-opener";
import { useMemo, useState } from "react";
import { useEscapeKey } from "../../../hooks/useEscapeKey";
import { useLockScroll } from "../../../hooks/useLockScroll";
import {
  useFollowPubkey,
  useIsFollowing,
  useUnfollowPubkey,
} from "../../../queries/useFollows";
import { useNostrProfileByPubkey } from "../../../queries/useNostrProfileByPubkey";
import { useStore } from "../../../store";
import { hexToNpub } from "../../../utils/crypto";
import { generateAvatarDataUri } from "../../../utils-react/avatar";
import { friendlyError } from "../../../utils-react/friendly-error";
import { CloseButton } from "../../shared/CloseButton";
import { showToast } from "../../shared/Toast";
import { CommentBody } from "./CommentBody";
import { ZapIcon } from "./icons";
import { ZapDialog } from "./ZapDialog";

/**
 * Read-only Nostr profile view for a comment author. Shows banner,
 * avatar, display name, NIP-05, npub, bio, website, and lud16. Follow
 * toggles the viewer's NIP-02 kind:3 contact list; Mute remains a
 * placeholder until NIP-51 wiring lands.
 */
export function CommentProfileDialog({
  pubkeyHex,
  open,
  onClose,
}: {
  pubkeyHex: string | null;
  open: boolean;
  onClose: () => void;
}) {
  useLockScroll(open);
  useEscapeKey(open, onClose);

  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const isSelf = !!sessionPubkey && sessionPubkey === pubkeyHex;
  const [zapOpen, setZapOpen] = useState(false);

  const isFollowing = useIsFollowing(pubkeyHex);
  const followMutation = useFollowPubkey();
  const unfollowMutation = useUnfollowPubkey();
  const followPending = followMutation.isPending || unfollowMutation.isPending;
  const handleToggleFollow = () => {
    if (!sessionPubkey || !pubkeyHex) return;
    const target = pubkeyHex;
    const mutation = isFollowing ? unfollowMutation : followMutation;
    mutation.mutate(target, {
      onSuccess: () => {
        showToast(isFollowing ? "Unfollowed" : "Followed", "success");
      },
      onError: (e) => showToast(friendlyError(String(e)), "error"),
    });
  };

  const { data: profile, isLoading } = useNostrProfileByPubkey(
    open ? pubkeyHex : null,
  );

  const npub = useMemo(
    () => (pubkeyHex ? hexToNpub(pubkeyHex) : ""),
    [pubkeyHex],
  );
  const avatarSrc =
    profile?.picture || (pubkeyHex ? generateAvatarDataUri(pubkeyHex) : "");
  const displayName =
    profile?.display_name?.trim() || profile?.name?.trim() || "Unnamed";

  if (!open || !pubkeyHex) return null;

  const handleCopyNpub = () => {
    void navigator.clipboard.writeText(npub);
    showToast("Copied public key to clipboard");
  };

  return (
    <div
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div className="relative mx-4 flex max-h-[85vh] w-full max-w-2xl flex-col overflow-hidden rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex-1 overflow-y-auto">
          {/* Banner */}
          <div className="relative h-40 bg-slate-900">
            <div className="absolute inset-x-0 top-0 z-10 flex items-center justify-end px-4 py-3">
              <CloseButton onClick={onClose} variant="overlay" />
            </div>
            {profile?.banner && (
              <img
                src={profile.banner}
                alt=""
                className="h-full w-full object-cover"
              />
            )}
          </div>

          {/* Avatar + identity — no overlap with the banner so busy
              banner art (logos, brand imagery) doesn't visually collide
              with the avatar. */}
          <div className="px-6 pt-5">
            <div className="mb-6 flex items-start gap-5">
              <img
                src={avatarSrc}
                alt=""
                className="h-20 w-20 shrink-0 rounded-full border border-slate-800 bg-slate-900 object-cover"
              />
              <div className="min-w-0 flex-1">
                <div className="flex items-start gap-2">
                  <p className="min-w-0 flex-1 truncate text-lg font-medium text-slate-100">
                    {displayName}
                  </p>
                  {/* Action row — hidden on your own profile since
                      none of the actions apply to self. */}
                  {!isSelf && (
                    <div className="flex shrink-0 items-center gap-1.5">
                      {/* Zap requires a session key to sign the
                          NIP-57 zap request. When signed-out we keep
                          the slot as a disabled placeholder so the
                          header layout doesn't shift. */}
                      <button
                        type="button"
                        onClick={() => setZapOpen(true)}
                        disabled={!sessionPubkey || !profile?.lud16}
                        title={
                          !sessionPubkey
                            ? "Sign in to zap"
                            : profile?.lud16
                              ? "Send a zap"
                              : "Recipient has no Lightning address"
                        }
                        className="flex h-8 w-8 items-center justify-center rounded-full border border-slate-700 bg-slate-900/60 text-amber-300 transition hover:border-amber-400/40 hover:bg-amber-400/10 disabled:cursor-default disabled:text-slate-500 disabled:opacity-60"
                      >
                        <ZapIcon className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={handleToggleFollow}
                        disabled={!sessionPubkey || followPending}
                        title={
                          !sessionPubkey
                            ? "Sign in to follow"
                            : isFollowing
                              ? "Unfollow"
                              : "Follow"
                        }
                        aria-pressed={isFollowing}
                        className={`flex h-8 w-8 items-center justify-center rounded-full border transition disabled:cursor-default disabled:opacity-60 ${
                          isFollowing
                            ? "border-emerald-400/40 bg-emerald-400/15 text-emerald-300 hover:border-emerald-400/60 hover:bg-emerald-400/20"
                            : "border-slate-700 bg-slate-900/60 text-slate-300 hover:border-emerald-400/40 hover:bg-emerald-400/10 hover:text-emerald-300"
                        }`}
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
                          <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
                          <circle cx="9" cy="7" r="4" />
                          {isFollowing ? (
                            <path d="m17 11 2 2 4-4" />
                          ) : (
                            <>
                              <path d="M19 8v6" />
                              <path d="M22 11h-6" />
                            </>
                          )}
                        </svg>
                      </button>
                      <button
                        type="button"
                        disabled
                        title="Mute coming soon"
                        className="flex h-8 w-8 cursor-default items-center justify-center rounded-full border border-slate-700 bg-slate-900/60 text-slate-400 opacity-60"
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
                          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
                          <line x1="22" x2="16" y1="9" y2="15" />
                          <line x1="16" x2="22" y1="9" y2="15" />
                        </svg>
                      </button>
                    </div>
                  )}
                </div>
                {profile?.nip05 && (
                  <p className="mt-1.5 flex items-center gap-1.5 truncate text-sm text-slate-300">
                    <svg
                      aria-hidden="true"
                      className="h-4 w-4 shrink-0"
                      viewBox="0 0 122.88 116.87"
                    >
                      <polygon
                        fill="#a855f7"
                        fillRule="evenodd"
                        points="61.37 8.24 80.43 0 90.88 17.79 111.15 22.32 109.15 42.85 122.88 58.43 109.2 73.87 111.15 94.55 91 99 80.43 116.87 61.51 108.62 42.45 116.87 32 99.08 11.73 94.55 13.73 74.01 0 58.43 13.68 42.99 11.73 22.32 31.88 17.87 42.45 0 61.37 8.24"
                      />
                      <path
                        fill="#fff"
                        d="M37.92,65c-6.07-6.53,3.25-16.26,10-10.1,2.38,2.17,5.84,5.34,8.24,7.49L74.66,39.66C81.1,33,91.27,42.78,84.91,49.48L61.67,77.2a7.13,7.13,0,0,1-9.9.44C47.83,73.89,42.05,68.5,37.92,65Z"
                      />
                    </svg>
                    <span className="truncate">{profile.nip05}</span>
                  </p>
                )}
                <button
                  type="button"
                  onClick={handleCopyNpub}
                  title="Click to copy"
                  className="group mono mt-0.5 flex items-center gap-1.5 text-xs text-slate-400 transition hover:text-white"
                >
                  {npub.slice(0, 16)}...{npub.slice(-6)}
                  <svg
                    aria-hidden="true"
                    className="h-3 w-3 shrink-0 text-slate-500 group-hover:text-slate-400"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                  </svg>
                </button>
              </div>
            </div>

            {/* Bio — routed through CommentBody so URLs render as
                link buttons (with the external-link confirm dialog) and
                `nostr:npub1…` / `nostr:nprofile1…` entries resolve to
                @mention chips. */}
            {profile?.about && (
              <div className="mb-5">
                <p className="mb-1 text-xs font-medium uppercase tracking-wide text-slate-500">
                  About
                </p>
                <CommentBody text={profile.about} />
              </div>
            )}

            {/* Links */}
            {(profile?.website || profile?.lud16) && (
              <div className="mb-6 space-y-2">
                {profile.website && (
                  <div>
                    <p className="mb-1 text-xs font-medium uppercase tracking-wide text-slate-500">
                      Website
                    </p>
                    <button
                      type="button"
                      onClick={() => void openUrl(profile.website ?? "")}
                      className="break-all text-sm text-sky-300 underline decoration-sky-400/40 underline-offset-2 transition hover:text-sky-200"
                    >
                      {profile.website}
                    </button>
                  </div>
                )}
                {profile.lud16 && (
                  <div>
                    <p className="mb-1 text-xs font-medium uppercase tracking-wide text-slate-500">
                      Lightning address
                    </p>
                    <p className="mono break-all text-sm text-slate-200">
                      {profile.lud16}
                    </p>
                  </div>
                )}
              </div>
            )}

            {isLoading && !profile && (
              <p className="pb-6 text-center text-xs text-slate-500">
                Loading profile…
              </p>
            )}
            {!isLoading && !profile && (
              <p className="pb-6 text-center text-xs text-slate-500">
                No profile found for this pubkey.
              </p>
            )}
          </div>
        </div>
      </div>

      {sessionPubkey && (
        <ZapDialog
          recipientPubkeyHex={pubkeyHex}
          open={zapOpen}
          onClose={() => setZapOpen(false)}
        />
      )}
    </div>
  );
}
