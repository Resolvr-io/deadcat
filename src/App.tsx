import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect } from "react";
import CreateMarketPage from "./components/create/CreateMarketPage";
import DetailPage from "./components/detail/DetailPage";
import HomePage from "./components/home/HomePage";
import { TopShell } from "./components/layout/TopShell";
import { NostrEventModal } from "./components/modals/NostrEventModal";
import OnboardingOverlay from "./components/onboarding/OnboardingOverlay";
import ProfilePage from "./components/profile/ProfilePage";

import { ToastContainer } from "./components/shared/Toast";
import { WalletPage } from "./components/wallet/WalletPage";
import { useActivityTracking } from "./hooks/useActivityTracking";
import { useBootstrap } from "./hooks/useBootstrap";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { useStore } from "./store";

function CloseConfirmDialog() {
  const open = useStore((s) => s.closeConfirmOpen);

  const handleCancel = useCallback(() => {
    useStore.setState({ closeConfirmOpen: false });
  }, []);

  const handleQuit = useCallback(() => {
    void invoke("confirm_quit");
  }, []);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div className="w-full max-w-sm rounded-2xl border border-slate-800 bg-slate-950 p-8 text-center">
        <h2 className="text-lg font-medium text-slate-100">
          Quit Deadcat Live?
        </h2>
        <p className="mt-2 text-sm text-slate-400">
          Make sure you&apos;ve saved your work before quitting.
        </p>
        <div className="mt-6 flex gap-3">
          <button
            type="button"
            onClick={handleCancel}
            className="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleQuit}
            className="flex-1 rounded-xl bg-rose-500 py-2.5 text-sm font-medium text-white transition hover:bg-rose-400"
          >
            Quit
          </button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  useBootstrap();
  useTauriEvents();
  useActivityTracking();

  // Listen for close-requested from Rust (Cmd+Q / window close)
  useEffect(() => {
    const unlisten = listen("close-requested", () => {
      useStore.setState({ closeConfirmOpen: true });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const view = useStore((s) => s.view);
  const walletOpen = useStore((s) => s.walletOpen);
  const setupModalOpen = useStore((s) => s.setupModalOpen);
  const nostrEventModal = useStore((s) => s.nostrEventModal);

  return (
    <>
      <div className="min-h-screen text-slate-100">
        <TopShell />

        <main>
          {view === "home" && <HomePage />}
          {view === "detail" && <DetailPage />}
          {view === "create" && <CreateMarketPage />}
        </main>
      </div>

      <ProfilePage />
      {walletOpen && <WalletPage />}
      {setupModalOpen && <OnboardingOverlay />}
      {nostrEventModal && <NostrEventModal />}
      <CloseConfirmDialog />
      <ToastContainer />
    </>
  );
}
