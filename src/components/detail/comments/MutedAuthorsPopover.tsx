import { useEffect, useRef, useState } from "react";
import { useEscapeKey } from "../../../hooks/useEscapeKey";
import { useMuteList, useUnmutePubkey } from "../../../queries/useMutes";
import { useNostrProfileByPubkey } from "../../../queries/useNostrProfileByPubkey";
import { generateAvatarDataUri } from "../../../utils-react/avatar";
import { friendlyError } from "../../../utils-react/friendly-error";
import { showToast } from "../../shared/Toast";

/**
 * In-context unmute affordance. Rendered as a subtle chip in the
 * `CommentsSection` header ("N hidden by mute"); clicking opens a
 * small popover listing every muted pubkey so the user can reverse
 * the action without leaving the page — important because a muted
 * author's comments disappear entirely and there's no other local
 * entry point to their profile. Settings-panel list management
 * remains a follow-up.
 */
export function MutedAuthorsPopover({ hiddenCount }: { hiddenCount: number }) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  useEscapeKey(open, () => setOpen(false));

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e: MouseEvent) {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  const { data: muteList } = useMuteList();
  const pubkeys =
    muteList?.entries
      .filter((e) => e.kind === "p")
      .map((e) => e.value)
      // Stable order so the popover doesn't reshuffle between opens.
      .sort() ?? [];

  if (hiddenCount === 0 && pubkeys.length === 0) return null;

  return (
    <div ref={wrapperRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="text-xs text-slate-500 underline decoration-dotted underline-offset-2 transition hover:text-slate-300"
        aria-haspopup="menu"
        aria-expanded={open}
      >
        {hiddenCount} hidden by mute
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full z-30 mt-1 w-72 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 shadow-xl"
        >
          <div className="border-b border-slate-800 px-3 py-2 text-xs uppercase tracking-wider text-slate-500">
            Muted on your list
          </div>
          <ul className="max-h-80 divide-y divide-slate-800 overflow-y-auto">
            {pubkeys.map((hex) => (
              <li key={hex}>
                <MutedRow pubkeyHex={hex} />
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function MutedRow({ pubkeyHex }: { pubkeyHex: string }) {
  const { data: profile } = useNostrProfileByPubkey(pubkeyHex);
  const unmute = useUnmutePubkey();
  const avatarSrc = profile?.picture || generateAvatarDataUri(pubkeyHex);
  const displayName =
    profile?.display_name?.trim() ||
    profile?.name?.trim() ||
    `${pubkeyHex.slice(0, 8)}…${pubkeyHex.slice(-6)}`;

  return (
    <div className="flex items-center gap-2 px-3 py-2">
      <img
        src={avatarSrc}
        alt=""
        className="h-7 w-7 shrink-0 rounded-full border border-slate-800 bg-slate-900 object-cover"
      />
      <p className="min-w-0 flex-1 truncate text-sm text-slate-200">
        {displayName}
      </p>
      <button
        type="button"
        onClick={() =>
          unmute.mutate(pubkeyHex, {
            onSuccess: () => showToast("Unmuted", "success"),
            onError: (e) => showToast(friendlyError(String(e)), "error"),
          })
        }
        disabled={unmute.isPending}
        className="shrink-0 rounded-md border border-slate-700 px-2 py-1 text-xs text-slate-300 transition hover:border-emerald-400/40 hover:bg-emerald-400/10 hover:text-emerald-300 disabled:cursor-default disabled:opacity-60"
      >
        Unmute
      </button>
    </div>
  );
}
