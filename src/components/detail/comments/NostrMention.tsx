import { useNostrProfileByPubkey } from "../../../queries/useNostrProfileByPubkey";
import { hexToNpub } from "../../../utils/crypto";

/**
 * Inline `@name` mention chip. Looks up the referenced pubkey's
 * kind:0 profile (shared React-Query cache, so the same mention
 * repeated in a bio / thread hits the network once), falls back to a
 * truncated npub while loading or when no profile is found. Click
 * opens the author's profile dialog via the parent.
 */
export function NostrMention({
  pubkeyHex,
  onOpen,
}: {
  pubkeyHex: string;
  onOpen: () => void;
}) {
  const { data: profile } = useNostrProfileByPubkey(pubkeyHex);
  const label =
    profile?.display_name?.trim() ||
    profile?.name?.trim() ||
    truncateNpub(pubkeyHex);

  return (
    <button
      type="button"
      onClick={onOpen}
      title={profile?.nip05 || hexToNpub(pubkeyHex)}
      className="mono rounded text-emerald-300 transition hover:text-emerald-200 hover:underline"
    >
      @{label}
    </button>
  );
}

function truncateNpub(pubkeyHex: string): string {
  const npub = hexToNpub(pubkeyHex);
  return `${npub.slice(0, 10)}…${npub.slice(-6)}`;
}
