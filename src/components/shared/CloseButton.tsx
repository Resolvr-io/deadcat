/**
 * Consistent close (X) button used across all modals and overlays.
 * Single circular style that works on any background.
 */
export function CloseButton({
  onClick,
  className = "",
}: {
  onClick: () => void;
  variant?: string;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-8 w-8 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:bg-slate-800 hover:text-slate-200 ${className}`}
      title="Close"
    >
      <svg
        aria-hidden="true"
        className="h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M18 6L6 18M6 6l12 12" />
      </svg>
    </button>
  );
}
