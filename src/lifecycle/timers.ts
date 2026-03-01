import {
  currentWalletNetwork,
  syncCurrentHeightFromLwk,
} from "../services/wallet.ts";
import { state } from "../state.ts";

export function startRecurringTimers(
  render: () => void,
  updateEstClockLabels: () => void,
): () => void {
  const estClockInterval = setInterval(updateEstClockLabels, 1_000);
  const heightSyncInterval = setInterval(() => {
    if (state.onboardingStep === null) {
      void syncCurrentHeightFromLwk(
        currentWalletNetwork(),
        render,
        updateEstClockLabels,
      );
    }
  }, 60_000);

  return () => {
    clearInterval(estClockInterval);
    clearInterval(heightSyncInterval);
  };
}
