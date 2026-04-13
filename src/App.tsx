import CreateMarketPage from "./components/create/CreateMarketPage";
import DetailPage from "./components/detail/DetailPage";
import HomePage from "./components/home/HomePage";
import { TopShell } from "./components/layout/TopShell";
import { NostrEventModal } from "./components/modals/NostrEventModal";
import OnboardingOverlay from "./components/onboarding/OnboardingOverlay";
import { OverlayLoader } from "./components/shared/OverlayLoader";
import { ToastContainer } from "./components/shared/Toast";
import { WalletPage } from "./components/wallet/WalletPage";
import { useActivityTracking } from "./hooks/useActivityTracking";
import { useBootstrap } from "./hooks/useBootstrap";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { useStore } from "./store";

export default function App() {
  useBootstrap();
  useTauriEvents();
  useActivityTracking();

  const view = useStore((s) => s.view);
  const walletOpen = useStore((s) => s.walletOpen);
  const setupModalOpen = useStore((s) => s.setupModalOpen);
  const nostrEventModal = useStore((s) => s.nostrEventModal);
  const marketsLoading = useStore((s) => s.marketsLoading);

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

      {walletOpen && <WalletPage />}
      {setupModalOpen && <OnboardingOverlay />}
      {nostrEventModal && <NostrEventModal />}
      {marketsLoading && <OverlayLoader message="Loading markets..." />}
      <ToastContainer />
    </>
  );
}
