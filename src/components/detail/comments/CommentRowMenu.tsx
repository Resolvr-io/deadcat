import { type ReactNode, useEffect, useRef, useState } from "react";
import { useEscapeKey } from "../../../hooks/useEscapeKey";
import { hexToNpub } from "../../../utils/crypto";
import { showToast } from "../../shared/Toast";
import { CopyIcon, MoreIcon, MuteIcon, TrashIcon } from "./icons";

/**
 * Kebab "more actions" menu on a comment row. Always exposes Copy
 * text / Copy npub / Mute; shows Delete only when `isOwn`. Mute is a
 * disabled placeholder until NIP-51 publish lands in the backend.
 */
export function CommentRowMenu({
  commentBody,
  authorPubkeyHex,
  isOwn,
  onDelete,
  deletePending,
}: {
  commentBody: string;
  authorPubkeyHex: string;
  isOwn: boolean;
  onDelete: () => void;
  deletePending: boolean;
}) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement | null>(null);

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

  const close = () => setOpen(false);

  const handleCopyText = () => {
    void navigator.clipboard.writeText(commentBody);
    showToast("Copied comment to clipboard");
    close();
  };

  const handleCopyNpub = () => {
    void navigator.clipboard.writeText(hexToNpub(authorPubkeyHex));
    showToast("Copied npub to clipboard");
    close();
  };

  const handleDelete = () => {
    close();
    onDelete();
  };

  return (
    <div ref={wrapperRef} className="relative ml-auto">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title="More actions"
        aria-haspopup="menu"
        aria-expanded={open}
        className="flex h-8 w-8 items-center justify-center rounded-full text-slate-500 transition hover:bg-slate-800/60 hover:text-slate-200"
      >
        <MoreIcon className="h-[18px] w-[18px]" />
      </button>

      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full z-20 mt-1 w-52 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 shadow-xl"
        >
          <MenuItem
            icon={<CopyIcon className="h-4 w-4" />}
            label="Copy comment text"
            onClick={handleCopyText}
          />
          <MenuItem
            icon={<CopyIcon className="h-4 w-4" />}
            label="Copy author npub"
            onClick={handleCopyNpub}
          />
          <MenuItem
            icon={<MuteIcon className="h-4 w-4" />}
            label="Mute author"
            disabled
            hint="Coming soon"
          />
          {isOwn && (
            <>
              <div className="h-px bg-slate-800" />
              <MenuItem
                icon={<TrashIcon className="h-4 w-4" />}
                label={deletePending ? "Deleting…" : "Delete comment"}
                onClick={handleDelete}
                disabled={deletePending}
                destructive
              />
            </>
          )}
        </div>
      )}
    </div>
  );
}

function MenuItem({
  icon,
  label,
  onClick,
  disabled,
  destructive,
  hint,
}: {
  icon: ReactNode;
  label: string;
  onClick?: () => void;
  disabled?: boolean;
  destructive?: boolean;
  hint?: string;
}) {
  const base =
    "flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm transition";
  const tone = destructive
    ? "text-rose-300 hover:bg-rose-500/10"
    : "text-slate-200 hover:bg-slate-800";
  const disabledCls = disabled
    ? "cursor-default text-slate-500 opacity-60 hover:bg-transparent"
    : tone;
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      disabled={disabled}
      className={`${base} ${disabledCls}`}
    >
      <span className="shrink-0">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {hint && <span className="text-xs text-slate-500">{hint}</span>}
    </button>
  );
}
