/**
 * Consistent close (X) button used across all modals and overlays.
 *
 * Two variants:
 * - `default` — transparent with a thin border. Reads cleanly over
 *   solid dark dialog backgrounds.
 * - `overlay` — filled slate-900 with backdrop blur. Used when the
 *   button sits on top of a user-supplied image (profile banner,
 *   avatar) where a transparent button would get lost against busy
 *   art. The chip is always legible regardless of the banner behind
 *   it.
 */
export function CloseButton({
  onClick,
  variant = "default",
  className = "",
}: {
  onClick: () => void;
  variant?: "default" | "overlay";
  className?: string;
}) {
  const variantClass =
    variant === "overlay"
      ? "border border-slate-700/80 bg-slate-950/70 text-slate-200 backdrop-blur-sm hover:border-slate-500 hover:bg-slate-900/90 hover:text-white"
      : "border border-slate-700 text-slate-400 hover:border-slate-500 hover:bg-slate-800 hover:text-slate-200";

  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-8 w-8 items-center justify-center rounded-full transition ${variantClass} ${className}`}
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
