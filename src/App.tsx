import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect } from "react";
import CreateMarketPage from "./components/create/CreateMarketPage";
import DetailPage from "./components/detail/DetailPage";
import GroupDetailPage from "./components/group/GroupDetailPage";
import HomePage from "./components/home/HomePage";
import { Footer } from "./components/layout/Footer";
import { TopShell } from "./components/layout/TopShell";
import { NostrEventModal } from "./components/modals/NostrEventModal";
import OnboardingOverlay from "./components/onboarding/OnboardingOverlay";
import ProfilePage from "./components/profile/ProfilePage";

import { ToastContainer } from "./components/shared/Toast";
import { WalletPage } from "./components/wallet/WalletPage";
import { useActivityTracking } from "./hooks/useActivityTracking";
import { useBootstrap } from "./hooks/useBootstrap";
import { useEscapeKey } from "./hooks/useEscapeKey";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { useNotificationsEventListener } from "./queries/useNotifications";
import { useStore } from "./store";

/**
 * Always-available window drag strip along the top 32px of the
 * viewport. The TopShell header handles dragging when visible, but
 * once the user scrolls down the header leaves the screen and the
 * titlebar area becomes dead space. This fixed overlay catches
 * drags regardless of scroll position. Transparent and only 32px
 * tall so it never overlaps real UI below the traffic lights.
 */
function TitleBarDragStrip() {
  const onMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (e.buttons !== 1) return;
    e.preventDefault();
    void getCurrentWindow().startDragging();
  }, []);
  return (
    <div
      aria-hidden="true"
      onMouseDown={onMouseDown}
      className="fixed top-0 right-0 left-0 z-[999] h-8"
    />
  );
}

function CloseConfirmDialog() {
  const open = useStore((s) => s.closeConfirmOpen);

  const handleCancel = useCallback(() => {
    useStore.setState({ closeConfirmOpen: false });
  }, []);

  const handleQuit = useCallback(() => {
    void invoke("confirm_quit");
  }, []);

  useEscapeKey(open, handleCancel);

  if (!open) return null;

  return (
    <div className="macos-overlay-safe-top fixed inset-0 z-[100] flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div className="w-full max-w-sm rounded-2xl border border-slate-800 bg-slate-950 p-8 text-center">
        <h2 className="text-lg font-medium text-slate-100">
          Quit Deadcat Live?
        </h2>
        <p className="mt-2 text-sm text-slate-400">
          Your wallet and identity are saved. Pending operations will be
          interrupted.
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
  useNotificationsEventListener();

  // Tag <html> with the host OS so CSS can key off it (e.g. reserve space
  // for macOS traffic lights without affecting other platforms).
  useEffect(() => {
    const ua = navigator.userAgent;
    const os = /Mac/i.test(ua)
      ? "macos"
      : /Win/i.test(ua)
        ? "windows"
        : "linux";
    document.documentElement.dataset.os = os;
  }, []);

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
      <TitleBarDragStrip />
      <div className="flex min-h-screen flex-col text-slate-100">
        <TopShell />

        <main className="flex-1">
          {view === "home" && <HomePage />}
          {view === "detail" && <DetailPage />}
          {view === "group" && <GroupDetailPage />}
          {view === "create" && <CreateMarketPage />}
        </main>

        <Footer />
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
