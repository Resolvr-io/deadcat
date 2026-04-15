import { useCallback } from "react";
import { useStore } from "../../store";
import { hexToNpub } from "../../utils/crypto";
import { CloseButton } from "../shared/CloseButton";
import { showToast } from "../shared/Toast";

function truncHex(v: string): string {
  return v.length > 16 ? `${v.slice(0, 8)}\u2026${v.slice(-8)}` : v;
}

function truncBech32(v: string): string {
  return v.length > 24 ? `${v.slice(0, 12)}\u2026${v.slice(-8)}` : v;
}

function CopyableField({
  value,
  displayValue,
}: {
  value: string;
  displayValue?: string;
}) {
  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(value);
    showToast("Copied to clipboard");
  }, [value]);

  const display = displayValue ? truncBech32(displayValue) : truncHex(value);

  return (
    <button
      type="button"
      onClick={handleCopy}
      className="flex w-full min-w-0 items-center gap-1.5 overflow-hidden font-mono text-xs text-slate-200 transition-colors hover:text-white"
    >
      <span className="truncate">{display}</span>
      <svg
        aria-hidden="true"
        xmlns="http://www.w3.org/2000/svg"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="shrink-0 text-slate-500"
      >
        <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
        <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
      </svg>
    </button>
  );
}

export function NostrEventModal() {
  const nostrEventModal = useStore((s) => s.nostrEventModal);
  const nostrEventJson = useStore((s) => s.nostrEventJson);
  const nostrEventNevent = useStore((s) => s.nostrEventNevent);
  const relays = useStore((s) => s.relays);

  const handleClose = useCallback(() => {
    useStore.setState({
      nostrEventModal: false,
      nostrEventJson: null,
      nostrEventNevent: null,
    });
  }, []);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (e.target === e.currentTarget) {
        handleClose();
      }
    },
    [handleClose],
  );

  const handleCopyJson = useCallback(() => {
    if (nostrEventJson) {
      void navigator.clipboard.writeText(nostrEventJson);
      showToast("Copied event JSON");
    }
  }, [nostrEventJson]);

  const handleCopyNevent = useCallback(() => {
    if (nostrEventNevent) {
      void navigator.clipboard.writeText(nostrEventNevent);
      showToast("Copied event ID");
    }
  }, [nostrEventNevent]);

  if (!nostrEventModal || !nostrEventJson) return null;

  let parsed: {
    id?: string;
    pubkey?: string;
    kind?: number;
    created_at?: number;
    tags?: string[][];
    content?: string;
    sig?: string;
  } | null = null;
  try {
    parsed = JSON.parse(nostrEventJson);
  } catch {
    /* raw fallback */
  }

  let contentPretty: string | null = null;
  if (parsed?.content) {
    try {
      contentPretty = JSON.stringify(JSON.parse(parsed.content), null, 2);
    } catch {
      contentPretty = parsed.content;
    }
  }

  const neventDisplay = nostrEventNevent ?? parsed?.id;
  const npubDisplay = parsed?.pubkey ? hexToNpub(parsed.pubkey) : undefined;

  return (
    <div
      role="presentation"
      onClick={handleBackdropClick}
      onKeyDown={(e) => {
        if (e.key === "Escape") handleClose();
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    >
      <div className="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div className="flex items-center gap-2.5">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-violet-500/15">
              <svg
                aria-hidden="true"
                xmlns="http://www.w3.org/2000/svg"
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                className="text-violet-400"
              >
                <circle cx="12" cy="12" r="10" />
                <line x1="2" y1="12" x2="22" y2="12" />
                <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
              </svg>
            </div>
            <h3 className="text-lg font-medium text-slate-100">Nostr Event</h3>
          </div>
          <CloseButton onClick={handleClose} />
        </div>
        <div className="flex flex-col gap-3 p-6 max-h-[70vh] overflow-y-auto">
          {parsed ? (
            <>
              {/* Event ID */}
              {parsed.id && (
                <div className="min-w-0">
                  <span className="mb-0.5 block text-xs text-slate-500">
                    Event ID
                  </span>
                  <CopyableField
                    value={neventDisplay ?? parsed.id}
                    displayValue={neventDisplay}
                  />
                </div>
              )}

              {/* Public Key */}
              {parsed.pubkey && (
                <div className="min-w-0">
                  <span className="mb-0.5 block text-xs text-slate-500">
                    Public Key
                  </span>
                  <CopyableField
                    value={npubDisplay ?? parsed.pubkey}
                    displayValue={npubDisplay}
                  />
                </div>
              )}

              {/* Kind + Created */}
              <div className="grid grid-cols-2 gap-3">
                {parsed.kind !== undefined && (
                  <div className="min-w-0">
                    <span className="mb-0.5 block text-xs text-slate-500">
                      Kind
                    </span>
                    <p className="truncate text-xs text-slate-200">
                      {parsed.kind}
                    </p>
                  </div>
                )}
                {parsed.created_at && (
                  <div className="min-w-0">
                    <span className="mb-0.5 block text-xs text-slate-500">
                      Created
                    </span>
                    <p className="truncate text-xs text-slate-200">
                      {new Date(parsed.created_at * 1000).toLocaleString()}
                    </p>
                  </div>
                )}
              </div>

              {/* Relays */}
              {relays.length > 0 && (
                <div>
                  <span className="mb-0.5 block text-xs text-slate-500">
                    Relays
                  </span>
                  <div className="flex flex-wrap gap-1">
                    {relays.map((relay) => {
                      const display = relay.url.replace(/^wss?:\/\//, "");
                      return (
                        <span
                          key={relay.url}
                          className="inline-flex items-center gap-1 rounded bg-slate-800/80 px-1.5 py-0.5 text-[10px] text-slate-300"
                        >
                          <span className="h-1 w-1 rounded-full bg-emerald-400" />
                          {display}
                        </span>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Tags */}
              {parsed.tags && parsed.tags.length > 0 && (
                <div>
                  <span className="mb-1 block text-xs text-slate-500">
                    Tags
                  </span>
                  <div className="flex flex-wrap gap-1.5">
                    {parsed.tags.map((tag, i) => (
                      <span
                        key={i}
                        className="inline-flex items-center gap-1 rounded-md bg-slate-800 px-2 py-1 font-mono text-[10px] text-slate-200"
                      >
                        <span className="font-semibold text-violet-400">
                          {tag[0]}
                        </span>
                        {tag.slice(1).map((v, j) => (
                          <span key={j} className="max-w-[200px] truncate">
                            {v}
                          </span>
                        ))}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {/* Signature */}
              {parsed.sig && (
                <div className="min-w-0">
                  <span className="mb-0.5 block text-xs text-slate-500">
                    Signature
                  </span>
                  <CopyableField value={parsed.sig} />
                </div>
              )}

              {/* Content */}
              {contentPretty && (
                <details>
                  <summary className="cursor-pointer text-xs text-slate-500 hover:text-slate-300">
                    Content{" "}
                    <span className="text-slate-600">-- click to expand</span>
                  </summary>
                  <pre className="mt-1 max-h-48 overflow-auto rounded-lg bg-slate-800 p-3 font-mono text-xs text-slate-200 leading-relaxed">
                    {contentPretty}
                  </pre>
                </details>
              )}
            </>
          ) : (
            <pre className="max-h-96 overflow-auto rounded-lg bg-slate-800 p-3 font-mono text-xs text-slate-200 leading-relaxed">
              {nostrEventJson}
            </pre>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 border-t border-slate-800 px-6 py-4">
          {nostrEventNevent && (
            <button
              type="button"
              onClick={handleCopyNevent}
              className="flex items-center gap-1.5 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-slate-800"
            >
              <svg
                aria-hidden="true"
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
                <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
              </svg>
              Copy Event ID
            </button>
          )}
          <button
            type="button"
            onClick={handleCopyJson}
            className="flex items-center gap-1.5 rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-slate-800"
          >
            <svg
              aria-hidden="true"
              xmlns="http://www.w3.org/2000/svg"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
              <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
            </svg>
            Copy JSON
          </button>
          <button
            type="button"
            onClick={handleClose}
            className="rounded-lg bg-slate-800 px-4 py-1.5 text-sm text-slate-200 hover:bg-slate-700"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
