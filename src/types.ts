export type NavCategory =
  | "Trending"
  | "Ending Soon"
  | "New"
  | "Portfolio"
  | "My Markets"
  | "Resolved"
  | "Politics"
  | "Sports"
  | "Culture"
  | "Bitcoin"
  | "Weather"
  | "Macro";
export type MarketCategory = Exclude<
  NavCategory,
  "Trending" | "Ending Soon" | "New" | "Portfolio" | "My Markets" | "Resolved"
>;
export type ViewMode = "home" | "detail" | "create" | "group";

// ── Multi-outcome market group types ────────────────────────────────
export type MarketGroupOutcome = {
  id: string;
  name: string;
  yesPrice: number;
  change24h: number;
  volumeBtc: number;
  traderCount: number;
};

export type MarketGroup = {
  id: string;
  title: string;
  description: string;
  category: MarketCategory;
  resolutionSource: string;
  expiryHeight: number;
  currentHeight: number;
  outcomes: MarketGroupOutcome[];
  totalVolumeBtc: number;
  traderCount: number;
  createdAt: number;
  state: "active" | "resolved";
  resolvedOutcomeId?: string;
  cptSats: number;
  nevent?: string;
  nostrEventJson?: string;
  creationTxid?: string;
};
export type Side = "yes" | "no";
export type OrderType = "market" | "limit";
export type ActionTab = "trade" | "issue" | "redeem" | "cancel";
export type CovenantState = 0 | 1 | 2 | 3 | 4;
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

export type DormantOutputOpening = {
  asset_blinding_factor: string;
  value_blinding_factor: string;
};

export type PredictionMarketAnchor = {
  creation_txid: string;
  yes_dormant_opening: DormantOutputOpening;
  no_dormant_opening: DormantOutputOpening;
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
  anchor: PredictionMarketAnchor;
  state: CovenantState;
  nostr_event_json?: string | null;
  yes_price_bps?: number | null;
  no_price_bps?: number | null;
  dormant_txid?: string | null;
  unresolved_txid?: string | null;
  resolved_yes_txid?: string | null;
  resolved_no_txid?: string | null;
  expired_txid?: string | null;
};

export type MarketComment = {
  id: string;
  author_pubkey: string;
  content: string;
  created_at: number;
  market_id: string;
  parent_id?: string | null;
  /** True when a kind:5 deletion has been published against this
   *  comment. Backend keeps the record so the UI can render a
   *  tombstone when the comment has replies; leaves without replies
   *  are hidden by the caller. */
  deleted?: boolean;
  nostr_event_json?: string | null;
};

export type DiscoveredOrder = {
  id: string;
  market_id: string;
  base_asset_id: string;
  quote_asset_id: string;
  price: number;
  min_fill_lots: number;
  min_remainder_lots: number;
  direction: "sell-base" | "sell-quote";
  direction_label: string;
  maker_base_pubkey: string;
  order_nonce: string;
  covenant_address: string;
  offered_amount: number;
  cosigner_pubkey: string;
  maker_receive_spk_hash: string;
  creator_pubkey: string;
  created_at: number;
  source?: string;
  nostr_event_json?: string | null;
};

export type MinerFeePolicy =
  | {
      kind: "confirmation_target_blocks";
      blocks: number;
    }
  | {
      kind: "rate_sat_per_vb";
      sat_per_vb: number;
    }
  | {
      kind: "exact_amount_sat";
      amount_sat: number;
    };

export type TxOptions = {
  feePolicy: MinerFeePolicy;
};

export type ResolvedMinerFee = {
  policy: MinerFeePolicy;
  amountSat: number;
  rateSatPerVb: number;
  discountVsize: number;
};

export type CreateContractOnchainResponse = {
  market: DiscoveredMarket;
  fee: ResolvedMinerFee;
};

export type IssuanceResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  pairs_issued: number;
  fee: ResolvedMinerFee;
};

export type ResolutionResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  outcome_yes: boolean;
  fee: ResolvedMinerFee;
};

export type CancellationResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  pairs_burned: number;
  is_full_cancellation: boolean;
  fee: ResolvedMinerFee;
};

export type RedemptionResult = {
  txid: string;
  previous_state: number;
  tokens_redeemed: number;
  payout_sats: number;
  fee: ResolvedMinerFee;
};

export type IdentityResponse = { pubkey_hex: string; npub: string };

export type PreviewIdentityResponse = {
  pubkey_hex: string;
  npub: string;
  nsec: string;
};

export type Nip46Status = {
  connected: boolean;
  remotSignerPubkey: string;
  userPubkey: string | null;
  relayUrls: string[];
  bunkerUri: string;
};

export type RelayEntry = { url: string; has_backup: boolean };
export type RelayBackupResult = { url: string; has_backup: boolean };
export type WalletEntry = { name: string; d_tag: string };
export type NostrBackupStatus = {
  has_backup: boolean;
  relay_results: RelayBackupResult[];
  wallets: WalletEntry[];
};
export type NostrProfile = {
  picture?: string;
  banner?: string;
  name?: string;
  display_name?: string;
  about?: string;
  website?: string;
  nip05?: string;
  lud16?: string;
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
  anchor: PredictionMarketAnchor | null;
  limitOrders: DiscoveredOrder[];
  creationTxid: string | null;
  collateralUtxos: CollateralUtxo[];
  resolveTx?: ResolveTx;
  nostrEventJson: string | null;
  yesPrice: number | null;
  change24h: number;
  volumeBtc: number;
  liquidityBtc: number;
  traderCount: number;
  openInterestBtc: number;
  createdAt: number;
  creatorPubkey: string;
  dormantTxid: string | null;
  unresolvedTxid: string | null;
  resolvedYesTxid: string | null;
  resolvedNoTxid: string | null;
  expiredTxid: string | null;
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
  backupCopied: boolean;
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

export type FullOrderbook = {
  asks: OrderbookLevel[];
  bids: OrderbookLevel[];
  spread: number | null;
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

export type LimitSellWarning = {
  referencePriceSats: number;
  discountSats: number;
  discountPct: number;
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
  grossPayoutSats: number;
  slippagePct: number;
  positionContracts: number;
};

export type TradeDirection = "buy" | "sell";

export type RouteLegSource =
  | {
      kind: "lmsr_pool";
      pool_id: string;
      old_s_index: number;
      new_s_index: number;
    }
  | {
      kind: "limit_order";
      order_id: string;
      price: number;
      lots: number;
    };

export type RouteLeg = {
  source: RouteLegSource;
  input_amount: number;
  output_amount: number;
};

export type TradeQuoteResponse = {
  total_input: number;
  total_output: number;
  effective_price: number;
  legs: RouteLeg[];
};

export type QuoteMarketTradeResult = TradeQuoteResponse & {
  direction: TradeDirection;
  quote_id?: string;
  expires_at_unix?: number;
};

export type ExecuteTradeExpectedQuote = {
  total_input: number;
  total_output: number;
  legs: RouteLeg[];
};

export type ExecuteTradeResponse = {
  txid: string;
  total_input: number;
  total_output: number;
  num_orders_filled: number;
  pool_used: boolean;
  new_reserves: {
    r_yes: number;
    r_no: number;
    r_lbtc: number;
  } | null;
  fee: ResolvedMinerFee;
};

export type CreateLimitOrderResponse = {
  txid: string;
  nostr_event_id: string;
  covenant_address: string;
  order_amount: number;
  order_index: number;
  fee: ResolvedMinerFee;
};

export type CancelLimitOrderResponse = {
  txid: string;
  refunded_amount: number;
  fee: ResolvedMinerFee;
};

export type OwnOrderSummary = {
  creation_txid: string | null;
  market_id: string | null;
  direction_label: string | null;
  price: number;
  offered_amount: number | null;
  order_status: string;
};

export type TradeQuoteSnapshot = {
  marketId: string;
  side: Side;
  direction: TradeDirection;
  exactInput: number;
  quote: TradeQuoteResponse;
};

// LMSR Pool types
export type LmsrPoolInfo = {
  pool_id: string;
  market_id: string;
  creation_txid: string;
  current_s_index: number;
  reserve_yes: number;
  reserve_no: number;
  reserve_collateral: number;
  state_source: string;
  params_json: string;
  created_at: string;
  updated_at: string;
};

export type PriceHistoryEntry = {
  pool_id: string;
  market_id: string;
  transition_txid: string;
  old_s_index: number;
  new_s_index: number;
  reserve_yes: number;
  reserve_no: number;
  reserve_collateral: number;
  implied_yes_price_bps: number;
  block_height: number;
};

export type ChartTimescale = "1h" | "4h" | "1d" | "3d" | "7d" | "1M" | "all";

export type CreateLmsrPoolResponse = {
  txid: string;
  pool_id: string;
  fee: ResolvedMinerFee;
};

export type ScanLmsrPoolResponse = {
  pool_id: string;
  current_s_index: number;
  reserve_yes: number;
  reserve_no: number;
  reserve_collateral: number;
};

export type CloseLmsrPoolResponse = {
  txid: string;
  reclaimed_yes: number;
  reclaimed_no: number;
  reclaimed_collateral: number;
};

export type LiquidSendResult = {
  txid: string;
  feeSat: number;
  fee: ResolvedMinerFee;
};

export type PrepareSendResult = {
  address: string;
  amountSat: number;
  feeSat: number;
  fee: ResolvedMinerFee;
};
