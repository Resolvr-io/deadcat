import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { type ReactNode, useCallback } from "react";
import { useStore } from "../../store";
import type { IdentityResponse, NostrProfile } from "../../types";
import { generateAvatarDataUri } from "../../utils-react/avatar";
import { showToast } from "../shared/Toast";

const ADJECTIVES = [
  "sleepy", "sneaky", "fuzzy", "lazy", "curious", "shadow", "midnight",
  "pouncy", "fluffy", "stealthy", "prowling", "purring", "crafty", "nimble",
  "slinky", "feisty", "rogue", "silent", "velvet", "ghostly", "feral",
  "savage", "golden", "silver", "copper", "ember", "frost", "storm",
  "lunar", "solar", "misty", "dusky", "smoky", "inky", "ashen",
  "crimson", "scarlet", "onyx", "scruffy", "rusty", "mossy", "sandy",
  "wild", "lone", "swift", "bold", "keen", "sly", "wily",
];
const NOUNS = [
  "cat", "kitten", "tabby", "calico", "panther", "lynx", "tomcat",
  "mouser", "paws", "whiskers", "chonk", "neko", "lion", "tiger",
  "jaguar", "leopard", "cheetah", "cougar", "puma", "bobcat", "ocelot",
  "serval", "caracal", "margay", "manul", "wildcat", "civet", "genet",
  "sphinx", "bengal", "maine", "siamese", "persian", "ragdoll", "abyssinian",
  "birman", "burmese", "korat", "chartreux", "savannah", "toyger", "ocicat",
  "claw", "fang", "stripe", "prowler", "stalker", "hunter", "shadow",
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

  // ── Profile continue (generate flow) ────────────────────────────────
  const handleProfileContinue = useCallback(async () => {
    const s = useStore.getState();
    useStore.setState({
      nostrPubkey: s.onboardingPendingPubkey,
      nostrNpub: s.onboardingPendingNpub,
      nostrProfile: {
        name: s.onboardingNostrDisplayName || undefined,
        display_name: s.onboardingNostrDisplayName || undefined,
        picture: s.onboardingProfilePhotoDataUrl || undefined,
      },
      onboardingPendingPubkey: "",
      onboardingPendingNpub: "",
      onboardingProfileStep: false,
      onboardingNostrDisplayName: "",
      onboardingProfilePhotoDataUrl: "",
      onboardingKeysOpen: false,
      onboardingNsecRevealed: false,
      onboardingNostrGeneratedNsec: "",
      setupModalOpen: false,
      onboardingError: "",
    });
    // Publish kind 0 (fire-and-forget)
    void invoke("publish_nostr_profile", {
      name: s.onboardingNostrDisplayName,
      picture: s.onboardingProfilePhotoDataUrl || null,
    }).catch(() => {});
  }, []);

  // ── Photo picker ───────────────────────────────────────────────────
  const handlePickPhoto = useCallback(async () => {
    try {
      const file = await open({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
      });
      if (!file) return;
      const bytes = await readFile(file);
      const ext = file.split(".").pop()?.toLowerCase() ?? "png";
      const mime = ext === "jpg" || ext === "jpeg" ? "image/jpeg" : ext === "webp" ? "image/webp" : ext === "gif" ? "image/gif" : "image/png";
      let binary = "";
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
      }
      const b64 = btoa(binary);
      useStore.setState({ onboardingProfilePhotoDataUrl: `data:${mime};base64,${b64}` });
    } catch {
      // User cancelled or error — ignore
    }
  }, []);

  const npubDisplay = pendingNpub || nostrNpub || "";

  // ── Profile setup screen (generate flow) ────────────────────────────
  if (profileStep && !nostrDone) {
    const avatarSrc = profilePhoto || generateAvatarDataUri(pendingNpub || displayName || "default");

    return (
      <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        <p className="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">
          Identity created
        </p>
        <h2 className="text-2xl font-semibold text-white">
          Set your username
        </h2>
        <p className="mt-3 text-sm text-slate-400 leading-relaxed">
          This is how others will see you on Nostr. You can change it anytime
          from your profile.
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
          <input
            id="onboarding-display-name"
            type="text"
            maxLength={50}
            value={displayName}
            onChange={(e) =>
              useStore.setState({ onboardingNostrDisplayName: e.target.value })
            }
            className="mt-1.5 h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2"
          />
        </div>

        {/* Keys accordion */}
        <div className="mt-5 rounded-xl border border-slate-800 overflow-hidden">
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

        {errorHtml && <div className="mt-4">{errorHtml}</div>}

        <button
          type="button"
          onClick={handleProfileContinue}
          className="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition"
        >
          Get started
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
                <p className="mt-0.5 text-[11px] text-slate-600 mono truncate">
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

  // ── Main nostr step ─────────────────────────────────────────────────
  return (
    <div className="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
      {stepIndicator}
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">
        Step 1 of 2
      </p>
      <h2 className="text-2xl font-semibold text-white">
        Set up your identity
      </h2>
      <p className="mt-3 text-sm text-slate-400 leading-relaxed">
        deadcat uses Nostr keypairs to publish markets and sign attestations.
        Generate a new identity or bring your own.
      </p>
      {errorHtml && <div className="mt-5">{errorHtml}</div>}
      <div className="mt-10 space-y-3">
        <button
          type="button"
          onClick={handleGenerate}
          disabled={loading}
          className="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 transition"
        >
          {loading ? "Generating..." : "Generate new identity"}
        </button>
        <button
          type="button"
          onClick={() =>
            useStore.setState({
              onboardingNostrMode: "import",
              onboardingError: "",
            })
          }
          className="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition"
        >
          Import existing identity
        </button>
      </div>
    </div>
  );
}
