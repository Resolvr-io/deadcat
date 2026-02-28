import { invoke } from "@tauri-apps/api/core";
import type {
  AttestationResult,
  BoltzChainSwapCreated,
  BoltzChainSwapPairsInfo,
  BoltzLightningReceiveCreated,
  BoltzSubmarineSwapCreated,
  ChainTipResponse,
  DiscoveredMarket,
  IdentityResponse,
  IssuanceResult,
  NostrBackupStatus,
  NostrProfile,
  PaymentSwap,
  PricePoint,
  WalletNetwork,
  WalletTransaction,
} from "../types.ts";

type AppStateResponse = {
  walletStatus: "not_created" | "locked" | "unlocked";
  networkStatus: { network: string; policyAssetId: string };
};

type WalletBalanceResponse = { assets: Record<string, number> };
type WalletAddressResponse = { address: string };
type LiquidSendResult = { txid: string; feeSat: number };
type ResolveMarketResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  outcome_yes: boolean;
};
type CancelTokensResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  pairs_burned: number;
  is_full_cancellation: boolean;
};
type RedeemTokensResult = {
  txid: string;
  previous_state: number;
  tokens_redeemed: number;
  payout_sats: number;
};
type MarketStateResult = { state: number };

export function tauriInvoke<T>(
  command: string,
  payload?: Record<string, unknown>,
) {
  return invoke<T>(command, payload);
}

export const tauriApi = {
  getAppState: () => tauriInvoke<AppStateResponse>("get_app_state"),
  recordActivity: () => tauriInvoke<void>("record_activity"),

  fetchChainTip: (network: WalletNetwork) =>
    tauriInvoke<ChainTipResponse>("fetch_chain_tip", { network }),

  initNostrIdentity: () =>
    tauriInvoke<IdentityResponse | null>("init_nostr_identity"),
  generateNostrIdentity: () =>
    tauriInvoke<IdentityResponse>("generate_nostr_identity"),
  importNostrNsec: (nsec: string) =>
    tauriInvoke<IdentityResponse>("import_nostr_nsec", { nsec }),
  exportNostrNsec: () => tauriInvoke<string>("export_nostr_nsec"),
  deleteNostrIdentity: () => tauriInvoke<void>("delete_nostr_identity"),
  fetchNostrProfile: () =>
    tauriInvoke<NostrProfile | null>("fetch_nostr_profile"),
  fetchNip65RelayList: () => tauriInvoke<string[]>("fetch_nip65_relay_list"),
  setRelayList: (relays: string[]) =>
    tauriInvoke<void>("set_relay_list", { relays }),
  addRelay: (url: string) => tauriInvoke<string[]>("add_relay", { url }),
  removeRelay: (url: string) => tauriInvoke<string[]>("remove_relay", { url }),
  checkNostrBackup: () => tauriInvoke<NostrBackupStatus>("check_nostr_backup"),
  backupMnemonicToNostr: (password: string) =>
    tauriInvoke<string>("backup_mnemonic_to_nostr", { password }),
  restoreMnemonicFromNostr: () =>
    tauriInvoke<string>("restore_mnemonic_from_nostr"),
  deleteNostrBackup: () => tauriInvoke<string>("delete_nostr_backup"),

  discoverContracts: () =>
    tauriInvoke<DiscoveredMarket[]>("discover_contracts"),
  listContracts: () => tauriInvoke<DiscoveredMarket[]>("list_contracts"),
  createContractOnchain: (request: {
    question: string;
    description: string;
    category: string;
    resolution_source: string;
    settlement_deadline_unix: number;
    collateral_per_token: number;
  }) => tauriInvoke<DiscoveredMarket>("create_contract_onchain", { request }),
  issueTokens: (
    contractParamsJson: string,
    creationTxid: string,
    pairs: number,
  ) =>
    tauriInvoke<IssuanceResult>("issue_tokens", {
      contractParamsJson,
      creationTxid,
      pairs,
    }),
  cancelTokens: (contractParamsJson: string, pairs: number) =>
    tauriInvoke<CancelTokensResult>("cancel_tokens", {
      contractParamsJson,
      pairs,
    }),
  resolveMarket: (
    contractParamsJson: string,
    outcomeYes: boolean,
    oracleSignatureHex: string,
  ) =>
    tauriInvoke<ResolveMarketResult>("resolve_market", {
      contractParamsJson,
      outcomeYes,
      oracleSignatureHex,
    }),
  redeemTokens: (contractParamsJson: string, tokens: number) =>
    tauriInvoke<RedeemTokensResult>("redeem_tokens", {
      contractParamsJson,
      tokens,
    }),
  redeemExpired: (
    contractParamsJson: string,
    tokenAssetHex: string,
    tokens: number,
  ) =>
    tauriInvoke<RedeemTokensResult>("redeem_expired", {
      contractParamsJson,
      tokenAssetHex,
      tokens,
    }),
  getMarketState: (contractParamsJson: string) =>
    tauriInvoke<MarketStateResult>("get_market_state", { contractParamsJson }),
  oracleAttest: (marketIdHex: string, outcomeYes: boolean) =>
    tauriInvoke<AttestationResult>("oracle_attest", {
      marketIdHex,
      outcomeYes,
    }),
  syncPool: (poolId: string) => tauriInvoke<void>("sync_pool", { poolId }),
  getPoolPriceHistory: (marketId: string) =>
    tauriInvoke<PricePoint[]>("get_pool_price_history", { marketId }),

  createWallet: (password: string) =>
    tauriInvoke<string>("create_wallet", { password }),
  restoreWallet: (mnemonic: string, password: string) =>
    tauriInvoke<void>("restore_wallet", {
      mnemonic,
      password,
    }),
  unlockWallet: (password: string) =>
    tauriInvoke<void>("unlock_wallet", { password }),
  lockWallet: () => tauriInvoke<void>("lock_wallet"),
  deleteWallet: () => tauriInvoke<void>("delete_wallet"),
  syncWallet: () => tauriInvoke<void>("sync_wallet"),
  getWalletBalance: () =>
    tauriInvoke<WalletBalanceResponse>("get_wallet_balance"),
  getWalletTransactions: () =>
    tauriInvoke<WalletTransaction[]>("get_wallet_transactions"),
  getWalletAddress: (index?: number) =>
    tauriInvoke<WalletAddressResponse>("get_wallet_address", { index }),
  listPaymentSwaps: () => tauriInvoke<PaymentSwap[]>("list_payment_swaps"),
  refreshPaymentSwapStatus: (swapId: string) =>
    tauriInvoke<PaymentSwap>("refresh_payment_swap_status", { swapId }),
  getMnemonicWordCount: (password: string) =>
    tauriInvoke<number>("get_mnemonic_word_count", { password }),
  getMnemonicWord: (password: string, index: number) =>
    tauriInvoke<string>("get_mnemonic_word", { password, index }),

  getChainSwapPairs: () =>
    tauriInvoke<BoltzChainSwapPairsInfo>("get_chain_swap_pairs"),
  createLightningReceive: (amountSat: number) =>
    tauriInvoke<BoltzLightningReceiveCreated>("create_lightning_receive", {
      amountSat,
    }),
  createBitcoinReceive: (amountSat: number) =>
    tauriInvoke<BoltzChainSwapCreated>("create_bitcoin_receive", { amountSat }),
  createBitcoinSend: (amountSat: number) =>
    tauriInvoke<BoltzChainSwapCreated>("create_bitcoin_send", { amountSat }),
  payLightningInvoice: (invoice: string) =>
    tauriInvoke<BoltzSubmarineSwapCreated>("pay_lightning_invoice", {
      invoice,
    }),
  sendLbtc: (address: string, amountSat: number) =>
    tauriInvoke<LiquidSendResult>("send_lbtc", { address, amountSat }),
};
