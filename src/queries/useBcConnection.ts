import {
  disconnect as bcDisconnect,
  connectNWC,
  getConnectorConfig,
  onConnected,
  onDisconnected,
} from "@getalby/bitcoin-connect-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useStore } from "../store";

/**
 * Exposes Bitcoin Connect's connection state as a React hook and
 * bridges it to the Rust-side encrypted NWC store.
 *
 * Lifecycle:
 * - On `onConnected` (user completes the connect flow): pull the
 *   active connector config from BC, extract the NWC URL, and
 *   persist it encrypted via `set_nwc_url` keyed by the wallet
 *   session password.
 * - On `onDisconnected`: wipe the stored URL via `clear_nwc_url`.
 * - On wallet unlock: if a stored URL exists and BC isn't already
 *   connected, decrypt it and call `connectNWC(url)` to silently
 *   restore the previous session.
 *
 * BC itself is initialized with `persistConnection: false` so it
 * never writes the URL to plaintext localStorage — this hook is the
 * only persistence path.
 */
export type BcConnectionState = {
  connected: boolean;
  alias: string | null;
};

type BcProvider = {
  getInfo?: () => Promise<{ node?: { alias?: string } }>;
};

async function readAliasFromProvider(
  provider: BcProvider,
): Promise<string | null> {
  try {
    const info = await provider.getInfo?.();
    return info?.node?.alias?.trim() || null;
  } catch {
    return null;
  }
}

async function persistActiveNwcUrl(sessionPassword: string) {
  if (!sessionPassword) return;
  const config = getConnectorConfig();
  const url = config?.nwcUrl;
  if (!url) return;
  try {
    await invoke<void>("set_nwc_url", { url, password: sessionPassword });
  } catch (e) {
    console.warn("[nwc] failed to persist NWC URL:", e);
  }
}

async function clearStoredNwcUrl() {
  try {
    await invoke<void>("clear_nwc_url");
  } catch (e) {
    // Not fatal — the URL may simply not exist yet.
    console.warn("[nwc] clear_nwc_url failed:", e);
  }
}

/** Try to restore a previously-saved NWC connection. No-op if BC is
 *  already connected, the wallet isn't unlocked, or nothing's stored. */
async function tryRestoreConnection(sessionPassword: string) {
  if (!sessionPassword) return;
  try {
    const existing = getConnectorConfig();
    if (existing?.nwcUrl) return; // BC already has a live session
    const hasStored = await invoke<boolean>("has_nwc_url");
    if (!hasStored) return;
    const url = await invoke<string>("get_nwc_url", {
      password: sessionPassword,
    });
    if (url) connectNWC(url);
  } catch (e) {
    console.warn("[nwc] restore failed:", e);
  }
}

export function useBcConnection(): BcConnectionState & {
  disconnect: () => Promise<void>;
} {
  const [state, setState] = useState<BcConnectionState>({
    connected: false,
    alias: null,
  });
  const sessionPassword = useStore((s) => s.walletSessionPassword);

  // Hold the latest session password in a ref so BC event callbacks
  // always read the current value without needing to re-subscribe.
  const sessionPasswordRef = useRef(sessionPassword);
  useEffect(() => {
    sessionPasswordRef.current = sessionPassword;
  }, [sessionPassword]);

  useEffect(() => {
    const unsubConnected = onConnected(async (provider: BcProvider) => {
      const alias = await readAliasFromProvider(provider);
      setState({ connected: true, alias });
      // Persist the URL for next boot. Safe to call repeatedly — it
      // just overwrites with the same encrypted blob.
      await persistActiveNwcUrl(sessionPasswordRef.current);
    });
    const unsubDisconnected = onDisconnected(() => {
      setState({ connected: false, alias: null });
      void clearStoredNwcUrl();
    });
    return () => {
      unsubConnected?.();
      unsubDisconnected?.();
    };
  }, []);

  // Boot-time restore: fires whenever the wallet unlock password
  // becomes available (login, unlock after auto-lock, app start with
  // wallet already unlocked). Idempotent — bails early if BC is
  // already connected or nothing is stored.
  useEffect(() => {
    if (!sessionPassword) return;
    void tryRestoreConnection(sessionPassword);
  }, [sessionPassword]);

  return {
    ...state,
    disconnect: async () => {
      await bcDisconnect();
    },
  };
}
