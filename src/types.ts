export type NavCategory =
  | "Trending"
  | "My Markets"
  | "Politics"
  | "Sports"
  | "Culture"
  | "Bitcoin"
  | "Weather"
  | "Macro";
export type MarketCategory = Exclude<NavCategory, "Trending" | "My Markets">;
export type ViewMode = "home" | "detail" | "create" | "wallet";
export type Side = "yes" | "no";
export type OrderType = "market" | "limit";
export type ActionTab = "trade" | "issue" | "redeem" | "cancel";
export type CovenantState = 0 | 1 | 2 | 3;
export type TradeIntent = "open" | "close";
export type SizeMode = "sats" | "contracts";

export type ResolveTx = {
  txid: string;
  outcome: Side;
  sigVerified: boolean;
  height: number;
  signatureHash: string;
};

export type CollateralUtxo = {
  txid: string;
  vout: number;
  amountSats: number;
};

export type WalletNetwork = "liquid" | "liquid-testnet" | "liquid-regtest";
export type AppNetwork = "mainnet" | "testnet" | "regtest";

export type ChainTipResponse = {
  height: number;
  block_hash: string;
  timestamp: number;
};

export type BoltzLightningReceiveCreated = {
  id: string;
  flow: string;
  invoiceAmountSat: number;
  expectedOnchainAmountSat: number;
  invoice: string;
  invoiceExpiresAt: string;
  invoiceExpirySeconds: number;
};

export type BoltzSubmarineSwapCreated = {
  id: string;
  flow: string;
  invoiceAmountSat: number;
  expectedAmountSat: number;
  lockupAddress: string;
  bip21: string;
  invoiceExpiresAt: string;
  invoiceExpirySeconds: number;
};

export type BoltzChainSwapCreated = {
  id: string;
  flow: string;
  amountSat: number;
  expectedAmountSat: number;
  lockupAddress: string;
  claimLockupAddress: string;
  timeoutBlockHeight: number;
  bip21: string | null;
};

export type BoltzChainSwapPairInfo = {
  pairHash: string;
  minAmountSat: number;
  maxAmountSat: number;
  feePercentage: number;
  minerFeeLockupSat: number;
  minerFeeClaimSat: number;
  minerFeeServerSat: number;
  fixedMinerFeeTotalSat: number;
};

export type BoltzChainSwapPairsInfo = {
  bitcoinToLiquid: BoltzChainSwapPairInfo;
  liquidToBitcoin: BoltzChainSwapPairInfo;
};

export type PaymentSwap = {
  id: string;
  flow: string;
  network: string;
  status: string;
  invoiceAmountSat: number;
  expectedAmountSat: number | null;
  lockupAddress: string | null;
  invoice: string | null;
  invoiceExpiresAt: string | null;
  lockupTxid: string | null;
  createdAt: string;
  updatedAt: string;
};

export type DiscoveredMarket = {
  id: string;
  nevent: string;
  market_id: string;
  question: string;
  category: string;
  description: string;
  resolution_source: string;
  oracle_pubkey: string;
  expiry_height: number;
  cpt_sats: number;
  collateral_asset_id: string;
  yes_asset_id: string;
  no_asset_id: string;
  yes_reissuance_token: string;
  no_reissuance_token: string;
  creator_pubkey: string;
  created_at: number;
  creation_txid: string | null;
  state: CovenantState;
  nostr_event_json?: string | null;
  yes_price_bps?: number | null;
  no_price_bps?: number | null;
};

export type DiscoveredOrder = {
  id: string;
  order_uid: string;
  market_id: string;
  base_asset_id: string;
  quote_asset_id: string;
  price: number;
  min_fill_lots: number;
  min_remainder_lots: number;
  direction: OrderDirection;
  direction_label: string;
  maker_base_pubkey: string;
  order_nonce: string;
  covenant_address: string;
  offered_amount: number;
  cosigner_pubkey: string;
  maker_receive_spk_hash: string;
  creator_pubkey: string;
  created_at: number;
  nostr_event_json?: string | null;
  source?: "nostr" | "recovered-local";
  is_recoverable_by_current_wallet?: boolean;
  own_order_recovery_status?: OwnOrderRecoveryStatus | null;
};

export type OwnOrderRecoveryStatus =
  | "active_confirmed"
  | "active_mempool"
  | "spent_or_filled"
  | "ambiguous";

export type RecoveredOwnLimitOrder = {
  txid: string;
  vout: number;
  outpoint: string;
  offered_asset_id_hex: string;
  offered_amount: number;
  order_index: number | null;
  maker_base_pubkey_hex: string | null;
  order_nonce_hex: string | null;
  order_params: MakerOrderParamsPayload | null;
  status: OwnOrderRecoveryStatus;
  ambiguity_count: number;
  is_cancelable: boolean;
};

export type PricePoint = {
  block_height: number | null;
  yes_price_bps: number;
  no_price_bps: number;
  r_yes: number;
  r_no: number;
  r_lbtc: number;
};

export type ContractParamsPayload = {
  oracle_public_key: number[];
  collateral_asset_id: number[];
  yes_token_asset: number[];
  no_token_asset: number[];
  yes_reissuance_token: number[];
  no_reissuance_token: number[];
  collateral_per_token: number;
  expiry_time: number;
};

export type IssuanceResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  pairs_issued: number;
};

export type OrderDirection = "sell-base" | "sell-quote";

export type MakerOrderParamsPayload = {
  base_asset_id_hex: string;
  quote_asset_id_hex: string;
  price: number;
  min_fill_lots: number;
  min_remainder_lots: number;
  direction: OrderDirection;
  maker_receive_spk_hash_hex: string;
  cosigner_pubkey_hex: string;
  maker_pubkey_hex: string;
};

export type CreateLimitOrderRequestPayload = {
  base_asset_id_hex: string;
  quote_asset_id_hex: string;
  price: number;
  order_amount: number;
  direction: OrderDirection;
  min_fill_lots: number;
  min_remainder_lots: number;
  market_id: string;
  direction_label: string;
};

export type CreateLimitOrderResult = {
  txid: string;
  order_event_id: string;
  order_uid: string;
  order_params: MakerOrderParamsPayload;
  maker_base_pubkey_hex: string;
  order_nonce_hex: string;
  covenant_address: string;
  order_amount: number;
};

export type CancelLimitOrderRequestPayload = {
  order_params: MakerOrderParamsPayload;
  maker_base_pubkey_hex: string;
  order_nonce_hex: string;
};

export type CancelLimitOrderResult = {
  txid: string;
  refunded_amount: number;
};

export type FillLimitOrderRequestPayload = {
  order_params: MakerOrderParamsPayload;
  maker_base_pubkey_hex: string;
  order_nonce_hex: string;
  lots_to_fill: number;
};

export type FillLimitOrderResult = {
  txid: string;
  lots_filled: number;
  is_partial: boolean;
};

export type TradeDirection = "buy" | "sell";

export type TradeQuoteLegSource =
  | {
      kind: "amm_pool";
      pool_id: string;
    }
  | {
      kind: "limit_order";
      order_id: string;
      price: number;
      lots: number;
    };

export type TradeQuoteLeg = {
  source: TradeQuoteLegSource;
  input_amount: number;
  output_amount: number;
};

export type QuoteMarketTradeRequestPayload = {
  contract_params: ContractParamsPayload;
  market_id: string;
  side: Side;
  direction: TradeDirection;
  exact_input: number;
};

export type PreviewMarketTradeRequestPayload = {
  contract_params: ContractParamsPayload;
  market_id: string;
  side: Side;
  direction: TradeDirection;
  exact_input: number;
};

export type QuoteMarketTradeResult = {
  quote_id: string;
  market_id: string;
  side: Side;
  direction: TradeDirection;
  exact_input: number;
  total_input: number;
  total_output: number;
  effective_price: number;
  expires_at_unix: number;
  legs: TradeQuoteLeg[];
};

export type PreviewMarketTradeResult = {
  market_id: string;
  side: Side;
  direction: TradeDirection;
  exact_input: number;
  total_input: number;
  total_output: number;
  effective_price: number;
  legs: TradeQuoteLeg[];
};

export type ExecuteMarketTradeQuoteRequestPayload = {
  quote_id: string;
};

export type ExecuteMarketTradeQuoteResult = {
  txid: string;
  total_input: number;
  total_output: number;
  num_orders_filled: number;
  pool_used: boolean;
};

export type LimitSellWarning = {
  referencePriceSats: number;
  discountSats: number;
  discountPct: number;
};

export type IdentityResponse = { pubkey_hex: string; npub: string };

export type RelayEntry = { url: string; has_backup: boolean };
export type RelayBackupResult = { url: string; has_backup: boolean };
export type NostrBackupStatus = {
  has_backup: boolean;
  relay_results: RelayBackupResult[];
};
export type NostrProfile = {
  picture?: string;
  name?: string;
  display_name?: string;
};

export type AttestationResult = {
  market_id: string;
  outcome_yes: boolean;
  signature_hex: string;
  nostr_event_id: string;
};

export type Market = {
  id: string;
  nevent: string;
  question: string;
  category: MarketCategory;
  description: string;
  resolutionSource: string;
  isLive: boolean;
  state: CovenantState;
  marketId: string;
  oraclePubkey: string;
  expiryHeight: number;
  currentHeight: number;
  cptSats: number;
  collateralAssetId: string;
  yesAssetId: string;
  noAssetId: string;
  yesReissuanceToken: string;
  noReissuanceToken: string;
  creationTxid: string | null;
  collateralUtxos: CollateralUtxo[];
  resolveTx?: ResolveTx;
  nostrEventJson: string | null;
  yesPrice: number | null;
  change24h: number;
  volumeBtc: number;
  liquidityBtc: number;
  limitOrders: DiscoveredOrder[];
};

export type PathAvailability = {
  initialIssue: boolean;
  issue: boolean;
  resolve: boolean;
  redeem: boolean;
  expiryRedeem: boolean;
  cancel: boolean;
};

export type WalletTransaction = {
  txid: string;
  balanceChange: number;
  fee: number;
  height: number | null;
  timestamp: number | null;
  txType: string;
};

export type WalletUtxo = {
  txid: string;
  vout: number;
  assetId: string;
  value: number;
  height: number | null;
};

export type WalletData = {
  balance: Record<string, number>;
  transactions: WalletTransaction[];
  utxos: WalletUtxo[];
  swaps: PaymentSwap[];
  backupWords: string[];
  backedUp: boolean;
  showBackup: boolean;
  backupPassword: string;
};

export type BaseCurrency =
  | "BTC"
  | "USD"
  | "EUR"
  | "JPY"
  | "GBP"
  | "CNY"
  | "CHF"
  | "AUD"
  | "CAD";

export type OrderbookLevel = {
  priceSats: number;
  contracts: number;
};

export type FillEstimate = {
  avgPriceSats: number;
  bestPriceSats: number;
  worstPriceSats: number;
  filledContracts: number;
  requestedContracts: number;
  totalSats: number;
  isPartial: boolean;
};

export type TradePreview = {
  basePriceSats: number;
  limitPriceSats: number;
  referencePriceSats: number;
  requestedContracts: number;
  fill: FillEstimate;
  executionPriceSats: number;
  notionalSats: number;
  executedSats: number;
  executionFeeSats: number;
  winFeeSats: number;
  grossPayoutSats: number;
  netIfCorrectSats: number;
  maxProfitSats: number;
  netAfterFeesSats: number;
  slippagePct: number;
  positionContracts: number;
};
