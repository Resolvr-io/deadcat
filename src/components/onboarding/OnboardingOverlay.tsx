import { useCallback } from "react";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useLockScroll } from "../../hooks/useLockScroll";
import { useStore } from "../../store";
import { CloseButton } from "../shared/CloseButton";
import NostrSetupStep from "./NostrSetupStep";
import WalletSetupStep from "./WalletSetupStep";

export default function OnboardingOverlay() {
  const setupModalOpen = useStore((s) => s.setupModalOpen);
  const onboardingStep = useStore((s) => s.onboardingStep);
  const onboardingNostrDone = useStore((s) => s.onboardingNostrDone);
  const onboardingWalletOnly = useStore((s) => s.onboardingWalletOnly);
  const backupFileContent = useStore((s) => s.onboardingBackupFileContent);
  useLockScroll();

  const handleClose = useCallback(() => {
    useStore.setState({
      setupModalOpen: false,
      setupRequires: null,
      onboardingStep: null,
      onboardingNostrMode: "generate",
      onboardingNostrDone: false,
      onboardingNostrGeneratedNsec: "",
      onboardingNsecRevealed: false,
      onboardingNsecAcknowledged: false,
      onboardingError: "",
      onboardingWalletMode: "create",
      onboardingWalletMnemonic: "",
      onboardingWalletPassword: "",
      onboardingWalletPasswordConfirm: "",
      onboardingWalletPasswordStep: false,
      onboardingMnemonicVerifyStep: false,
      onboardingMnemonicVerifyIndices: [],
      onboardingMnemonicVerifyInputs: [],
      onboardingBackupFound: false,
      onboardingBackupScanning: false,
      onboardingPasswordRevealed: false,
    });
  }, []);

  useEscapeKey(setupModalOpen && !backupFileContent, handleClose);

  if (!setupModalOpen) return null;

  // Step indicator for multi-step onboarding (hidden in wallet-only mode)
  const stepIndicator = onboardingWalletOnly ? null : (
    <div className="flex items-center gap-3 mb-10">
      <div className="flex items-center gap-2.5">
        <div
          className={`h-7 w-7 rounded-full ${
            onboardingStep === "nostr" || onboardingNostrDone
              ? "bg-emerald-400 text-slate-950"
              : "border border-slate-700 text-slate-500"
          } flex items-center justify-center text-xs font-semibold shrink-0`}
        >
          {onboardingNostrDone && onboardingStep !== "nostr" ? (
            <svg
              className="h-3.5 w-3.5"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={3}
              aria-hidden="true"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M5 13l4 4L19 7"
              />
            </svg>
          ) : (
            "1"
          )}
        </div>
        <span
          className={`text-xs font-medium ${onboardingStep === "nostr" ? "text-slate-200" : "text-slate-500"}`}
        >
          Nostr identity
        </span>
      </div>
      <div
        className={`h-px flex-1 ${onboardingStep === "wallet" ? "bg-emerald-400" : "bg-slate-700"}`}
      />
      <div className="flex items-center gap-2.5">
        <div
          className={`h-7 w-7 rounded-full ${
            onboardingStep === "wallet"
              ? "bg-emerald-400 text-slate-950"
              : "border border-slate-700 text-slate-500"
          } flex items-center justify-center text-xs font-semibold shrink-0`}
        >
          2
        </div>
        <span
          className={`text-xs font-medium ${onboardingStep === "wallet" ? "text-slate-200" : "text-slate-500"}`}
        >
          Wallet
        </span>
      </div>
    </div>
  );

  return (
    <div
      role="presentation"
      className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm"
    >
      <div className="relative w-full max-w-[432px] mx-4">
        {!backupFileContent && (
          <CloseButton
            onClick={handleClose}
            variant="overlay"
            className="absolute -top-12 right-0"
          />
        )}
        {onboardingStep === "nostr" ? (
          <NostrSetupStep stepIndicator={stepIndicator} />
        ) : onboardingStep === "wallet" ? (
          <WalletSetupStep stepIndicator={stepIndicator} />
        ) : null}
      </div>
    </div>
  );
}
