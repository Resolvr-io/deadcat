import {
  disconnect as bcDisconnect,
  onConnected,
  onDisconnected,
} from "@getalby/bitcoin-connect-react";
import { useEffect, useState } from "react";

/**
 * Exposes the Bitcoin Connect connection state as a React hook.
 *
 * Bitcoin Connect keeps its own singleton store; we mirror the bits we
 * care about (is-connected + wallet alias) so the UI can re-render
 * when the user attaches or detaches a wallet.
 *
 * Secret-grade persistence (the NWC URL itself) is planned to move to
 * a Rust-side encrypted store for parity with the wallet mnemonic;
 * for now we rely on BC's built-in `localStorage` so the connect
 * button is functional. See memory/project_bitcoin_connect_nwc.md.
 */
export type BcConnectionState = {
  connected: boolean;
  alias: string | null;
};

type BcProvider = {
  getInfo?: () => Promise<{ node?: { alias?: string } }>;
};

export function useBcConnection(): BcConnectionState & {
  disconnect: () => Promise<void>;
} {
  const [state, setState] = useState<BcConnectionState>({
    connected: false,
    alias: null,
  });

  useEffect(() => {
    const unsubConnected = onConnected(async (provider: BcProvider) => {
      let alias: string | null = null;
      try {
        const info = await provider.getInfo?.();
        alias = info?.node?.alias?.trim() || null;
      } catch {
        // getInfo isn't required by NWC; fall through with no alias.
      }
      setState({ connected: true, alias });
    });
    const unsubDisconnected = onDisconnected(() => {
      setState({ connected: false, alias: null });
    });
    return () => {
      unsubConnected?.();
      unsubDisconnected?.();
    };
  }, []);

  return {
    ...state,
    disconnect: async () => {
      await bcDisconnect();
    },
  };
}
