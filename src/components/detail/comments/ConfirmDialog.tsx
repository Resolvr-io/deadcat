import { useEscapeKey } from "../../../hooks/useEscapeKey";
import { useLockScroll } from "../../../hooks/useLockScroll";
import { CloseButton } from "../../shared/CloseButton";

export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  destructive = false,
  onConfirm,
  onClose,
}: {
  open: boolean;
  title: string;
  body: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  useLockScroll(open);
  useEscapeKey(open, onClose);

  if (!open) return null;

  const confirmClass = destructive
    ? "bg-rose-400 hover:bg-rose-300"
    : "bg-emerald-400 hover:bg-emerald-300";

  return (
    <div
      role="presentation"
      className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="relative mx-4 w-full max-w-sm rounded-2xl border border-slate-700 bg-slate-950 p-6 shadow-2xl">
        <CloseButton onClick={onClose} className="absolute right-4 top-4" />
        <h2 className="pr-8 text-lg font-semibold text-white">{title}</h2>
        <p className="mt-3 text-sm leading-relaxed text-slate-400">{body}</p>
        <div className="mt-6 flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 transition hover:bg-slate-800"
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            onClick={() => {
              onConfirm();
            }}
            className={`rounded-lg px-4 py-2 text-sm font-semibold text-slate-950 transition ${confirmClass}`}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
