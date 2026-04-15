import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import { categories } from "../../constants";
import { useStore } from "../../store";
import type { NavCategory } from "../../types";
import { formatCompactSats } from "../../utils-react/wallet";
import { CloseButton } from "../shared/CloseButton";
import { SearchBar } from "./SearchBar";
import { SettingsPanel } from "./SettingsPanel";
import { UserMenu } from "./UserMenu";

/* ── Category icon helper ──────────────────────────────────────────── */

const iconProps = (size: string) =>
  ({
    className: `${size} shrink-0`,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  }) as const;

export function categoryIcon(
  category: string,
  size = "h-3.5 w-3.5",
): React.ReactNode {
  const a = iconProps(size);
  switch (category) {
    case "Trending":
      return (
        <svg aria-hidden="true" {...a}>
          <polyline points="22 7 13.5 15.5 8.5 10.5 2 17" />
          <polyline points="16 7 22 7 22 13" />
        </svg>
      );
    case "Ending Soon":
      return (
        <svg aria-hidden="true" {...a}>
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      );
    case "New":
      return (
        <svg aria-hidden="true" {...a}>
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
      );
    case "Politics":
      return (
        <svg
          aria-hidden="true"
          className={`${size} shrink-0`}
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M20 6h3v2h-1v11h1v2H1v-2h1V8H1V6h3V4a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2zm0 2H4v11h3v-7h2v7h2v-7h2v7h2v-7h2v7h3V8zM6 5v1h12V5H6z" />
        </svg>
      );
    case "Sports":
      return (
        <svg aria-hidden="true" {...a}>
          <path d="M6 9H4a2 2 0 0 1-2-2V4h4" />
          <path d="M18 9h2a2 2 0 0 0 2-2V4h-4" />
          <path d="M4 22h16" />
          <path d="M10 14.66V17c0 .55-.47.98-.97 1.21C7.85 18.75 7 20.24 7 22" />
          <path d="M14 14.66V17c0 .55.47.98.97 1.21C16.15 18.75 17 20.24 17 22" />
          <path d="M18 2H6v7a6 6 0 0 0 12 0V2Z" />
        </svg>
      );
    case "Culture":
      return (
        <svg aria-hidden="true" {...a}>
          <circle cx="12" cy="12" r="10" />
          <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
          <path d="M2 12h20" />
        </svg>
      );
    case "Bitcoin":
      return (
        <svg
          aria-hidden="true"
          className={`${size} shrink-0`}
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M9 2a1 1 0 0 1 2 0v2h2V2a1 1 0 1 1 2 0v2c3.59 0 5.88 2.01 5.88 4.5 0 1.41-.73 2.67-2.02 3.49 1.31.82 2.06 2.08 2.06 3.51 0 2.5-2.3 4.5-5.91 4.5v2a1 1 0 1 1-2 0v-2h-2v2a1 1 0 1 1-2 0v-2H4a1 1 0 1 1 0-2h2V6H4a1 1 0 0 1 0-2h5V2zm6 9c2.91 0 3.88-1.49 3.88-2.5S17.91 6 15 6H8v5h7zM8 13v5h7c2.94 0 3.91-1.5 3.91-2.5S17.94 13 15 13H8z"
          />
        </svg>
      );
    case "Weather":
      return (
        <svg aria-hidden="true" {...a}>
          <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
        </svg>
      );
    case "Macro":
      return (
        <svg aria-hidden="true" {...a}>
          <path d="M3 3v18h18" />
          <path d="m19 9-5 5-4-4-3 3" />
        </svg>
      );
    case "My Markets":
      return (
        <svg aria-hidden="true" {...a}>
          <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
      );
    default:
      return null;
  }
}

/* ── Help modal ────────────────────────────────────────────────────── */

function HelpModal() {
  const helpOpen = useStore((s) => s.helpOpen);
  if (!helpOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div className="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-slate-100">Help</h2>
          <button
            type="button"
            onClick={() => useStore.setState({ helpOpen: false })}
            className="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
          >
            <svg
              aria-hidden="true"
              className="h-5 w-5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>
        <p className="mt-4 text-sm text-slate-400">Help content coming soon.</p>
      </div>
    </div>
  );
}

/* ── Logout modal ──────────────────────────────────────────────────── */

function LogoutModal() {
  const logoutOpen = useStore((s) => s.logoutOpen);
  const logoutBackedUp = useStore((s) => s.logoutBackedUp);
  const logoutBackupError = useStore((s) => s.logoutBackupError);
  const walletSessionPassword = useStore((s) => s.walletSessionPassword);
  const walletStatus = useStore((s) => s.walletStatus);
  const logoutPasswordInput = useStore((s) => s.logoutPasswordInput ?? "");
  const logoutBackupDownloaded = useStore((s) => s.logoutBackupDownloaded);
  const needsPassword = walletStatus === "locked" || !walletSessionPassword;

  const closeLogout = useCallback(() => {
    useStore.setState({
      logoutOpen: false,
      logoutBackedUp: false,
      logoutBackupDownloaded: false,
      logoutBackupError: "",
      logoutPasswordInput: "",
    });
  }, []);

  const confirmLogout = useCallback(async () => {
    // Lock first to ensure clean state, then delete
    try {
      await invoke("lock_wallet");
    } catch (e) {
      console.warn("lock_wallet:", e);
    }
    try {
      await invoke("delete_wallet");
    } catch (e) {
      console.warn("delete_wallet:", e);
    }
    try {
      await invoke("delete_nostr_identity");
    } catch (e) {
      console.warn("delete_nostr_identity:", e);
    }
    useStore.setState({
      // Clear identity
      nostrPubkey: null,
      nostrNpub: null,
      nostrProfile: null,
      nostrNsecRevealed: null,
      nostrImportNsec: "",
      nostrBackupStatus: null,
      nostrBackupPassword: "",
      nostrBackupPrompt: false,
      nostrBackupNsecInput: "",
      nostrBackupNsecPrompt: false,
      // Clear wallet
      walletStatus: "not_created",
      walletData: null,
      walletPassword: "",
      walletPasswordConfirm: "",
      walletMnemonic: "",
      walletRestoreMnemonic: "",
      walletError: "",
      walletOpen: false,
      walletSessionPassword: "",
      // Clear relays
      relays: [],
      relayInput: "",
      // Clear UI
      logoutOpen: false,
      logoutBackedUp: false,
      profilePicError: false,
    });
    localStorage.removeItem("deadcat_tx_labels");
    window.location.reload();
  }, []);

  if (!logoutOpen) return null;

  const hasWallet = walletStatus !== "not_created";

  // Simple logout for identity-only (no wallet)
  if (!hasWallet) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
        <div className="w-full max-w-sm rounded-2xl border border-slate-800 bg-slate-950 p-8">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-medium text-slate-100">Log Out</h2>
            <CloseButton onClick={closeLogout} />
          </div>
          <div className="mt-5 space-y-4">
            <p className="text-sm text-slate-400 leading-relaxed">
              This will remove your Nostr identity from this device. Make sure
              you have your nsec saved if you want to log back in.
            </p>
            <div className="flex gap-3">
              <button
                type="button"
                onClick={closeLogout}
                className="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={confirmLogout}
                className="flex-1 rounded-xl bg-rose-500 py-2.5 text-sm font-medium text-white transition hover:bg-rose-400"
              >
                Log Out
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Full logout with backup flow when wallet exists
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-medium text-slate-100">
            Log Out
          </h2>
          <CloseButton onClick={closeLogout} />
        </div>
        <div className="mt-5 space-y-4">
          <div className="rounded-xl border border-slate-700 bg-slate-900/60 p-4">
            <p className="text-sm text-slate-300 leading-relaxed">
              Logging out will{" "}
              <strong className="text-slate-100">
                permanently remove your Nostr keys and wallet
              </strong>{" "}
              from this device. Download your backup file before logging out so
              you can restore your account later.
            </p>
          </div>
          {logoutBackupDownloaded ? (
            <div className="flex items-center gap-2 rounded-lg border border-emerald-500/20 bg-emerald-500/5 px-4 py-3 text-sm text-emerald-300">
              <svg
                aria-hidden="true"
                className="h-5 w-5 shrink-0"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M5 13l4 4L19 7"
                />
              </svg>
              Backup file downloaded
              <button
                type="button"
                onClick={() => {
                  void invoke("open_downloads_folder").catch(() => {});
                }}
                className="ml-auto shrink-0 text-xs text-slate-400 underline hover:text-slate-200 transition"
              >
                Open folder
              </button>
            </div>
          ) : (
            <>
              {needsPassword && (
                <input
                  type="password"
                  value={logoutPasswordInput}
                  onChange={(e) =>
                    useStore.setState({ logoutPasswordInput: e.target.value })
                  }
                  placeholder="Enter your wallet password"
                  className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
                />
              )}
              {logoutBackupError && (
                <div className="rounded-lg border border-amber-700/30 bg-amber-950/20 px-3 py-2">
                  <p className="text-xs text-amber-300/80">
                    {logoutBackupError}
                  </p>
                </div>
              )}
              <button
                type="button"
                onClick={async () => {
                  try {
                    const nsec = await invoke<string>("export_nostr_nsec");
                    const pw = needsPassword
                      ? useStore.getState().logoutPasswordInput
                      : useStore.getState().walletSessionPassword;
                    if (!pw) {
                      useStore.setState({
                        logoutBackupError:
                          "Enter your wallet password to create the backup.",
                      });
                      return;
                    }
                    let mnemonic: string;
                    try {
                      mnemonic = await invoke<string>("get_cached_mnemonic");
                    } catch {
                      mnemonic = await invoke<string>("get_wallet_mnemonic", {
                        password: pw,
                      });
                    }
                    const name =
                      useStore.getState().nostrProfile?.display_name ||
                      useStore.getState().nostrProfile?.name ||
                      "unnamed";
                    useStore.setState({ logoutBackupError: "" });
                    const fileContent = await invoke<string>(
                      "export_identity_file",
                      { password: pw, nsec, mnemonic, displayName: name },
                    );
                    const blob = new Blob([fileContent], {
                      type: "application/json",
                    });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement("a");
                    a.href = url;
                    a.download = `deadcat-${name}.dcid`;
                    a.click();
                    URL.revokeObjectURL(url);
                    useStore.setState({
                      logoutPasswordInput: "",
                      logoutBackupDownloaded: true,
                    });
                  } catch (e) {
                    useStore.setState({
                      logoutBackupError: `Could not create backup: ${String(e)}`,
                    });
                  }
                }}
                className="flex w-full items-center justify-center gap-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-4 py-3 text-sm font-medium text-emerald-300 transition hover:bg-emerald-500/20"
              >
                <svg
                  aria-hidden="true"
                  className="h-4 w-4"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
                  />
                </svg>
                Download backup file
              </button>
            </>
          )}
          <label className="flex items-start gap-3 rounded-xl border border-slate-800 bg-slate-900/40 p-4 text-sm text-slate-300">
            <input
              type="checkbox"
              checked={logoutBackedUp}
              onChange={(e) =>
                useStore.setState({ logoutBackedUp: e.target.checked })
              }
              className="mt-0.5 h-4 w-4 rounded border-slate-700 bg-slate-950 text-rose-400 focus:ring-rose-400"
            />
            <span>I have downloaded my backup or already have one saved.</span>
          </label>
          <p className="text-xs text-slate-500">
            <strong className="text-slate-300">
              Deadcat Live does not hold user funds.
            </strong>{" "}
            If you lose your backup file and password, your funds cannot be
            recovered.
          </p>
          <div className="flex gap-3">
            <button
              type="button"
              onClick={closeLogout}
              className="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={confirmLogout}
              disabled={!logoutBackedUp}
              className={`flex-1 rounded-xl py-2.5 text-sm font-medium transition ${logoutBackedUp ? "bg-rose-500 text-white hover:bg-rose-400" : "cursor-not-allowed border border-slate-800 text-slate-600"}`}
            >
              Log Out
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ── Category bar ──────────────────────────────────────────────────── */

function CategoryBar() {
  const activeCategory = useStore((s) => s.activeCategory);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const marketMakerMode = useStore((s) => s.marketMakerMode);

  const filteredCategories = categories.filter(
    (category) =>
      category !== "Portfolio" &&
      (category !== "My Markets" || (nostrPubkey && marketMakerMode)),
  );

  return (
    <div className="border-t border-slate-800">
      <div className="phi-container py-2">
        <div className="flex items-center gap-1 overflow-x-auto whitespace-nowrap">
          {filteredCategories.map((category) => {
            const active = activeCategory === category;
            const icon = categoryIcon(category);
            return (
              <button
                type="button"
                key={category}
                onClick={() =>
                  useStore.setState({
                    activeCategory: category as NavCategory,
                    view: "home",
                    selectedMarketId: "",
                  })
                }
                className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm font-normal transition ${
                  active
                    ? "bg-slate-800/80 text-slate-100"
                    : "text-slate-500 hover:text-slate-300"
                }`}
              >
                {icon}
                {category}
              </button>
            );
          })}
          <button
            type="button"
            onClick={() => useStore.setState({ helpOpen: true })}
            className="ml-auto flex shrink-0 items-center gap-1.5 rounded-full px-3 py-1.5 text-sm font-normal text-slate-500 transition hover:text-slate-300"
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
              <path d="M3 11h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-5Zm0 0a9 9 0 1 1 18 0m0 0v5a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3Z" />
              <path d="M21 16v2a4 4 0 0 1-4 4h-5" />
            </svg>
            Help
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── Wallet button in header ───────────────────────────────────────── */

function WalletButton() {
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const walletStatus = useStore((s) => s.walletStatus);
  const walletData = useStore((s) => s.walletData);
  const walletPolicyAssetId = useStore((s) => s.walletPolicyAssetId);
  const walletBalanceHidden = useStore((s) => s.walletBalanceHidden);

  const openWallet = useCallback(() => {
    useStore.setState({ walletOpen: true });
  }, []);

  if (!nostrPubkey) {
    return (
      <button
        type="button"
        onClick={() =>
          useStore.setState({ setupModalOpen: true, onboardingStep: "nostr" })
        }
        className="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-semibold text-slate-950 hover:bg-emerald-300 transition"
      >
        Get started
      </button>
    );
  }

  if (walletStatus === "not_created") {
    return (
      <button
        type="button"
        onClick={openWallet}
        className="shrink-0 px-3 py-1.5 text-sm font-medium text-emerald-400 hover:text-emerald-300 transition"
      >
        Set up wallet
      </button>
    );
  }

  const showBalance =
    walletStatus === "unlocked" && walletData?.balance && !walletBalanceHidden;
  const isLocked = walletStatus === "locked";

  return (
    <button
      type="button"
      onClick={openWallet}
      className={`flex h-9 shrink-0 items-center justify-center rounded-full border transition hover:border-slate-500 hover:text-slate-200 ${isLocked ? "gap-1 border-amber-500/40 px-3 text-slate-400" : showBalance ? "gap-1.5 border-slate-700 px-3 text-slate-400" : "w-9 border-slate-700 text-slate-400"}`}
    >
      <svg
        aria-hidden="true"
        className="h-[18px] w-[18px] shrink-0"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="1" y="4" width="22" height="16" rx="2" ry="2" />
        <line x1="1" y1="10" x2="23" y2="10" />
      </svg>
      {showBalance && (
        <span className="text-xs font-medium text-slate-300">
          {formatCompactSats(walletData?.balance[walletPolicyAssetId] ?? 0)}
        </span>
      )}
      {isLocked && (
        <svg
          aria-hidden="true"
          className="h-3.5 w-3.5 shrink-0 text-amber-400"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0110 0v4" />
        </svg>
      )}
    </button>
  );
}

/* ── Logo SVG ──────────────────────────────────────────────────────── */

function Logo() {
  return (
    <svg
      className="h-10"
      viewBox="0 0 1274 267"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <path
        d="M0.146484 9.04596C0.146625 1.23444 10.9146 -3.15996 16.7881 2.6983L86.5566 71.7335C100.141 68.0293 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0305 173.453 71.7364L243.212 2.71197C249.085 -3.1467 259.853 1.24702 259.854 9.05865V161.26C259.949 162.835 260 164.419 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C4.78859e-06 164.419 0.0506708 162.835 0.146484 161.26V9.04596ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.23 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.216 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z"
        fill="#34D399"
      />
      <path
        d="M861.074 187V124.998H835.164V107.8H906.444V124.998H880.534V187H861.074Z"
        fill="#34D399"
      />
      <path
        d="M756.185 187L788.657 107.8H810.946L842.965 187H821.921L814.68 167.879H783.792L776.437 187H756.185ZM789.675 152.378H808.909L799.405 127.034L789.675 152.378Z"
        fill="#34D399"
      />
      <path
        d="M716.605 188.131C710.571 188.131 704.951 187.113 699.747 185.077C694.618 182.965 690.13 180.061 686.283 176.365C682.436 172.669 679.419 168.369 677.231 163.466C675.119 158.488 674.063 153.133 674.063 147.4C674.063 141.592 675.119 136.237 677.231 131.334C679.419 126.355 682.436 122.018 686.283 118.322C690.205 114.626 694.731 111.76 699.86 109.723C705.065 107.611 710.646 106.555 716.605 106.555C720.98 106.555 725.279 107.197 729.503 108.479C733.727 109.761 737.612 111.571 741.157 113.91C744.778 116.173 747.795 118.888 750.209 122.056L737.084 134.954C734.293 131.409 731.163 128.769 727.693 127.034C724.299 125.299 720.603 124.432 716.605 124.432C713.437 124.432 710.458 125.035 707.667 126.242C704.952 127.374 702.575 128.958 700.539 130.994C698.502 133.031 696.918 135.445 695.787 138.235C694.655 141.026 694.09 144.081 694.09 147.4C694.09 150.643 694.655 153.661 695.787 156.451C696.994 159.167 698.615 161.581 700.652 163.693C702.764 165.729 705.215 167.313 708.006 168.445C710.873 169.576 713.965 170.142 717.284 170.142C721.131 170.142 724.676 169.312 727.919 167.653C731.238 165.993 734.218 163.542 736.858 160.298L749.643 172.857C747.229 175.95 744.25 178.665 740.705 181.003C737.16 183.266 733.313 185.039 729.164 186.321C725.015 187.528 720.829 188.131 716.605 188.131Z"
        fill="#34D399"
      />
      <path
        d="M606.391 169.802H618.61C621.703 169.802 624.569 169.237 627.209 168.105C629.924 166.974 632.3 165.39 634.337 163.353C636.374 161.317 637.958 158.978 639.089 156.338C640.22 153.623 640.786 150.719 640.786 147.626C640.786 144.458 640.22 141.517 639.089 138.801C637.958 136.01 636.374 133.597 634.337 131.56C632.3 129.523 629.924 127.939 627.209 126.808C624.569 125.601 621.703 124.998 618.61 124.998H606.391V169.802ZM586.93 187V107.8H619.063C624.946 107.8 630.415 108.818 635.468 110.855C640.522 112.891 644.935 115.72 648.706 119.341C652.553 122.961 655.532 127.185 657.644 132.013C659.832 136.84 660.926 142.045 660.926 147.626C660.926 153.133 659.832 158.262 657.644 163.014C655.532 167.766 652.553 171.952 648.706 175.573C644.935 179.118 640.522 181.909 635.468 183.945C630.415 185.982 624.946 187 619.063 187H586.93Z"
        fill="#34D399"
      />
      <path
        d="M488.73 187L521.202 107.8H543.491L575.511 187H554.466L547.225 167.879H516.337L508.983 187H488.73ZM522.22 152.378H541.455L531.951 127.034L522.22 152.378Z"
        fill="#34D399"
      />
      <path
        d="M414.962 187V107.8H477.417V124.658H434.422V138.914H462.821V155.207H434.422V170.142H477.869V187H414.962Z"
        fill="#34D399"
      />
      <path
        d="M344.461 169.802H356.68C359.773 169.802 362.639 169.237 365.279 168.105C367.994 166.974 370.37 165.39 372.407 163.353C374.443 161.317 376.027 158.978 377.159 156.338C378.29 153.623 378.856 150.719 378.856 147.626C378.856 144.458 378.29 141.517 377.159 138.801C376.027 136.01 374.443 133.597 372.407 131.56C370.37 129.523 367.994 127.939 365.279 126.808C362.639 125.601 359.773 124.998 356.68 124.998H344.461V169.802ZM325 187V107.8H357.133C363.016 107.8 368.485 108.818 373.538 110.855C378.592 112.891 383.005 115.72 386.776 119.341C390.623 122.961 393.602 127.185 395.714 132.013C397.902 136.84 398.995 142.045 398.995 147.626C398.995 153.133 397.902 158.262 395.714 163.014C393.602 167.766 390.623 171.952 386.776 175.573C383.005 179.118 378.592 181.909 373.538 183.945C368.485 185.982 363.016 187 357.133 187H325Z"
        fill="#34D399"
      />
      <rect
        x="937.564"
        y="67.8096"
        width="336.286"
        height="152.952"
        rx="26.1905"
        fill="#EF4444"
      />
      <path
        d="M1178.21 187V107.8H1240.66V124.658H1197.67V138.914H1226.07V155.207H1197.67V170.142H1241.11V187H1178.21Z"
        fill="white"
      />
      <path
        d="M1112.03 187L1080.01 107.8H1101.05L1123.57 166.747L1146.54 107.8H1166.79L1134.32 187H1112.03Z"
        fill="white"
      />
      <path d="M1049.11 187V107.8H1068.57V187H1049.11Z" fill="white" />
      <path
        d="M971.412 187V107.8H990.873V169.802H1032.62V187H971.412Z"
        fill="white"
      />
    </svg>
  );
}

/* ── Main TopShell component ───────────────────────────────────────── */

export function TopShell() {
  const view = useStore((s) => s.view);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const activeCategory = useStore((s) => s.activeCategory);

  const goHome = useCallback(() => {
    useStore.setState({ view: "home", selectedMarketId: "" });
  }, []);

  return (
    <>
      <header className="relative z-30 border-b border-slate-800 bg-slate-950/80 backdrop-blur">
        <div className="phi-container py-4 lg:py-5">
          <div className="flex items-end gap-5">
            {/* Logo */}
            <button
              type="button"
              onClick={goHome}
              className="shrink-0 py-1 leading-none"
            >
              <Logo />
            </button>

            {/* Nav links */}
            <nav className="flex shrink-0 items-baseline gap-5 whitespace-nowrap pb-[9px] text-base text-slate-400">
              <button
                type="button"
                onClick={goHome}
                className={
                  view === "home" || view === "detail"
                    ? "font-medium text-slate-100"
                    : "hover:text-slate-200"
                }
              >
                Markets
              </button>
              <button type="button" className="hover:text-slate-200">
                Live
              </button>
              <button type="button" className="hover:text-slate-200">
                Social
              </button>
            </nav>

            {/* Right side: search + wallet + user menu */}
            <div className="ml-auto flex shrink-0 items-center gap-2 pb-[5px]">
              <SearchBar />
              {nostrPubkey && (
                <button
                  type="button"
                  onClick={() =>
                    useStore.setState({
                      view: "home",
                      activeCategory: "Portfolio",
                    })
                  }
                  className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 transition ${activeCategory === "Portfolio" ? "border-slate-500 text-slate-100" : "text-slate-400 hover:border-slate-500 hover:text-slate-200"}`}
                  title="Portfolio"
                >
                  <svg
                    aria-hidden="true"
                    className="h-[18px] w-[18px]"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M3 3v18h18" />
                    <rect x="7" y="13" width="9" height="4" rx="1" />
                    <rect x="7" y="5" width="12" height="4" rx="1" />
                  </svg>
                </button>
              )}
              <WalletButton />
              {nostrPubkey && <UserMenu />}
            </div>
          </div>
        </div>

        {/* Category bar */}
        <CategoryBar />
      </header>

      {/* Modals */}
      <HelpModal />
      <SettingsPanel />
      <LogoutModal />
    </>
  );
}
