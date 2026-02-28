import { tauriApi } from "../api/tauri.ts";
import { state } from "../state.ts";
import type { NostrBackupStatus, RelayBackupResult } from "../types.ts";

const DEFAULT_RELAYS = ["wss://relay.damus.io", "wss://relay.primal.net"];

export function applyRelayBackupStatus(status: NostrBackupStatus): void {
  state.nostrBackupStatus = status;
  if (!status.relay_results) return;
  state.relays = state.relays.map((relay) => ({
    ...relay,
    has_backup:
      status.relay_results.find(
        (result: RelayBackupResult) => result.url === relay.url,
      )?.has_backup ?? false,
  }));
}

export async function refreshRelayBackupStatus(): Promise<void> {
  const status = await tauriApi.checkNostrBackup();
  applyRelayBackupStatus(status);
}

export async function refreshRelaysAndBackup(options?: {
  fallbackToDefaults?: boolean;
}): Promise<void> {
  try {
    const relays = await tauriApi.fetchNip65RelayList();
    state.relays = relays.map((url) => ({ url, has_backup: false }));
  } catch {
    if (options?.fallbackToDefaults) {
      state.relays = DEFAULT_RELAYS.map((url) => ({ url, has_backup: false }));
    }
  }

  await refreshRelayBackupStatus();
}
