import { invoke } from "@tauri-apps/api/core";
import type {
  AppNetwork,
  ChainTipResponse,
  MarketComment,
  NostrBackupStatus,
  PaymentSwap,
  WalletNetwork,
  WalletTransaction,
} from "../types.ts";

type AppStateResponse = {
  walletStatus: "not_created" | "locked" | "unlocked";
  networkStatus: { network: AppNetwork; policyAssetId: string };
};

type WalletBalanceResponse = { assets: Record<string, number> };

export function tauriInvoke<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, payload);
}

export const tauriApi = {
  getAppState: () => tauriInvoke<AppStateResponse>("get_app_state"),
  recordActivity: () => tauriInvoke<void>("record_activity"),

  fetchChainTip: (network: WalletNetwork) =>
    tauriInvoke<ChainTipResponse>("fetch_chain_tip", { network }),

  fetchNip65RelayList: () => tauriInvoke<string[]>("fetch_nip65_relay_list"),
  checkNostrBackup: () => tauriInvoke<NostrBackupStatus>("check_nostr_backup"),

  getWalletBalance: () =>
    tauriInvoke<WalletBalanceResponse>("get_wallet_balance"),
  getWalletTransactions: () =>
    tauriInvoke<WalletTransaction[]>("get_wallet_transactions"),
  listPaymentSwaps: () => tauriInvoke<PaymentSwap[]>("list_payment_swaps"),

  restoreWallet: (mnemonic: string, password: string) =>
    tauriInvoke<void>("restore_wallet", { mnemonic, password }),
  unlockWallet: (password: string) =>
    tauriInvoke<void>("unlock_wallet", { password }),
  syncWallet: () => tauriInvoke<void>("sync_wallet"),

  // NIP-22 market comments
  fetchMarketComments: (args: {
    marketIdHex: string;
    creatorPubkeyHex: string;
  }) =>
    tauriInvoke<MarketComment[]>("fetch_market_comments", {
      marketIdHex: args.marketIdHex,
      creatorPubkeyHex: args.creatorPubkeyHex,
    }),
  publishMarketComment: (args: {
    marketIdHex: string;
    creatorPubkeyHex: string;
    marketEventIdHex: string;
    body: string;
    parentEventIdHex?: string;
    parentAuthorPubkeyHex?: string;
  }) =>
    tauriInvoke<MarketComment>("publish_market_comment", {
      marketIdHex: args.marketIdHex,
      creatorPubkeyHex: args.creatorPubkeyHex,
      marketEventIdHex: args.marketEventIdHex,
      parentEventIdHex: args.parentEventIdHex ?? null,
      parentAuthorPubkeyHex: args.parentAuthorPubkeyHex ?? null,
      body: args.body,
    }),
  deleteMarketComment: (commentEventIdHex: string) =>
    tauriInvoke<void>("delete_market_comment", { commentEventIdHex }),
};
