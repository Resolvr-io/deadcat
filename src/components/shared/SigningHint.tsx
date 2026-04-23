import { useEffect, useState } from "react";
import { useNip46Connected } from "../../queries/useNip46Connected";

/**
 * Helper line shown during async signing operations when the user is
 * connected to a NIP-46 remote signer. Reminds them the signer app
 * needs their approval (phone might be asleep, another app focused,
 * etc.) instead of leaving them to wonder whether the UI has hung.
 *
 * Delays showing by ~600ms so fast signers don't flash the hint
 * unnecessarily — local nsec signing doesn't render this at all.
 */
export function SigningHint({
  active,
  className = "",
}: {
  /** Usually the mutation's `isPending`. */
  active: boolean;
  className?: string;
}) {
  const connected = useNip46Connected();
  const [show, setShow] = useState(false);

  useEffect(() => {
    if (!active || !connected) {
      setShow(false);
      return;
    }
    const t = setTimeout(() => setShow(true), 600);
    return () => clearTimeout(t);
  }, [active, connected]);

  if (!show) return null;

  return (
    <p
      className={`flex items-center gap-1.5 text-xs text-amber-300 ${className}`}
    >
      <svg
        aria-hidden="true"
        className="h-3.5 w-3.5 shrink-0 animate-pulse"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="5" y="2" width="14" height="20" rx="2" />
        <path d="M12 18h.01" />
      </svg>
      Approve the signing request in your signer app.
    </p>
  );
}
