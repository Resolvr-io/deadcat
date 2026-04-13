import { useCallback } from "react";
import { useStore } from "../../store";

export function UserMenu() {
  const userMenuOpen = useStore((s) => s.userMenuOpen);
  const nostrNpub = useStore((s) => s.nostrNpub);

  const toggleMenu = useCallback(() => {
    useStore.setState((s) => ({ userMenuOpen: !s.userMenuOpen }));
  }, []);

  const copyNpub = useCallback(async () => {
    if (nostrNpub) {
      await navigator.clipboard.writeText(nostrNpub);
    }
  }, [nostrNpub]);

  const openSettings = useCallback(() => {
    useStore.setState({ userMenuOpen: false, settingsOpen: true });
  }, []);

  const openLogout = useCallback(() => {
    useStore.setState({ userMenuOpen: false, logoutOpen: true });
  }, []);

  const nostrProfile = useStore((s) => s.nostrProfile);
  const profilePicError = useStore((s) => s.profilePicError);

  return (
    <div className="relative shrink-0">
      <button
        onClick={toggleMenu}
        className="flex h-9 w-9 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 overflow-hidden"
      >
        {nostrProfile?.picture && !profilePicError ? (
          <>
            <img
              src={nostrProfile.picture}
              className="h-full w-full rounded-full object-cover"
              onError={() => useStore.setState({ profilePicError: true })}
            />
          </>
        ) : (
          <svg
            className="h-[18px] w-[18px]"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
            <circle cx="12" cy="7" r="4" />
          </svg>
        )}
      </button>

      {userMenuOpen && nostrNpub && (
        <div className="absolute right-0 top-full z-50 mt-2 w-64 rounded-xl border border-slate-700 bg-slate-900 shadow-xl">
          <div className="px-3 pb-1 pt-3">
            <div className="mb-1.5 text-[11px] text-slate-500">
              Nostr Publishing ID
            </div>
            <button
              onClick={copyNpub}
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-slate-800"
              title="Click to copy npub"
            >
              <span className="mono min-w-0 truncate text-xs text-slate-300">
                {nostrNpub}
              </span>
              <svg
                className="h-3.5 w-3.5 shrink-0 text-slate-500"
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

          <div className="mt-1 border-t border-slate-800 py-1">
            <button
              onClick={openSettings}
              className="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-slate-300 hover:bg-slate-800 hover:text-slate-100"
            >
              <svg
                className="h-4 w-4"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
              Settings
            </button>
          </div>

          <div className="border-t border-slate-800 py-1">
            <button
              onClick={openLogout}
              className="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-slate-300 hover:bg-slate-800 hover:text-slate-100"
            >
              <svg
                className="h-4 w-4"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                <polyline points="16 17 21 12 16 7" />
                <line x1="21" y1="12" x2="9" y2="12" />
              </svg>
              Log out
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
