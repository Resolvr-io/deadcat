import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile, readTextFile } from "@tauri-apps/plugin-fs";
import { type ReactNode, useCallback, useEffect, useState } from "react";
import { useStore } from "../../store";
import type { IdentityResponse, NostrProfile } from "../../types";
import { generateAvatarDataUri } from "../../utils-react/avatar";
import { showToast } from "../shared/Toast";

const ADJECTIVES = [
  "sleepy",
  "sneaky",
  "fuzzy",
  "lazy",
  "curious",
  "shadow",
  "midnight",
  "pouncy",
  "fluffy",
  "stealthy",
  "prowling",
  "purring",
  "crafty",
  "nimble",
  "slinky",
  "feisty",
  "rogue",
  "silent",
  "velvet",
  "ghostly",
  "feral",
  "savage",
  "golden",
  "silver",
  "copper",
  "ember",
  "frost",
  "storm",
  "lunar",
  "solar",
  "misty",
  "dusky",
  "smoky",
  "inky",
  "ashen",
  "crimson",
  "scarlet",
  "onyx",
  "scruffy",
  "rusty",
  "mossy",
  "sandy",
  "wild",
  "lone",
  "swift",
  "bold",
  "keen",
  "sly",
  "wily",
];
const NOUNS = [
  "cat",
  "kitten",
  "tabby",
  "calico",
  "panther",
  "lynx",
  "tomcat",
  "mouser",
  "paws",
  "whiskers",
  "chonk",
  "neko",
  "lion",
  "tiger",
  "jaguar",
  "leopard",
  "cheetah",
  "cougar",
  "puma",
  "bobcat",
  "ocelot",
  "serval",
  "caracal",
  "margay",
  "manul",
  "wildcat",
  "civet",
  "genet",
  "sphinx",
  "bengal",
  "maine",
  "siamese",
  "persian",
  "ragdoll",
  "abyssinian",
  "birman",
  "burmese",
  "korat",
  "chartreux",
  "savannah",
  "toyger",
  "ocicat",
  "claw",
  "fang",
  "stripe",
  "prowler",
  "stalker",
  "hunter",
  "shadow",
];

function randomName(): string {
  const adj = ADJECTIVES[Math.floor(Math.random() * ADJECTIVES.length)];
  const noun = NOUNS[Math.floor(Math.random() * NOUNS.length)];
  return `${adj}-${noun}`;
}

function BackButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mb-8 flex items-center gap-1.5 text-xs text-slate-500 hover:text-slate-300 transition"
    >
      <svg
        className="h-3.5 w-3.5"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={2}
        aria-hidden="true"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M15 19l-7-7 7-7"
        />
      </svg>
      Back
    </button>
  );
}

interface NostrSetupStepProps {
  stepIndicator: ReactNode;
}

export default function NostrSetupStep({ stepIndicator }: NostrSetupStepProps) {
  const [showPassword, setShowPassword] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const nostrMode = useStore((s) => s.onboardingNostrMode);
  const nostrDone = useStore((s) => s.onboardingNostrDone);
  const loading = useStore((s) => s.onboardingLoading);
  const error = useStore((s) => s.onboardingError);
  const pendingNpub = useStore((s) => s.onboardingPendingNpub);
  const nostrNpub = useStore((s) => s.nostrNpub);
  const generatedNsec = useStore((s) => s.onboardingNostrGeneratedNsec);
  const nsecRevealed = useStore((s) => s.onboardingNsecRevealed);
  const nsecAcknowledged = useStore((s) => s.onboardingNsecAcknowledged);
  const nostrProfile = useStore((s) => s.nostrProfile);
  const profilePicError = useStore((s) => s.profilePicError);
  const onboardingNostrNsec = useStore((s) => s.onboardingNostrNsec);
  const profileStep = useStore((s) => s.onboardingProfileStep);
  const displayName = useStore((s) => s.onboardingNostrDisplayName);
  const profilePhoto = useStore((s) => s.onboardingProfilePhotoDataUrl);
  const keysOpen = useStore((s) => s.onboardingKeysOpen);
  const walletPassword = useStore((s) => s.onboardingWalletPassword);
  const walletPasswordConfirm = useStore(
    (s) => s.onboardingWalletPasswordConfirm,
  );
  const backupFileContent = useStore((s) => s.onboardingBackupFileContent);
  const restoreFileContent = useStore((s) => s.onboardingRestoreFileContent);
  const restorePassword = useStore((s) => s.onboardingRestorePassword);
  const restoreMnemonic = useStore((s) => s.onboardingRestoreMnemonic);
  const restoreNsec = useStore((s) => s.onboardingRestoreNsec);

  // Tauri native file drop listener for restore flow
  useEffect(() => {
    if (nostrMode !== "restore") return;
    let unlisten: (() => void) | null = null;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter") {
          setDragOver(true);
        } else if (event.payload.type === "leave") {
          setDragOver(false);
        } else if (event.payload.type === "drop") {
          setDragOver(false);
          const path = event.payload.paths[0];
          if (path) {
            readTextFile(path)
              .then((content) => {
                useStore.setState({ onboardingRestoreFileContent: content });
              })
              .catch(() => {});
          }
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => {
      unlisten?.();
    };
  }, [nostrMode]);

  const errorHtml = error ? (
    <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3">
      <p className="text-sm text-red-400">{error}</p>
    </div>
  ) : null;

  // ── Back handler ────────────────────────────────────────────────────
  const handleBack = useCallback(() => {
    if (nostrDone) {
      // Back from confirmation → main nostr page, discard identity
      useStore.setState({
        onboardingNostrMode: "generate",
        onboardingNostrDone: false,
        onboardingNostrGeneratedNsec: "",
        onboardingNsecRevealed: false,
        onboardingNsecAcknowledged: false,
        onboardingPendingPubkey: "",
        onboardingPendingNpub: "",
        onboardingError: "",
      });
      void invoke("delete_nostr_identity").catch(() => {});
    } else if (nostrMode === "import") {
      // Back from import → main nostr page
      useStore.setState({
        onboardingNostrMode: "generate",
        onboardingNostrDone: false,
        onboardingNostrGeneratedNsec: "",
        onboardingNsecRevealed: false,
        onboardingNsecAcknowledged: false,
        onboardingPendingPubkey: "",
        onboardingPendingNpub: "",
        onboardingError: "",
      });
      void invoke("delete_nostr_identity").catch(() => {});
    }
  }, [nostrDone, nostrMode]);

  // ── Generate handler ────────────────────────────────────────────────
  const handleGenerate = useCallback(async () => {
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      const identity = await invoke<IdentityResponse>(
        "generate_nostr_identity",
      );
      const nsec = await invoke<string>("export_nostr_nsec");
      useStore.setState({
        onboardingPendingPubkey: identity.pubkey_hex,
        onboardingPendingNpub: identity.npub,
        onboardingNostrGeneratedNsec: nsec,
        onboardingProfileStep: true,
        onboardingNostrDisplayName: randomName(),
      });
    } catch (e) {
      useStore.setState({ onboardingError: String(e) });
    }
    useStore.setState({ onboardingLoading: false });
  }, []);

  // ── Import handler ──────────────────────────────────────────────────
  const handleImport = useCallback(async () => {
    const nsecInput = onboardingNostrNsec.trim();
    if (!nsecInput) {
      useStore.setState({ onboardingError: "Paste an nsec to import." });
      return;
    }
    if (!nsecInput.startsWith("nsec1")) {
      useStore.setState({
        onboardingError: "Invalid secret key. It should start with nsec1.",
      });
      return;
    }
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      const identity = await invoke<IdentityResponse>("import_nostr_nsec", {
        nsec: nsecInput,
      });
      useStore.setState({
        onboardingPendingPubkey: identity.pubkey_hex,
        onboardingPendingNpub: identity.npub,
        onboardingNostrDone: true,
        onboardingLoading: false,
      });
      // Fetch profile in background
      invoke<NostrProfile | null>("fetch_nostr_profile")
        .then((profile) => {
          if (profile) {
            useStore.setState({ nostrProfile: profile });
          }
        })
        .catch(() => {});
      return;
    } catch (e) {
      useStore.setState({ onboardingError: String(e) });
    }
    useStore.setState({ onboardingLoading: false });
  }, [onboardingNostrNsec]);

  // ── Continue to wallet handler ──────────────────────────────────────
  const handleContinue = useCallback(async () => {
    const state = useStore.getState();
    useStore.setState({
      nostrPubkey: state.onboardingPendingPubkey || state.nostrPubkey,
      nostrNpub: state.onboardingPendingNpub || state.nostrNpub,
      onboardingPendingPubkey: "",
      onboardingPendingNpub: "",
      onboardingNsecAcknowledged: false,
      onboardingNsecRevealed: false,
      onboardingStep: "wallet",
      onboardingWalletOnly: false,
      onboardingNostrNsec: "",
      onboardingError: "",
    });
    if (state.onboardingNostrMode === "import") {
      // Scan for backups when importing an existing identity
      useStore.setState({ onboardingBackupScanning: true });
      try {
        const status =
          await invoke<import("../../types").NostrBackupStatus>(
            "check_nostr_backup",
          );
        useStore.setState({ nostrBackupStatus: status });
        if (status.has_backup) {
          useStore.setState({
            onboardingBackupFound: true,
            onboardingWalletMode: "nostr-restore",
          });
          if (status.wallets.length > 0) {
            useStore.setState({
              onboardingSelectedWalletDTag: status.wallets[0].d_tag,
            });
          }
        }
      } catch {
        /* scan failed silently */
      }
      useStore.setState({ onboardingBackupScanning: false });
    }
  }, []);

  // ── Copy npub ───────────────────────────────────────────────────────
  const handleCopyNpub = useCallback(() => {
    const npub = pendingNpub || nostrNpub || "";
    if (npub) {
      void navigator.clipboard.writeText(npub);
      showToast("Copied npub to clipboard");
    }
  }, [pendingNpub, nostrNpub]);

  // ── Reveal nsec ─────────────────────────────────────────────────────
  const handleRevealNsec = useCallback(() => {
    useStore.setState({ onboardingNsecRevealed: true });
  }, []);

  // ── Copy nsec ───────────────────────────────────────────────────────
  const handleCopyNsec = useCallback(() => {
    if (generatedNsec) {
      void navigator.clipboard.writeText(generatedNsec);
      showToast("Copied nsec to clipboard");
    }
  }, [generatedNsec]);

  // ── Acknowledge checkbox ────────────────────────────────────────────
  const handleAcknowledge = useCallback(() => {
    useStore.setState((s) => ({
      onboardingNsecAcknowledged: !s.onboardingNsecAcknowledged,
    }));
  }, []);

  // ── Create account (generate flow) — nostr + wallet in one shot ─────
  const handleCreateAccount = useCallback(async () => {
    const s = useStore.getState();
    const password = s.onboardingWalletPassword;
    useStore.setState({ onboardingLoading: true, onboardingError: "" });
    try {
      // 1. Commit identity
      useStore.setState({
        nostrPubkey: s.onboardingPendingPubkey,
        nostrNpub: s.onboardingPendingNpub,
        nostrProfile: {
          name: s.onboardingNostrDisplayName || undefined,
          display_name: s.onboardingNostrDisplayName || undefined,
          picture: s.onboardingProfilePhotoDataUrl || undefined,
        },
      });
      // 2. Create wallet
      const mnemonic = await invoke<string>("create_wallet", { password });
      // 3. Init nostr node + unlock wallet
      await invoke("init_nostr_identity");
      await invoke<void>("unlock_wallet", { password });
      useStore.setState({ walletSessionPassword: password });
      // 4. Publish kind 0 — wait briefly for relay connections then publish
      try {
        await new Promise((r) => setTimeout(r, 1000));
        await invoke("publish_nostr_profile", {
          name: s.onboardingNostrDisplayName,
          picture: s.onboardingProfilePhotoDataUrl || null,
        });
      } catch (e) {
        console.warn("Failed to publish profile:", e);
      }
      // 5. Generate backup file and show download screen
      const nsec = s.onboardingNostrGeneratedNsec;
      const displayNameVal = s.onboardingNostrDisplayName || "unnamed";
      let backupContent = "";
      try {
        backupContent = await invoke<string>("export_identity_file", {
          password,
          nsec,
          mnemonic,
          displayName: displayNameVal,
        });
      } catch {
        // If export fails, still allow proceeding
      }
      useStore.setState({
        walletStatus: "unlocked",
        onboardingLoading: false,
        onboardingProfileStep: false,
        onboardingBackupDownloaded: false,
        onboardingBackupFileContent: backupContent,
        // Keep these for the backup screen display name
        onboardingWalletPassword: "",
        onboardingWalletPasswordConfirm: "",
      });
    } catch (e) {
      useStore.setState({
        onboardingError: String(e),
        onboardingLoading: false,
      });
    }
  }, []);

  // ── Photo picker ───────────────────────────────────────────────────
  const handlePickPhoto = useCallback(async () => {
    try {
      const file = await open({
        multiple: false,
        filters: [
          { name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] },
        ],
      });
      if (!file) return;
      const bytes = await readFile(file);
      const ext = file.split(".").pop()?.toLowerCase() ?? "png";
      const mime =
        ext === "jpg" || ext === "jpeg"
          ? "image/jpeg"
          : ext === "webp"
            ? "image/webp"
            : ext === "gif"
              ? "image/gif"
              : "image/png";
      let binary = "";
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
      }
      const b64 = btoa(binary);
      useStore.setState({
        onboardingProfilePhotoDataUrl: `data:${mime};base64,${b64}`,
      });
    } catch {
      // User cancelled or error — ignore
    }
  }, []);

  const npubDisplay = pendingNpub || nostrNpub || "";

  // ── Backup download screen (after account creation) ─────────────────
  if (backupFileContent) {
    const currentDisplayName =
      displayName ||
      useStore.getState().nostrProfile?.display_name ||
      "unnamed";
    const handleDownloadAndFinish = () => {
      const blob = new Blob([backupFileContent], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `deadcat-${currentDisplayName}.dcid`;
      a.click();
      URL.revokeObjectURL(url);
      useStore.setState({
        onboardingBackupFileContent: "",
        onboardingBackupDownloaded: false,
        onboardingPendingPubkey: "",
        onboardingPendingNpub: "",
        onboardingNostrDisplayName: "",
        onboardingProfilePhotoDataUrl: "",
        onboardingKeysOpen: false,
        onboardingNsecRevealed: false,
        onboardingNostrGeneratedNsec: "",
        setupModalOpen: false,
      });
    };

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        <div className="flex justify-center mb-6">
          <div className="flex h-16 w-16 items-center justify-center rounded-full border border-emerald-500/30 bg-emerald-500/10">
            <svg
              aria-hidden="true"
              className="h-8 w-8 text-emerald-400"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="1.5"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
              />
            </svg>
          </div>
        </div>
        <h2 className="text-2xl font-semibold text-white text-center">
          Account created
        </h2>
        <p className="mt-3 text-sm text-slate-400 text-center leading-relaxed">
          Download your encrypted backup file. You can also download it later
          from Settings.
        </p>
        <div className="mt-6 rounded-lg border border-amber-700/30 bg-amber-950/20 px-4 py-3">
          <p className="text-xs text-amber-300/90 leading-relaxed">
            You&apos;ll need this file and your password to restore your account
            on a new device.
          </p>
        </div>
        <button
          type="button"
          onClick={handleDownloadAndFinish}
          className="mt-6 flex w-full items-center justify-center gap-2 rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
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
              d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
            />
          </svg>
          Download &amp; continue
        </button>
      </div>
    );
  }

  // ── Profile setup screen (generate flow) ────────────────────────────
  if (profileStep && !nostrDone) {
    const avatarSrc =
      profilePhoto ||
      generateAvatarDataUri(displayName || pendingNpub || "default");

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        <h2 className="text-2xl font-semibold text-white">
          Create your account
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Choose a name, set a password, and you&apos;re in.
        </p>

        {/* Avatar */}
        <div className="mt-8 flex justify-center">
          <div className="relative">
            <img
              src={avatarSrc}
              alt="Avatar"
              className="h-24 w-24 rounded-full object-cover"
            />
            <button
              type="button"
              onClick={handlePickPhoto}
              className="absolute bottom-0 right-0 flex h-8 w-8 items-center justify-center rounded-full border border-slate-700 bg-slate-900 text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
              title="Change photo"
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
                  d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931z"
                />
              </svg>
            </button>
          </div>
        </div>

        {/* Username input */}
        <div className="mt-6">
          <label
            htmlFor="onboarding-display-name"
            className="text-xs font-medium text-slate-400 uppercase tracking-wide"
          >
            Display Name
          </label>
          <div className="mt-1.5 flex gap-2">
            <input
              id="onboarding-display-name"
              type="text"
              maxLength={50}
              value={displayName}
              onChange={(e) =>
                useStore.setState({
                  onboardingNostrDisplayName: e.target.value,
                })
              }
              className="h-11 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
            />
            <button
              type="button"
              onClick={() =>
                useStore.setState({
                  onboardingNostrDisplayName: randomName(),
                })
              }
              className="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200"
              title="Randomize"
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
                  d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"
                />
              </svg>
            </button>
          </div>
        </div>

        {/* Keys accordion — hidden at signup, available in settings later */}
        <div className="hidden">
          <button
            type="button"
            onClick={() =>
              useStore.setState((s) => ({
                onboardingKeysOpen: !s.onboardingKeysOpen,
                ...(s.onboardingKeysOpen
                  ? { onboardingNsecRevealed: false }
                  : {}),
              }))
            }
            className="w-full flex items-center justify-between px-4 py-3.5 text-left hover:bg-slate-900/50 transition"
          >
            <div className="flex items-center gap-2">
              <svg
                aria-hidden="true"
                className="h-3.5 w-3.5 text-slate-500"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z"
                />
              </svg>
              <span className="text-xs font-semibold text-slate-400">
                Your keys
              </span>
            </div>
            <svg
              aria-hidden="true"
              className={`h-4 w-4 text-slate-500 transition-transform ${keysOpen ? "rotate-180" : ""}`}
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M19 9l-7 7-7-7"
              />
            </svg>
          </button>
          {keysOpen && (
            <div className="border-t border-slate-800 px-4 py-3 space-y-3">
              <div>
                <p className="text-[10px] font-medium uppercase tracking-wide text-slate-500 mb-1">
                  npub &mdash; public key
                </p>
                <div className="flex items-start justify-between gap-2">
                  <p className="break-all mono text-xs text-slate-400">
                    {npubDisplay}
                  </p>
                  <button
                    type="button"
                    onClick={handleCopyNpub}
                    className="shrink-0 rounded border border-slate-700 px-2 py-1 text-[10px] text-slate-300 hover:bg-slate-800 transition"
                  >
                    Copy
                  </button>
                </div>
              </div>
              <div>
                <p className="text-[10px] font-medium uppercase tracking-wide text-slate-500 mb-1">
                  nsec &mdash; secret key
                </p>
                {nsecRevealed ? (
                  <>
                    <div className="flex items-start justify-between gap-2">
                      <p className="break-all mono text-xs text-rose-300">
                        {generatedNsec}
                      </p>
                      <button
                        type="button"
                        onClick={handleCopyNsec}
                        className="shrink-0 rounded border border-slate-700 px-2 py-1 text-[10px] text-slate-300 hover:bg-slate-800 transition"
                      >
                        Copy
                      </button>
                    </div>
                    <p className="mt-2 text-[10px] text-amber-300/80">
                      Never share your nsec with anyone.
                    </p>
                  </>
                ) : (
                  <div className="flex items-center justify-between gap-2">
                    <p className="text-xs text-slate-500 italic">
                      Hidden for your protection
                    </p>
                    <button
                      type="button"
                      onClick={handleRevealNsec}
                      className="shrink-0 rounded border border-amber-700/60 bg-amber-950/20 px-2 py-1 text-[10px] text-amber-300 hover:bg-amber-900/30 transition"
                    >
                      Reveal
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Password */}
        <div className="mt-5">
          <label
            htmlFor="onboarding-password"
            className="text-xs font-medium text-slate-400 uppercase tracking-wide"
          >
            Password
          </label>
          <div className="relative mt-1.5">
            <input
              id="onboarding-password"
              type={showPassword ? "text" : "password"}
              minLength={4}
              maxLength={32}
              value={walletPassword}
              onChange={(e) =>
                useStore.setState({ onboardingWalletPassword: e.target.value })
              }
              placeholder="Enter a password"
              autoComplete="new-password"
              className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 pr-11 text-sm outline-none ring-emerald-400 transition focus:ring-2"
              disabled={loading}
            />
            <button
              type="button"
              onClick={() => setShowPassword((v) => !v)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition"
              tabIndex={-1}
            >
              {showPassword ? (
                <svg
                  aria-hidden="true"
                  className="h-5 w-5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M3.98 8.223A10.477 10.477 0 001.934 12C3.226 16.338 7.244 19.5 12 19.5c.993 0 1.953-.138 2.863-.395M6.228 6.228A10.45 10.45 0 0112 4.5c4.756 0 8.773 3.162 10.065 7.498a10.523 10.523 0 01-4.293 5.774M6.228 6.228L3 3m3.228 3.228l3.65 3.65m7.894 7.894L21 21m-3.228-3.228l-3.65-3.65m0 0a3 3 0 10-4.243-4.243m4.242 4.242L9.88 9.88"
                  />
                </svg>
              ) : (
                <svg
                  aria-hidden="true"
                  className="h-5 w-5"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M2.036 12.322a1.012 1.012 0 010-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178z"
                  />
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                  />
                </svg>
              )}
            </button>
          </div>
          <div className="relative mt-2">
            <input
              id="onboarding-password-confirm"
              type={showPassword ? "text" : "password"}
              maxLength={32}
              value={walletPasswordConfirm}
              onChange={(e) =>
                useStore.setState({
                  onboardingWalletPasswordConfirm: e.target.value,
                })
              }
              placeholder="Confirm password"
              autoComplete="new-password"
              className={`h-11 w-full rounded-lg border ${walletPasswordConfirm && walletPassword !== walletPasswordConfirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 pr-11 text-sm outline-none ring-emerald-400 transition focus:ring-2`}
              disabled={loading}
            />
          </div>
          <div className="mt-1.5 h-5">
            {walletPassword && walletPassword.length < 4 ? (
              <p className="text-xs text-amber-300">Minimum 4 characters</p>
            ) : walletPasswordConfirm &&
              walletPassword !== walletPasswordConfirm ? (
              <p className="text-xs text-rose-400">
                Passwords don&apos;t match
              </p>
            ) : null}
          </div>
          <div className="mt-3 flex gap-2 rounded-lg border border-amber-700/30 bg-amber-950/20 px-3 py-2.5">
            <svg
              aria-hidden="true"
              className="h-4 w-4 shrink-0 text-amber-400 mt-0.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"
              />
            </svg>
            <p className="text-xs text-amber-300/90 leading-relaxed">
              You&apos;ll need this password to unlock your wallet and to
              restore from your backup file.
            </p>
          </div>
        </div>

        {errorHtml && <div className="mt-4">{errorHtml}</div>}

        <button
          type="button"
          onClick={handleCreateAccount}
          disabled={
            loading ||
            !walletPassword ||
            walletPassword.length < 4 ||
            walletPassword !== walletPasswordConfirm
          }
          className="mt-5 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loading ? (
            <span className="inline-flex items-center">
              Creating account
              <span className="ml-0.5 inline-flex">
                <span className="loading-dot">.</span>
                <span className="loading-dot">.</span>
                <span className="loading-dot">.</span>
              </span>
            </span>
          ) : (
            "Create Account"
          )}
        </button>
      </div>
    );
  }

  // ── Nostr backup / confirmation screen ──────────────────────────────
  if (nostrDone) {
    const isImport = nostrMode === "import";
    const eyebrow = isImport ? "Identity imported" : "Identity created";
    const title = isImport
      ? "Confirm your identity"
      : "Back up your secret key";
    const description = isImport
      ? "Your Nostr identity has been imported. Confirm your details below before continuing."
      : "Your nsec is the only way to prove ownership of markets you create. Store it somewhere safe \u2014 it cannot be recovered if lost.";

    const truncatedNpub =
      npubDisplay.length > 20
        ? `${npubDisplay.slice(0, 10)}...${npubDisplay.slice(-8)}`
        : npubDisplay;

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <BackButton onClick={handleBack} />
        <p className="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">
          {eyebrow}
        </p>
        <h2 className="text-2xl font-semibold text-white">{title}</h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          {description}
        </p>

        {!isImport && (
          <div className="mt-5 rounded-lg border border-amber-700/30 bg-amber-950/20 px-4 py-3">
            <p className="text-xs text-amber-300/90 leading-relaxed">
              Never share your nsec with anyone. Anyone who has it can act as
              you on Nostr.
            </p>
          </div>
        )}

        <div className="mt-4 border-t border-slate-800 divide-y divide-slate-800">
          {isImport ? (
            <div className="py-4 flex items-center gap-3">
              {nostrProfile?.picture && !profilePicError ? (
                <img
                  src={nostrProfile.picture}
                  alt="Profile"
                  className="h-10 w-10 rounded-full object-cover shrink-0"
                  onError={() => useStore.setState({ profilePicError: true })}
                />
              ) : (
                <div className="h-10 w-10 rounded-full bg-slate-800 flex items-center justify-center shrink-0">
                  <svg
                    className="h-5 w-5 text-slate-500"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    strokeWidth={1.5}
                    aria-hidden="true"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"
                    />
                  </svg>
                </div>
              )}
              <div className="min-w-0">
                <p className="text-sm font-semibold text-white truncate">
                  {nostrProfile?.display_name ||
                    nostrProfile?.name ||
                    "No display name"}
                </p>
                <p className="mt-0.5 text-xs text-slate-600 mono truncate">
                  {truncatedNpub}
                </p>
              </div>
            </div>
          ) : (
            <>
              <div className="py-4 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[10px] font-medium uppercase tracking-wide text-slate-500 mb-1">
                    npub &mdash; public key
                  </p>
                  <p className="mono truncate text-xs text-slate-600">
                    {npubDisplay}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={handleCopyNpub}
                  className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
                >
                  Copy
                </button>
              </div>
              <div className="py-4 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="text-[10px] font-medium uppercase tracking-wide text-slate-500 mb-1">
                    nsec &mdash; secret key
                  </p>
                  {nsecRevealed ? (
                    <p className="mono truncate text-xs text-rose-300">
                      {generatedNsec}
                    </p>
                  ) : (
                    <p className="text-xs text-slate-500 italic">
                      Hidden for your protection
                    </p>
                  )}
                </div>
                {nsecRevealed ? (
                  <button
                    type="button"
                    onClick={handleCopyNsec}
                    className="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition"
                  >
                    Copy
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={handleRevealNsec}
                    className="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition"
                  >
                    Show
                  </button>
                )}
              </div>
            </>
          )}
        </div>

        {!isImport && (
          <label className="mt-4 flex items-start gap-3 cursor-pointer select-none">
            <span
              className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded ${nsecAcknowledged ? "bg-emerald-400" : "border border-slate-600 bg-slate-800"}`}
            >
              {nsecAcknowledged && (
                <svg
                  className="h-3 w-3 text-slate-950"
                  viewBox="0 0 12 12"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2.5}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden="true"
                >
                  <polyline points="2,6 5,9 10,3" />
                </svg>
              )}
            </span>
            <input
              type="checkbox"
              checked={nsecAcknowledged}
              onChange={handleAcknowledge}
              className="sr-only"
            />
            <span className="text-sm text-slate-300 leading-relaxed">
              I have saved my secret key in a safe place
            </span>
          </label>
        )}

        <button
          type="button"
          onClick={handleContinue}
          disabled={!isImport && !nsecAcknowledged}
          className="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-40 disabled:cursor-not-allowed transition"
        >
          Continue to wallet setup
        </button>
      </div>
    );
  }

  // ── Import sub-page ─────────────────────────────────────────────────
  if (nostrMode === "import") {
    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        {stepIndicator}
        <BackButton onClick={handleBack} />
        <h2 className="text-2xl font-semibold text-white">
          Import Nostr identity
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Paste your existing secret key (nsec) to restore your identity and
          access markets you&apos;ve previously created.
        </p>
        {errorHtml && <div className="mt-5">{errorHtml}</div>}
        <div className="mt-8 space-y-4">
          <div className="space-y-1.5">
            <label
              htmlFor="nostr-secret-key"
              className="text-xs font-medium text-slate-400 uppercase tracking-wide"
            >
              Secret Key
            </label>
            <input
              id="nostr-secret-key"
              type="text"
              placeholder="nsec1..."
              autoComplete="off"
              spellCheck={false}
              value={onboardingNostrNsec}
              onChange={(e) =>
                useStore.setState({ onboardingNostrNsec: e.target.value })
              }
              className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono"
            />
          </div>
          <button
            type="button"
            onClick={handleImport}
            disabled={loading}
            className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition"
          >
            {loading ? "Importing..." : "Import & continue"}
          </button>
        </div>
      </div>
    );
  }

  // ── Restore sub-page ────────────────────────────────────────────────
  if (nostrMode === "restore") {
    const hasFile = restoreFileContent.length > 0;
    const isManual = restoreNsec.length > 0 || restoreMnemonic.length > 0;

    const handleFileSelect = async () => {
      try {
        const path = await open({
          multiple: false,
          filters: [{ name: "Deadcat Identity", extensions: ["dcid", "json"] }],
        });
        if (!path) return;
        const content = await readTextFile(path);
        useStore.setState({ onboardingRestoreFileContent: content });
      } catch {
        // User cancelled
      }
    };

    const handleFileRestore = async () => {
      if (!restoreFileContent || !restorePassword) return;
      useStore.setState({ onboardingLoading: true, onboardingError: "" });
      try {
        const payload = await invoke<{
          nsec: string;
          mnemonic: string;
          display_name: string;
        }>("import_identity_file", {
          fileContent: restoreFileContent,
          password: restorePassword,
        });
        // Import nsec
        const identity = await invoke<IdentityResponse>("import_nostr_nsec", {
          nsec: payload.nsec,
        });
        useStore.setState({
          nostrPubkey: identity.pubkey_hex,
          nostrNpub: identity.npub,
        });
        // Restore wallet
        await invoke("restore_wallet", {
          mnemonic: payload.mnemonic,
          password: restorePassword,
        });
        await invoke("init_nostr_identity");
        await invoke<void>("unlock_wallet", { password: restorePassword });
        // Done
        useStore.setState({
          walletStatus: "unlocked",
          walletSessionPassword: restorePassword,
          nostrProfile: {
            name: payload.display_name,
            display_name: payload.display_name,
          },
          onboardingLoading: false,
          setupModalOpen: false,
          onboardingRestoreFileContent: "",
          onboardingRestorePassword: "",
        });
        // Fetch full kind 0 profile from relays after connection
        setTimeout(async () => {
          try {
            const profile = await invoke<
              import("../../types").NostrProfile | null
            >("fetch_nostr_profile");
            if (profile) useStore.setState({ nostrProfile: profile });
          } catch {
            // Best effort
          }
        }, 2000);
      } catch (e) {
        useStore.setState({
          onboardingError: String(e),
          onboardingLoading: false,
        });
      }
    };

    const handleManualRestore = async () => {
      if (!restoreNsec || !restoreMnemonic || !restorePassword) {
        useStore.setState({ onboardingError: "All fields are required." });
        return;
      }
      useStore.setState({ onboardingLoading: true, onboardingError: "" });
      try {
        const identity = await invoke<IdentityResponse>("import_nostr_nsec", {
          nsec: restoreNsec,
        });
        useStore.setState({
          nostrPubkey: identity.pubkey_hex,
          nostrNpub: identity.npub,
        });
        await invoke("restore_wallet", {
          mnemonic: restoreMnemonic,
          password: restorePassword,
        });
        await invoke("init_nostr_identity");
        await invoke<void>("unlock_wallet", { password: restorePassword });
        useStore.setState({
          walletStatus: "unlocked",
          walletSessionPassword: restorePassword,
          onboardingLoading: false,
          setupModalOpen: false,
          onboardingRestoreNsec: "",
          onboardingRestoreMnemonic: "",
          onboardingRestorePassword: "",
        });
        // Fetch kind 0 profile from relays
        setTimeout(async () => {
          try {
            const profile = await invoke<
              import("../../types").NostrProfile | null
            >("fetch_nostr_profile");
            if (profile) useStore.setState({ nostrProfile: profile });
          } catch {
            // Best effort
          }
        }, 2000);
      } catch (e) {
        useStore.setState({
          onboardingError: String(e),
          onboardingLoading: false,
        });
      }
    };

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        <BackButton
          onClick={() =>
            useStore.setState({
              onboardingNostrMode: "generate",
              onboardingError: "",
              onboardingRestoreFileContent: "",
              onboardingRestorePassword: "",
              onboardingRestoreNsec: "",
              onboardingRestoreMnemonic: "",
            })
          }
        />
        <h2 className="text-2xl font-semibold text-white">Restore account</h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          Import your Deadcat identity file, or enter your keys manually.
        </p>
        {errorHtml && <div className="mt-4">{errorHtml}</div>}

        {/* File upload */}
        <div className="mt-6 space-y-4">
          <p className="text-xs font-medium text-slate-400 uppercase tracking-wide">
            Import Deadcat Identity
          </p>
          <div
            className={`flex flex-col items-center justify-center rounded-xl border-2 border-dashed ${hasFile ? "border-emerald-500/50 bg-emerald-500/5" : dragOver ? "border-emerald-400 bg-emerald-500/10" : "border-slate-700"} px-6 py-8 text-center transition`}
          >
            {hasFile ? (
              <p className="text-sm text-emerald-300">File loaded</p>
            ) : (
              <>
                <svg
                  aria-hidden="true"
                  className="h-8 w-8 text-slate-600 mb-2"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  strokeWidth="1.5"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
                  />
                </svg>
                <p className="text-sm text-slate-400">
                  Drop your <span className="text-slate-200">.dcid</span> file
                  here
                </p>
                <button
                  type="button"
                  onClick={handleFileSelect}
                  className="mt-2 text-xs text-emerald-400 hover:text-emerald-300 transition"
                >
                  or click to browse
                </button>
              </>
            )}
          </div>
          {hasFile && (
            <>
              <input
                type="password"
                value={restorePassword}
                onChange={(e) =>
                  useStore.setState({
                    onboardingRestorePassword: e.target.value,
                  })
                }
                placeholder="Enter your password"
                className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
                disabled={loading}
              />
              <button
                type="button"
                onClick={handleFileRestore}
                disabled={loading || !restorePassword}
                className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition"
              >
                {loading ? "Restoring..." : "Restore Account"}
              </button>
            </>
          )}
        </div>

        {/* Manual entry */}
        <div className="mt-8 space-y-4">
          <p className="text-xs font-medium text-slate-400 uppercase tracking-wide">
            Or enter manually
          </p>
          <input
            type="text"
            value={restoreNsec}
            onChange={(e) =>
              useStore.setState({ onboardingRestoreNsec: e.target.value })
            }
            placeholder="nsec1..."
            autoComplete="off"
            spellCheck={false}
            className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm mono outline-none ring-emerald-400 transition focus:ring-2"
            disabled={loading || hasFile}
          />
          <textarea
            value={restoreMnemonic}
            onChange={(e) =>
              useStore.setState({ onboardingRestoreMnemonic: e.target.value })
            }
            placeholder="12-word recovery phrase..."
            rows={3}
            className="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm mono outline-none ring-emerald-400 transition focus:ring-2"
            disabled={loading || hasFile}
          />
          {isManual && !hasFile && (
            <>
              <input
                type="password"
                value={restorePassword}
                onChange={(e) =>
                  useStore.setState({
                    onboardingRestorePassword: e.target.value,
                  })
                }
                placeholder="Set a wallet password"
                className="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
                disabled={loading}
              />
              <button
                type="button"
                onClick={handleManualRestore}
                disabled={loading || !restorePassword}
                className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition"
              >
                {loading ? "Restoring..." : "Restore Account"}
              </button>
            </>
          )}
        </div>
      </div>
    );
  }

  // ── Main nostr step ─────────────────────────────────────────────────
  return (
    <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
      <div className="flex items-start gap-4 mb-6">
        <svg
          aria-hidden="true"
          className="h-10 w-10 shrink-0"
          viewBox="0 0 260 267"
          fill="#34D399"
        >
          <path d="M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.231 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.217 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" />
        </svg>
        <div>
          <h2 className="text-2xl font-semibold text-white">
            Welcome to Deadcat
          </h2>
          <p className="mt-1 text-sm text-slate-400">
            Trade prediction markets on Liquid with self-custody and Nostr
            identity.
          </p>
        </div>
      </div>
      {errorHtml && <div className="mb-5">{errorHtml}</div>}
      <div className="space-y-3">
        <button
          type="button"
          onClick={handleGenerate}
          disabled={loading}
          className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 transition"
        >
          {loading ? "Creating..." : "Create new account"}
        </button>
        <button
          type="button"
          onClick={() =>
            useStore.setState({
              onboardingNostrMode: "restore",
              onboardingError: "",
            })
          }
          className="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition"
        >
          Restore existing account
        </button>
      </div>
    </div>
  );
}
