import { useMutation, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { tauriApi } from "../../api/tauri";
import { DEFAULT_TX_OPTIONS } from "../../services/tx";
import { useStore } from "../../store";
import type {
  BoltzChainSwapCreated,
  BoltzChainSwapPairInfo,
  BoltzLightningReceiveCreated,
  BoltzSubmarineSwapCreated,
  LiquidSendResult,
  TxOptions,
} from "../../types";

export function useCreateWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (params: {
      password: string;
    }): Promise<{ mnemonic: string }> => {
      const mnemonic = await invoke<string>("create_wallet", {
        password: params.password,
      });
      // Ensure a Nostr node exists before unlocking (unlock_wallet requires it)
      if (!useStore.getState().nostrPubkey) {
        const identity = await invoke<{ pubkey_hex: string; npub: string }>(
          "generate_nostr_identity",
        );
        useStore.setState({
          nostrPubkey: identity.pubkey_hex,
          nostrNpub: identity.npub,
        });
      } else {
        // Re-initialize existing identity so the node is available
        await invoke("init_nostr_identity");
      }
      await tauriApi.unlockWallet(params.password);
      return { mnemonic };
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
    },
  });
}

export function useUnlockWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (params: { password: string }): Promise<void> => {
      await tauriApi.unlockWallet(params.password);
      useStore.setState({ walletSessionPassword: params.password });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useRestoreWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (params: {
      mnemonic: string;
      password: string;
      unlock?: boolean;
    }): Promise<void> => {
      await tauriApi.restoreWallet(params.mnemonic.trim(), params.password);
      if (params.unlock !== false) {
        // Ensure a Nostr node exists before unlocking
        if (!useStore.getState().nostrPubkey) {
          const identity = await invoke<{ pubkey_hex: string; npub: string }>(
            "generate_nostr_identity",
          );
          useStore.setState({
            nostrPubkey: identity.pubkey_hex,
            nostrNpub: identity.npub,
          });
        } else {
          await invoke("init_nostr_identity");
        }
        await tauriApi.unlockWallet(params.password);
      }
      await tauriApi.syncWallet();
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useSyncWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (): Promise<void> => tauriApi.syncWallet(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useLockWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (): Promise<void> => invoke("lock_wallet"),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
    },
  });
}

export function useDeleteWallet() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: { password: string }): Promise<void> =>
      invoke("delete_wallet", { password: params.password }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useSendLiquid() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      address: string;
      amountSat: number;
      txOptions?: TxOptions;
    }): Promise<LiquidSendResult> =>
      invoke("send_lbtc", {
        address: params.address,
        amountSat: params.amountSat,
        txOptions: params.txOptions ?? DEFAULT_TX_OPTIONS,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useCreateLightningReceive() {
  return useMutation({
    mutationFn: (params: {
      amountSat: number;
    }): Promise<BoltzLightningReceiveCreated> =>
      invoke("create_lightning_receive", { amountSat: params.amountSat }),
  });
}

export function usePayLightningInvoice() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      invoice: string;
    }): Promise<BoltzSubmarineSwapCreated> =>
      invoke("pay_lightning_invoice", { invoice: params.invoice }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useCreateBitcoinReceive() {
  return useMutation({
    mutationFn: (params: {
      amountSat: number;
    }): Promise<BoltzChainSwapCreated> =>
      invoke("create_bitcoin_receive", { amountSat: params.amountSat }),
  });
}

export function useCreateBitcoinSend() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      amountSat: number;
      pairInfo: BoltzChainSwapPairInfo;
    }): Promise<BoltzChainSwapCreated> =>
      invoke("create_bitcoin_send", {
        amountSat: params.amountSat,
        pairInfo: params.pairInfo,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}

export function useGenerateLiquidAddress() {
  return useMutation({
    mutationFn: async (): Promise<string> => {
      const result = await invoke<{ index: number; address: string }>(
        "get_wallet_address",
        {},
      );
      return result.address;
    },
  });
}
