import { useMutation, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { tauriApi } from "../../api/tauri";
import { DEFAULT_TX_OPTIONS } from "../../services/tx";
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
    mutationFn: (params: { password: string }): Promise<void> =>
      tauriApi.unlockWallet(params.password),
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
      invoke("send_liquid", {
        request: {
          address: params.address,
          amount_sat: params.amountSat,
          tx_options: params.txOptions ?? DEFAULT_TX_OPTIONS,
        },
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
    mutationFn: (): Promise<string> => invoke("get_receive_address"),
  });
}
