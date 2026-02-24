import "./style.css";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";

const app = document.querySelector<HTMLDivElement>("#app")!;
const DEV_MODE = import.meta.env.DEV;

// ── Deadcat loader SVG markup (reused for overlay) ──

const LOADER_CAT_SVG = `<svg viewBox="0 0 260 267" fill="none" xmlns="http://www.w3.org/2000/svg"><path fill-rule="evenodd" clip-rule="evenodd" d="M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.231 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.217 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" fill="#34d399"/></svg>`;

const LOADER_BAG_SVG = `<svg viewBox="0 0 298 376" fill="none" xmlns="http://www.w3.org/2000/svg"><path fill-rule="evenodd" clip-rule="evenodd" d="M11.897 1.04645L36.1423 14.835L60.3877 1.04645C62.8491 -0.348818 65.8439 -0.348818 68.3053 1.04645L92.5507 14.835L116.796 1.04645C119.257 -0.348818 122.252 -0.348818 124.714 1.04645L148.959 14.835L173.204 1.04645C175.666 -0.348818 178.661 -0.348818 181.122 1.04645L205.367 14.835L229.613 1.04645C232.074 -0.348818 235.069 -0.348818 237.53 1.04645L261.776 14.835L286.021 1.04645C288.523 -0.348818 291.559 -0.348818 294.021 1.08749C296.482 2.5238 298 5.1502 298 8.02282V350.973C298 364.228 287.252 375.02 273.96 375.02H24.0402C10.7894 375.02 0 364.269 0 350.973V8.02282C0 5.19123 1.5179 2.56484 3.97935 1.12853C6.4408 -0.307781 9.4766 -0.307781 11.9791 1.08749H11.9381L11.897 1.04645Z" fill="#1e293b"/></svg>`;

function loaderHtml(message?: string): string {
  const msgHtml = message
    ? `<p class="mt-4 text-sm text-slate-300 animate-pulse">${message}</p>`
    : "";
  return `<div class="deadcat-loader"><div class="deadcat-loader-scene"><div class="deadcat-loader-clip"><div class="deadcat-loader-cat">${LOADER_CAT_SVG}</div></div><div class="deadcat-loader-bag">${LOADER_BAG_SVG}</div></div>${msgHtml}</div>`;
}

function showOverlayLoader(message?: string): void {
  if (document.getElementById("deadcat-overlay")) return;
  const el = document.createElement("div");
  el.id = "deadcat-overlay";
  el.className = "deadcat-overlay";
  el.innerHTML = loaderHtml(message);
  document.body.appendChild(el);
}

function hideOverlayLoader(): void {
  const el = document.getElementById("deadcat-overlay");
  if (!el) return;
  el.classList.add("fade-out");
  el.addEventListener("transitionend", () => el.remove(), { once: true });
}

function updateOverlayMessage(message: string): void {
  const el = document.getElementById("deadcat-overlay");
  if (!el) return;
  const msgEl = el.querySelector("p");
  if (msgEl) {
    msgEl.textContent = message;
  }
}

type NavCategory =
  | "Trending"
  | "Politics"
  | "Sports"
  | "Culture"
  | "Bitcoin"
  | "Weather"
  | "Macro";
type MarketCategory = Exclude<NavCategory, "Trending">;
type ViewMode = "home" | "detail" | "create" | "wallet";
type Side = "yes" | "no";
type OrderType = "market" | "limit";
type ActionTab = "trade" | "issue" | "redeem" | "cancel";
type CovenantState = 0 | 1 | 2 | 3;
type TradeIntent = "open" | "close";
type SizeMode = "sats" | "contracts";

type ResolveTx = {
  txid: string;
  outcome: Side;
  sigVerified: boolean;
  height: number;
  signatureHash: string;
};

type CollateralUtxo = {
  txid: string;
  vout: number;
  amountSats: number;
};

type WalletNetwork = "liquid" | "liquid-testnet" | "liquid-regtest";

type ChainTipResponse = {
  height: number;
  block_hash: string;
  timestamp: number;
};

// Boltz swap response types
type BoltzLightningReceiveCreated = {
  id: string;
  flow: string;
  invoiceAmountSat: number;
  expectedOnchainAmountSat: number;
  invoice: string;
  invoiceExpiresAt: string;
  invoiceExpirySeconds: number;
};

type BoltzSubmarineSwapCreated = {
  id: string;
  flow: string;
  invoiceAmountSat: number;
  expectedAmountSat: number;
  lockupAddress: string;
  bip21: string;
  invoiceExpiresAt: string;
  invoiceExpirySeconds: number;
};

type BoltzChainSwapCreated = {
  id: string;
  flow: string;
  amountSat: number;
  expectedAmountSat: number;
  lockupAddress: string;
  claimLockupAddress: string;
  timeoutBlockHeight: number;
  bip21: string | null;
};

type BoltzChainSwapPairInfo = {
  pairHash: string;
  minAmountSat: number;
  maxAmountSat: number;
  feePercentage: number;
  minerFeeLockupSat: number;
  minerFeeClaimSat: number;
  minerFeeServerSat: number;
  fixedMinerFeeTotalSat: number;
};

type BoltzChainSwapPairsInfo = {
  bitcoinToLiquid: BoltzChainSwapPairInfo;
  liquidToBitcoin: BoltzChainSwapPairInfo;
};

type PaymentSwap = {
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

type DiscoveredMarket = {
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
  starting_yes_price: number;
  creator_pubkey: string;
  created_at: number;
  creation_txid: string | null;
  state: CovenantState;
};

type IssuanceResult = {
  txid: string;
  previous_state: number;
  new_state: number;
  pairs_issued: number;
};

type IdentityResponse = { pubkey_hex: string; npub: string };

type RelayEntry = { url: string; has_backup: boolean };
type RelayBackupResult = { url: string; has_backup: boolean };
type NostrBackupStatus = {
  has_backup: boolean;
  relay_results: RelayBackupResult[];
};
type NostrProfile = { picture?: string; name?: string; display_name?: string };

type AttestationResult = {
  market_id: string;
  outcome_yes: boolean;
  signature_hex: string;
  nostr_event_id: string;
};

type Market = {
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
  yesPrice: number;
  change24h: number;
  volumeBtc: number;
  liquidityBtc: number;
};

type PathAvailability = {
  initialIssue: boolean;
  issue: boolean;
  resolve: boolean;
  redeem: boolean;
  expiryRedeem: boolean;
  cancel: boolean;
};

const EXECUTION_FEE_RATE = 0.01;
const WIN_FEE_RATE = 0.02;

const categories: NavCategory[] = [
  "Trending",
  "Politics",
  "Sports",
  "Culture",
  "Bitcoin",
  "Weather",
  "Macro",
];

let markets: Market[] = [];

function discoveredToMarket(d: DiscoveredMarket): Market {
  return {
    id: d.id,
    nevent: d.nevent,
    question: d.question,
    category: ([
      "Bitcoin",
      "Politics",
      "Sports",
      "Culture",
      "Weather",
      "Macro",
    ].includes(d.category)
      ? d.category
      : "Bitcoin") as MarketCategory,
    description: d.description,
    resolutionSource: d.resolution_source,
    isLive: d.state === 1,
    state: d.state,
    marketId: d.market_id,
    oraclePubkey: d.oracle_pubkey,
    expiryHeight: d.expiry_height,
    currentHeight: 0,
    cptSats: d.cpt_sats,
    collateralAssetId: d.collateral_asset_id,
    yesAssetId: d.yes_asset_id,
    noAssetId: d.no_asset_id,
    yesReissuanceToken: d.yes_reissuance_token,
    noReissuanceToken: d.no_reissuance_token,
    creationTxid: d.creation_txid,
    collateralUtxos: [],
    yesPrice: d.starting_yes_price / 100,
    change24h: 0,
    volumeBtc: 0,
    liquidityBtc: 0,
  };
}

async function loadMarkets(): Promise<void> {
  try {
    // 1. Fetch from Nostr
    const discovered = await invoke<DiscoveredMarket[]>("discover_contracts");
    // 2. Ingest into store (incompatible contracts silently dropped)
    await invoke("ingest_discovered_markets", { markets: discovered });
    // 3. Load from store (only compatible, compiled contracts with on-chain state)
    const stored = await invoke<DiscoveredMarket[]>("list_contracts");
    markets = stored.map(discoveredToMarket);
  } catch (error) {
    console.warn("Failed to discover contracts:", error);
    markets = [];
  }
  render();
}

function hexToBytes(hex: string): number[] {
  const bytes: number[] = [];
  for (let i = 0; i < hex.length; i += 2) {
    bytes.push(parseInt(hex.substring(i, i + 2), 16));
  }
  return bytes;
}

/** Reverse byte-order of a hex string (internal ↔ display order for hash-based IDs). */
function reverseHex(hex: string): string {
  return (hex.match(/.{2}/g) || []).reverse().join("");
}

function marketToContractParamsJson(market: Market): string {
  return JSON.stringify({
    oracle_public_key: hexToBytes(market.oraclePubkey),
    collateral_asset_id: hexToBytes(market.collateralAssetId),
    yes_token_asset: hexToBytes(market.yesAssetId),
    no_token_asset: hexToBytes(market.noAssetId),
    yes_reissuance_token: hexToBytes(market.yesReissuanceToken),
    no_reissuance_token: hexToBytes(market.noReissuanceToken),
    collateral_per_token: market.cptSats,
    expiry_time: market.expiryHeight,
  });
}

async function issueTokens(
  market: Market,
  pairs: number,
): Promise<IssuanceResult> {
  if (!market.creationTxid) {
    throw new Error("Market has no creation txid — cannot issue tokens");
  }
  return invoke<IssuanceResult>("issue_tokens", {
    contractParamsJson: marketToContractParamsJson(market),
    creationTxid: market.creationTxid,
    pairs,
  });
}

function defaultSettlementInput(): string {
  const inThirtyDays = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000);
  const year = inThirtyDays.getFullYear();
  const month = String(inThirtyDays.getMonth() + 1).padStart(2, "0");
  const day = String(inThirtyDays.getDate()).padStart(2, "0");
  const hours = String(inThirtyDays.getHours()).padStart(2, "0");
  const minutes = String(inThirtyDays.getMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}T${hours}:${minutes}`;
}

const state: {
  view: ViewMode;
  activeCategory: NavCategory;
  search: string;
  trendingIndex: number;
  selectedMarketId: string;
  selectedSide: Side;
  orderType: OrderType;
  actionTab: ActionTab;
  tradeIntent: TradeIntent;
  sizeMode: SizeMode;
  showAdvancedDetails: boolean;
  showAdvancedActions: boolean;
  showOrderbook: boolean;
  showFeeDetails: boolean;
  tradeSizeSats: number;
  tradeSizeSatsDraft: string;
  tradeContracts: number;
  tradeContractsDraft: string;
  limitPrice: number;
  limitPriceDraft: string;
  pairsInput: number;
  tokensInput: number;
  createQuestion: string;
  createDescription: string;
  createCategory: MarketCategory;
  createResolutionSource: string;
  createSettlementInput: string;
  createStartingYesSats: number;
  walletStatus: "not_created" | "locked" | "unlocked";
  walletNetwork: "mainnet" | "testnet" | "regtest";
  walletBalance: Record<string, number> | null;
  walletPolicyAssetId: string;
  walletMnemonic: string;
  walletTransactions: {
    txid: string;
    balanceChange: number;
    fee: number;
    height: number | null;
    timestamp: number | null;
    txType: string;
  }[];
  walletError: string;
  walletLoading: boolean;
  walletPassword: string;
  walletRestoreMnemonic: string;
  walletShowRestore: boolean;
  walletShowBackup: boolean;
  walletBackedUp: boolean;
  walletBackupMnemonic: string;
  walletBackupPassword: string;
  walletSwaps: PaymentSwap[];
  // Modal state
  walletModal: "none" | "receive" | "send";
  walletModalTab: "lightning" | "liquid" | "bitcoin";
  modalQr: string;
  // Receive modal
  receiveAmount: string;
  receiveCreating: boolean;
  receiveError: string;
  receiveLightningSwap: BoltzLightningReceiveCreated | null;
  receiveLiquidAddress: string;
  receiveLiquidAddressIndex: number;
  receiveBitcoinSwap: BoltzChainSwapCreated | null;
  receiveBtcPairInfo: BoltzChainSwapPairInfo | null;
  // Send modal
  sendInvoice: string;
  sendLiquidAddress: string;
  sendLiquidAmount: string;
  sendBtcAmount: string;
  sendCreating: boolean;
  sendError: string;
  sentLightningSwap: BoltzSubmarineSwapCreated | null;
  sentLiquidResult: { txid: string; feeSat: number } | null;
  sentBitcoinSwap: BoltzChainSwapCreated | null;
  sendBtcPairInfo: BoltzChainSwapPairInfo | null;
  userMenuOpen: boolean;
  searchOpen: boolean;
  walletUnit: "sats" | "btc";
  walletBalanceHidden: boolean;
  baseCurrency: BaseCurrency;
  helpOpen: boolean;
  settingsOpen: boolean;
  settingsSection: Record<string, boolean>;
  logoutOpen: boolean;
  nostrPubkey: string | null;
  nostrNpub: string | null;
  nostrNsecRevealed: string | null;
  nostrImportNsec: string;
  nostrImporting: boolean;
  nostrReplacePrompt: boolean;
  nostrReplacePanel: boolean;
  nostrReplaceConfirm: string;
  walletDeletePrompt: boolean;
  walletDeleteConfirm: string;
  devResetPrompt: boolean;
  devResetConfirm: string;
  // Nostr backup
  nostrBackupStatus: NostrBackupStatus | null;
  nostrBackupLoading: boolean;
  nostrBackupPassword: string;
  nostrBackupPrompt: boolean;
  // Relay management
  relays: RelayEntry[];
  relayInput: string;
  relayLoading: boolean;
  // Profile
  nostrProfile: NostrProfile | null;
  profilePicError: boolean;
  // Onboarding
  onboardingStep: "nostr" | "wallet" | null;
  onboardingNostrMode: "generate" | "import";
  onboardingNostrNsec: string;
  onboardingNostrGeneratedNsec: string;
  onboardingNsecRevealed: boolean;
  onboardingNostrDone: boolean;
  onboardingWalletMode: "create" | "restore" | "nostr-restore";
  onboardingWalletPassword: string;
  onboardingWalletMnemonic: string;
  onboardingError: string;
  onboardingLoading: boolean;
  onboardingBackupFound: boolean;
  onboardingBackupScanning: boolean;
  marketCreating: boolean;
  marketsLoading: boolean;
  lastAttestationSig: string | null;
  lastAttestationOutcome: boolean | null;
  lastAttestationMarketId: string | null;
  resolutionExecuting: boolean;
} = {
  view: "home",
  activeCategory: "Trending",
  search: "",
  trendingIndex: 0,
  selectedMarketId: "mkt-3",
  selectedSide: "yes",
  orderType: "limit",
  actionTab: "trade",
  tradeIntent: "open",
  sizeMode: "sats",
  showAdvancedDetails: false,
  showAdvancedActions: false,
  showOrderbook: false,
  showFeeDetails: false,
  tradeSizeSats: 10000,
  tradeSizeSatsDraft: "10,000",
  tradeContracts: 10,
  tradeContractsDraft: "10.00",
  limitPrice: 0.5,
  limitPriceDraft: "50",
  pairsInput: 10,
  tokensInput: 25,
  createQuestion: "",
  createDescription: "",
  createCategory: "Bitcoin",
  createResolutionSource: "",
  createSettlementInput: defaultSettlementInput(),
  createStartingYesSats: 50,
  walletStatus: "not_created",
  walletNetwork: "testnet",
  walletBalance: null,
  walletPolicyAssetId: "",
  walletMnemonic: "",
  walletTransactions: [],
  walletError: "",
  walletLoading: false,
  walletPassword: "",
  walletRestoreMnemonic: "",
  walletShowRestore: false,
  walletShowBackup: false,
  walletBackedUp: false,
  walletBackupMnemonic: "",
  walletBackupPassword: "",
  walletSwaps: [],
  walletModal: "none",
  walletModalTab: "lightning",
  modalQr: "",
  receiveAmount: "",
  receiveCreating: false,
  receiveError: "",
  receiveLightningSwap: null,
  receiveLiquidAddress: "",
  receiveLiquidAddressIndex: 0,
  receiveBitcoinSwap: null,
  receiveBtcPairInfo: null,
  sendInvoice: "",
  sendLiquidAddress: "",
  sendLiquidAmount: "",
  sendBtcAmount: "",
  sendCreating: false,
  sendError: "",
  sentLightningSwap: null,
  sentLiquidResult: null,
  sentBitcoinSwap: null,
  sendBtcPairInfo: null,
  userMenuOpen: false,
  searchOpen: false,
  walletUnit: "sats",
  walletBalanceHidden: false,
  baseCurrency: "BTC" as BaseCurrency,
  marketCreating: false,
  helpOpen: false,
  settingsOpen: false,
  settingsSection: {
    nostr: true,
    relays: false,
    wallet: false,
    dev: false,
  } as Record<string, boolean>,
  logoutOpen: false,
  nostrPubkey: null,
  nostrNpub: null,
  nostrNsecRevealed: null,
  nostrImportNsec: "",
  nostrImporting: false,
  nostrReplacePrompt: false,
  nostrReplacePanel: false,
  nostrReplaceConfirm: "",
  walletDeletePrompt: false,
  walletDeleteConfirm: "",
  devResetPrompt: false,
  devResetConfirm: "",
  nostrBackupStatus: null,
  nostrBackupLoading: false,
  nostrBackupPassword: "",
  nostrBackupPrompt: false,
  relays: [],
  relayInput: "",
  relayLoading: false,
  nostrProfile: null,
  profilePicError: false,
  onboardingStep: null,
  onboardingNostrMode: "generate",
  onboardingNostrNsec: "",
  onboardingNostrGeneratedNsec: "",
  onboardingNsecRevealed: false,
  onboardingNostrDone: false,
  onboardingWalletMode: "create",
  onboardingWalletPassword: "",
  onboardingWalletMnemonic: "",
  onboardingError: "",
  onboardingLoading: false,
  onboardingBackupFound: false,
  onboardingBackupScanning: false,
  marketsLoading: true,
  lastAttestationSig: null,
  lastAttestationOutcome: null,
  lastAttestationMarketId: null,
  resolutionExecuting: false,
};

// ── Toast notifications ──────────────────────────────────────────────
function showToast(
  message: string,
  kind: "success" | "error" | "info" = "info",
) {
  const el = document.createElement("div");
  const style =
    kind === "success"
      ? "border-emerald-500/50 text-emerald-300"
      : kind === "error"
        ? "border-red-500/50 text-red-300"
        : "border-slate-600 text-slate-300";
  el.className = `fixed bottom-6 left-1/2 -translate-x-1/2 z-[999] max-w-lg w-[90vw] px-4 py-3 rounded-lg border bg-slate-950 ${style} text-sm shadow-lg transition-opacity duration-300`;
  el.style.opacity = "0";
  el.style.userSelect = "text";
  el.style.wordBreak = "break-all";
  el.textContent = message;
  document.body.appendChild(el);
  requestAnimationFrame(() => (el.style.opacity = "1"));
  setTimeout(() => {
    el.style.opacity = "0";
    setTimeout(() => el.remove(), 300);
  }, 6000);
}

const SATS_PER_FULL_CONTRACT = 100;
const formatProbabilitySats = (price: number): string =>
  `${Math.round(price * SATS_PER_FULL_CONTRACT)} sats`;
const formatProbabilityWithPercent = (price: number): string =>
  `${Math.round(price * 100)}% (${formatProbabilitySats(price)})`;
const formatPercent = (value: number): string =>
  `${value >= 0 ? "+" : ""}${value.toFixed(1)}%`;
const formatSats = (value: number): string => `${value.toLocaleString()} sats`;
const formatSatsInput = (value: number): string =>
  Math.max(1, Math.floor(value)).toLocaleString("en-US");
const formatVolumeBtc = (value: number): string =>
  value >= 1000
    ? `${(value / 1000).toFixed(1)}K BTC`
    : `${value.toFixed(1)} BTC`;
const formatBlockHeight = (value: number): string =>
  value.toLocaleString("en-US");
const _dateFmtCache = new Map<string, Intl.DateTimeFormat>();
const _numFmtCache = new Map<string, Intl.NumberFormat>();
function cachedDateFmt(
  key: string,
  locale: string,
  opts: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  let f = _dateFmtCache.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, opts);
    _dateFmtCache.set(key, f);
  }
  return f;
}
function cachedNumFmt(
  key: string,
  locale: string,
  opts: Intl.NumberFormatOptions,
): Intl.NumberFormat {
  let f = _numFmtCache.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, opts);
    _numFmtCache.set(key, f);
  }
  return f;
}

const formatEstTime = (date: Date): string =>
  cachedDateFmt("est-time", "en-US", {
    timeZone: "America/New_York",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  })
    .format(date)
    .toLowerCase();
const formatSettlementDateTime = (date: Date): string =>
  `${cachedDateFmt("settlement", "en-US", {
    timeZone: "America/New_York",
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  }).format(date)} ET`;

// --- Currency conversion (matching astrolabe) ---

type BaseCurrency =
  | "BTC"
  | "USD"
  | "EUR"
  | "JPY"
  | "GBP"
  | "CNY"
  | "CHF"
  | "AUD"
  | "CAD";

const baseCurrencyOptions: BaseCurrency[] = [
  "BTC",
  "USD",
  "EUR",
  "JPY",
  "GBP",
  "CNY",
  "CHF",
  "AUD",
  "CAD",
];

const fxRates: Record<BaseCurrency, number> = {
  BTC: 97000,
  USD: 1,
  EUR: 1.08,
  JPY: 0.0067,
  GBP: 1.28,
  CNY: 0.14,
  CHF: 1.12,
  AUD: 0.65,
  CAD: 0.74,
};

function satsToFiat(sats: number, currency: BaseCurrency): number {
  const btcValue = sats / 100_000_000;
  const usdValue = btcValue * fxRates.BTC;
  return usdValue / fxRates[currency];
}

function formatFiat(value: number, currency: BaseCurrency): string {
  switch (currency) {
    case "USD":
      return cachedNumFmt("USD", "en-US", {
        style: "currency",
        currency: "USD",
      }).format(value);
    case "EUR":
      return cachedNumFmt("EUR", "de-DE", {
        style: "currency",
        currency: "EUR",
      }).format(value);
    case "GBP":
      return cachedNumFmt("GBP", "en-GB", {
        style: "currency",
        currency: "GBP",
      }).format(value);
    case "JPY":
      return cachedNumFmt("JPY", "ja-JP", {
        style: "currency",
        currency: "JPY",
        maximumFractionDigits: 0,
      }).format(value);
    case "CNY":
      return cachedNumFmt("CNY", "zh-CN", {
        style: "currency",
        currency: "CNY",
      }).format(value);
    case "CHF":
      return cachedNumFmt("CHF", "de-CH", {
        style: "currency",
        currency: "CHF",
      }).format(value);
    case "AUD":
      return cachedNumFmt("AUD", "en-AU", {
        style: "currency",
        currency: "AUD",
      }).format(value);
    case "CAD":
      return cachedNumFmt("CAD", "en-CA", {
        style: "currency",
        currency: "CAD",
      }).format(value);
    default:
      return "";
  }
}

function satsToFiatStr(sats: number): string {
  if (state.baseCurrency === "BTC") return "";
  return formatFiat(satsToFiat(sats, state.baseCurrency), state.baseCurrency);
}

function stateLabel(value: CovenantState): string {
  if (value === 0) return "DORMANT";
  if (value === 1) return "UNRESOLVED";
  if (value === 2) return "RESOLVED YES";
  return "RESOLVED NO";
}

function stateBadge(value: CovenantState): string {
  const label = stateLabel(value);
  const colors =
    value === 0
      ? "bg-slate-600/30 text-slate-300"
      : value === 1
        ? "bg-emerald-500/20 text-emerald-300"
        : value === 2
          ? "bg-emerald-500/30 text-emerald-200"
          : "bg-rose-500/30 text-rose-200";
  return `<span class="rounded-full px-2.5 py-0.5 text-xs font-medium ${colors}">${label}</span>`;
}

function isExpired(market: Market): boolean {
  return market.currentHeight >= market.expiryHeight;
}

function getEstimatedSettlementDate(market: Market): Date {
  const blocksRemaining = market.expiryHeight - market.currentHeight;
  const minutesPerBlock = 1;
  return new Date(Date.now() + blocksRemaining * minutesPerBlock * 60 * 1000);
}

function getPathAvailability(market: Market): PathAvailability {
  const expired = isExpired(market);
  return {
    initialIssue: market.state === 0,
    issue: market.state === 1 && !expired,
    resolve: market.state === 1 && !expired,
    redeem: market.state === 2 || market.state === 3,
    expiryRedeem: market.state === 1 && expired,
    cancel: market.state === 1,
  };
}

function getMarketById(marketId: string): Market {
  return markets.find((market) => market.id === marketId) ?? markets[0];
}

function getSelectedMarket(): Market {
  return getMarketById(state.selectedMarketId);
}

function getTrendingMarkets(): Market[] {
  return markets.slice(0, 7);
}

function clampContractPriceSats(value: number): number {
  return Math.max(1, Math.min(SATS_PER_FULL_CONTRACT - 1, Math.round(value)));
}

function getBasePriceSats(market: Market, side: Side): number {
  const raw = side === "yes" ? market.yesPrice : 1 - market.yesPrice;
  return clampContractPriceSats(raw * SATS_PER_FULL_CONTRACT);
}

function getMarketSeed(market: Market): number {
  return [...market.id].reduce((sum, ch) => sum + ch.charCodeAt(0), 0);
}

function getPositionContracts(market: Market): { yes: number; no: number } {
  if (!state.walletBalance) return { yes: 0, no: 0 };
  const yesKey = reverseHex(market.yesAssetId);
  const noKey = reverseHex(market.noAssetId);
  return {
    yes: state.walletBalance[yesKey] ?? 0,
    no: state.walletBalance[noKey] ?? 0,
  };
}

type OrderbookLevel = {
  priceSats: number;
  contracts: number;
};

function getOrderbookLevels(
  market: Market,
  side: Side,
  intent: TradeIntent,
): OrderbookLevel[] {
  const seed = getMarketSeed(market);
  const base = getBasePriceSats(market, side);
  return Array.from({ length: 8 }).map((_, idx) => {
    const offset = intent === "open" ? idx + 1 : -(idx + 1);
    const priceSats = clampContractPriceSats(base + offset);
    const contracts = 12 + ((seed + idx * 11) % 34);
    return { priceSats, contracts };
  });
}

type FillEstimate = {
  avgPriceSats: number;
  bestPriceSats: number;
  worstPriceSats: number;
  filledContracts: number;
  requestedContracts: number;
  totalSats: number;
  isPartial: boolean;
};

function estimateFill(
  levels: OrderbookLevel[],
  requestedContracts: number,
  intent: TradeIntent,
  orderType: OrderType,
  limitPriceSats: number,
): FillEstimate {
  const request = Math.max(0.01, requestedContracts);
  const executable = levels.filter((level) =>
    orderType === "market"
      ? true
      : intent === "open"
        ? level.priceSats <= limitPriceSats
        : level.priceSats >= limitPriceSats,
  );

  let remaining = request;
  let totalSats = 0;
  let totalContracts = 0;
  let bestPrice = executable[0]?.priceSats ?? limitPriceSats;
  let worstPrice = bestPrice;

  for (const level of executable) {
    if (remaining <= 0) break;
    const take = Math.min(remaining, level.contracts);
    totalContracts += take;
    totalSats += take * level.priceSats;
    worstPrice = level.priceSats;
    remaining -= take;
  }

  const avgPriceSats =
    totalContracts > 0 ? totalSats / totalContracts : limitPriceSats;

  return {
    avgPriceSats,
    bestPriceSats: bestPrice,
    worstPriceSats: worstPrice,
    filledContracts: totalContracts,
    requestedContracts: request,
    totalSats: Math.round(totalSats),
    isPartial: totalContracts + 0.0001 < request,
  };
}

type TradePreview = {
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

function getTradePreview(market: Market): TradePreview {
  const limitPriceSats = clampContractPriceSats(
    state.limitPrice * SATS_PER_FULL_CONTRACT,
  );
  const basePriceSats = getBasePriceSats(market, state.selectedSide);
  const levels = getOrderbookLevels(
    market,
    state.selectedSide,
    state.tradeIntent,
  );
  const referencePriceSats =
    state.orderType === "limit" ? limitPriceSats : basePriceSats;
  const requestedContracts =
    state.sizeMode === "contracts"
      ? Math.max(0.01, state.tradeContracts)
      : Math.max(1, state.tradeSizeSats) / Math.max(1, referencePriceSats);
  const fill = estimateFill(
    levels,
    requestedContracts,
    state.tradeIntent,
    state.orderType,
    limitPriceSats,
  );
  const executionPriceSats =
    state.orderType === "market"
      ? Math.max(1, fill.avgPriceSats)
      : limitPriceSats;
  const notionalSats =
    state.sizeMode === "sats"
      ? Math.max(1, Math.floor(state.tradeSizeSats))
      : Math.max(1, Math.round(requestedContracts * referencePriceSats));
  const executedSats = Math.max(0, fill.totalSats);
  const executionFeeSats = Math.round(executedSats * EXECUTION_FEE_RATE);
  const grossPayoutSats = Math.floor(
    fill.filledContracts * SATS_PER_FULL_CONTRACT,
  );
  const grossProfitSats = Math.max(0, grossPayoutSats - executedSats);
  const winFeeSats =
    state.tradeIntent === "open"
      ? Math.round(grossProfitSats * WIN_FEE_RATE)
      : 0;
  const netIfCorrectSats = Math.max(
    0,
    grossPayoutSats - executionFeeSats - winFeeSats,
  );
  const maxProfitSats = Math.max(0, netIfCorrectSats - executedSats);
  const netAfterFeesSats = Math.max(0, executedSats - executionFeeSats);
  const slippagePct =
    fill.bestPriceSats > 0
      ? Math.max(
          0,
          ((fill.worstPriceSats - fill.bestPriceSats) / fill.bestPriceSats) *
            100,
        )
      : 0;
  const position = getPositionContracts(market);
  const positionContracts =
    state.selectedSide === "yes" ? position.yes : position.no;

  return {
    basePriceSats,
    limitPriceSats,
    referencePriceSats,
    requestedContracts,
    fill,
    executionPriceSats,
    notionalSats,
    executedSats,
    executionFeeSats,
    winFeeSats,
    grossPayoutSats,
    netIfCorrectSats,
    maxProfitSats,
    netAfterFeesSats,
    slippagePct,
    positionContracts,
  };
}

function commitTradeSizeSatsDraft(): void {
  const sanitized = state.tradeSizeSatsDraft.replace(/,/g, "");
  const parsed = Math.floor(Number(sanitized) || 1);
  const clamped = Math.max(1, parsed);
  state.tradeSizeSats = clamped;
  state.tradeSizeSatsDraft = formatSatsInput(clamped);
}

function commitTradeContractsDraft(market: Market): void {
  const positions = getPositionContracts(market);
  const available = state.selectedSide === "yes" ? positions.yes : positions.no;
  const parsed = Number(state.tradeContractsDraft);
  const base = Number.isFinite(parsed) ? parsed : 0.01;
  const normalized = Math.max(0.01, base);
  const clamped =
    state.tradeIntent === "close"
      ? Math.min(normalized, available)
      : normalized;
  state.tradeContracts = clamped;
  state.tradeContractsDraft = clamped.toFixed(2);
}

function setLimitPriceSats(limitPriceSats: number): void {
  const clampedSats = clampContractPriceSats(limitPriceSats);
  state.limitPrice = clampedSats / SATS_PER_FULL_CONTRACT;
  state.limitPriceDraft = String(clampedSats);
}

function commitLimitPriceDraft(): void {
  const sanitized = state.limitPriceDraft.replace(/[^\d]/g, "");
  if (sanitized.length === 0) {
    state.limitPriceDraft = String(
      clampContractPriceSats(state.limitPrice * SATS_PER_FULL_CONTRACT),
    );
    return;
  }
  setLimitPriceSats(Math.floor(Number(sanitized)));
}

function getFilteredMarkets(): Market[] {
  const lowered = state.search.trim().toLowerCase();
  return markets
    .filter((market) => {
      const categoryMatch =
        state.activeCategory === "Trending" ||
        market.category === state.activeCategory;
      const searchMatch =
        lowered.length === 0 ||
        market.question.toLowerCase().includes(lowered) ||
        market.category.toLowerCase().includes(lowered);
      return categoryMatch && searchMatch;
    })
    .sort((a, b) => b.volumeBtc - a.volumeBtc);
}

function chartSkeleton(market: Market): string {
  // Outer silhouette only (no face details) for small chart markers
  const chartLogoPath =
    "M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605Z";
  const markerWidth = 4.8;
  const markerHeight = (markerWidth * 267) / 260;
  const markerAt = (x: number, y: number, fill: string): string => `
    <g transform="translate(${x - markerWidth / 2} ${y - markerHeight / 2}) scale(${markerWidth / 260} ${markerHeight / 267})">
      <path d="${chartLogoPath}" fill="${fill}" />
    </g>
  `;
  const legendIcon = (fill: string): string => `
    <svg viewBox="0 0 260 267" class="h-[11px] w-[11px] shrink-0" aria-hidden="true">
      <path d="${chartLogoPath}" fill="${fill}" />
    </svg>
  `;

  const yes = market.yesPrice;
  const now = new Date();
  const xLabels = [
    new Date(now.getTime() - 3 * 60 * 60 * 1000),
    new Date(now.getTime() - 2 * 60 * 60 * 1000),
    new Date(now.getTime() - 1 * 60 * 60 * 1000),
    now,
  ];
  const yesPoints: Array<{ x: number; y: number }> = [
    { x: 6, y: 62 - yes * 14 },
    { x: 20, y: 58 - yes * 11 },
    { x: 34, y: 60 - yes * 13 },
    { x: 50, y: 56 - yes * 10 },
    { x: 66, y: 58 - yes * 12 },
    { x: 78, y: 57 - yes * 11 },
    { x: 88, y: 55 - yes * 9 },
    { x: 96, y: 54 - yes * 8 },
  ];
  const noPoints = yesPoints.map((point) => ({ x: point.x, y: 92 - point.y }));

  const yesPath = yesPoints.map((point) => `${point.x},${point.y}`).join(" ");
  const noPath = noPoints.map((point) => `${point.x},${point.y}`).join(" ");
  const yesEnd = yesPoints[yesPoints.length - 1];
  const noEnd = noPoints[noPoints.length - 1];
  const yesPct = Math.round(yes * 100);
  const noPct = 100 - yesPct;

  return `
    <div class="chart-grid relative h-64 rounded-xl border border-slate-800 bg-slate-950/60 p-3">
      <div class="mb-2 flex items-center gap-4 text-xs text-slate-300">
        <span class="inline-flex items-center gap-1">${legendIcon("#5eead4")}Yes ${yesPct}%</span>
        <span class="inline-flex items-center gap-1">${legendIcon("#fb7185")}No ${noPct}%</span>
        <span class="text-slate-500">Yes + No = ${SATS_PER_FULL_CONTRACT} sats</span>
        ${
          market.isLive
            ? '<span class="inline-flex items-center gap-1 text-[10px] font-medium text-rose-400"><span class="liveIndicatorDot"></span>Live · Round 1</span>'
            : ""
        }
      </div>
      <div class="pointer-events-none absolute inset-x-3 top-10 bottom-8">
        <svg viewBox="0 0 100 100" class="h-full w-full">
          <polyline fill="none" stroke="#5eead4" stroke-width="${market.isLive ? "1.6" : "1.3"}" stroke-opacity="${market.isLive ? "1" : "0.7"}" points="${yesPath}" />
          <polyline fill="none" stroke="#fb7185" stroke-width="${market.isLive ? "1.6" : "1.3"}" stroke-opacity="${market.isLive ? "1" : "0.7"}" points="${noPath}" />
          ${
            market.isLive
              ? `<circle class="chartLivePulse chartLivePulseYes" cx="${yesEnd.x}" cy="${yesEnd.y}" r="1.8" />
          <circle class="chartLivePulse chartLivePulseNo" cx="${noEnd.x}" cy="${noEnd.y}" r="1.8" />`
              : ""
          }
          ${markerAt(yesEnd.x, yesEnd.y, "#5eead4")}
          ${markerAt(noEnd.x, noEnd.y, "#fb7185")}
        </svg>
        <div class="absolute text-[12px] font-semibold text-emerald-300" style="left: calc(${yesEnd.x}% - 56px); top: calc(${yesEnd.y}% - 8px)">Yes ${yesPct}%</div>
        <div class="absolute text-[12px] font-semibold text-rose-400" style="left: calc(${noEnd.x}% - 50px); top: calc(${noEnd.y}% - 8px)">No ${noPct}%</div>
      </div>
      <div class="pointer-events-none absolute inset-x-3 top-10 bottom-8 rounded-xl bg-[linear-gradient(transparent_24%,rgba(148,163,184,0.13)_25%,transparent_26%,transparent_49%,rgba(148,163,184,0.13)_50%,transparent_51%,transparent_74%,rgba(148,163,184,0.13)_75%,transparent_76%)]"></div>
      <div class="pointer-events-none absolute right-1 top-10 bottom-8 flex flex-col justify-between text-[11px] text-slate-500"><span>100%</span><span>75%</span><span>50%</span><span>25%</span><span>0%</span></div>
      <div class="pointer-events-none absolute inset-x-3 bottom-1 flex items-center justify-between text-[11px] text-slate-500"><span data-est-label data-offset-hours="3">${formatEstTime(xLabels[0])}</span><span data-est-label data-offset-hours="2">${formatEstTime(xLabels[1])}</span><span data-est-label data-offset-hours="1">${formatEstTime(xLabels[2])}</span><span data-est-label data-offset-hours="0">${formatEstTime(xLabels[3])}</span><span class="ml-2 text-[10px] uppercase tracking-wide text-slate-600">ET</span></div>
    </div>
  `;
}

function settingsAccordion(
  key: string,
  title: string,
  content: string,
): string {
  const open = state.settingsSection[key];
  return `<div class="rounded-lg border border-slate-800 overflow-hidden">
    <button data-action="toggle-settings-section" data-section="${key}" class="w-full flex items-center justify-between px-4 py-3 text-left transition-colors hover:bg-slate-900/50">
      <span class="text-xs font-medium uppercase tracking-wider text-slate-400">${title}</span>
      <svg class="h-4 w-4 text-slate-500 transition-transform duration-200 ${open ? "rotate-180" : ""}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/></svg>
    </button>
    ${open ? `<div class="px-4 pb-4 pt-3 border-t border-slate-800">${content}</div>` : ""}
  </div>`;
}

function renderTopShell(): string {
  return `
    <header class="relative z-30 border-b border-slate-800 bg-slate-950/80 backdrop-blur">
      <div class="phi-container py-4 lg:py-5">
        <div class="flex items-end gap-5">
          <button data-action="go-home" class="shrink-0 py-1 leading-none"><svg class="h-10" viewBox="0 0 1192 267" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.146484 9.04596C0.146625 1.23444 10.9146 -3.15996 16.7881 2.6983L86.5566 71.7335C100.141 68.0293 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0305 173.453 71.7364L243.212 2.71197C249.085 -3.1467 259.853 1.24702 259.854 9.05865V161.26C259.949 162.835 260 164.419 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C4.78859e-06 164.419 0.0506708 162.835 0.146484 161.26V9.04596ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.23 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.216 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z" fill="#34D399"/><path d="M819.739 240V81.5058H793.197V65.9501H863.361V81.5058H836.927V240H819.739Z" fill="#34D399"/><path d="M718.828 240L737.538 65.9501H773.11L791.711 240H774.089L769.629 190.831H740.91L736.559 240H718.828ZM742.433 175.275H768.215L764.842 138.398L760.056 81.5058H750.701L745.806 138.507L742.433 175.275Z" fill="#34D399"/><path d="M668.029 241.632C656.498 241.632 648.013 239.311 642.574 234.67C637.207 230.028 634.452 222.341 634.307 211.608C634.162 200.948 634.053 190.831 633.98 181.258C633.908 171.613 633.871 162.185 633.871 152.975C633.871 143.765 633.908 134.373 633.98 124.801C634.053 115.228 634.162 105.111 634.307 94.4508C634.452 83.9353 637.207 76.3568 642.574 71.7155C648.013 67.0016 656.498 64.6447 668.029 64.6447C679.56 64.6447 688.045 67.0016 693.484 71.7155C698.923 76.4293 701.86 84.0078 702.295 94.4508C702.44 98.2218 702.512 101.848 702.512 105.329C702.585 108.737 702.585 112.146 702.512 115.554C702.44 118.963 702.295 122.589 702.077 126.432H684.89C685.035 122.516 685.107 118.818 685.107 115.337C685.18 111.856 685.216 108.375 685.216 104.894C685.216 101.413 685.144 97.7505 684.999 93.9069C684.781 88.3953 683.403 84.6242 680.865 82.5936C678.327 80.4905 674.048 79.439 668.029 79.439C662.155 79.439 657.948 80.4905 655.41 82.5936C652.944 84.6242 651.639 88.3953 651.494 93.9069C651.276 105.293 651.095 115.772 650.95 125.345C650.805 134.845 650.733 144.091 650.733 153.084C650.733 162.004 650.805 171.25 650.95 180.823C651.095 190.323 651.276 200.803 651.494 212.261C651.639 217.772 652.981 221.58 655.519 223.683C658.057 225.786 662.227 226.837 668.029 226.837C674.411 226.837 678.907 225.786 681.518 223.683C684.201 221.58 685.651 217.772 685.869 212.261C686.014 208.925 686.086 205.661 686.086 202.47C686.086 199.207 686.05 195.581 685.978 191.592C685.905 187.531 685.796 182.745 685.651 177.233H702.948C703.238 184.195 703.383 190.36 703.383 195.726C703.455 201.02 703.383 206.314 703.165 211.608C702.73 222.341 699.757 230.028 694.245 234.67C688.806 239.311 680.067 241.632 668.029 241.632Z" fill="#34D399"/><path d="M544.569 240V65.9501H577.421C589.024 65.9501 597.364 68.1982 602.44 72.6945C607.517 77.1908 610.128 84.5879 610.273 94.8859C610.49 108.665 610.635 121.791 610.708 134.265C610.78 146.738 610.78 159.212 610.708 171.685C610.635 184.086 610.49 197.176 610.273 210.955C610.128 221.326 607.589 228.759 602.658 233.256C597.727 237.752 589.713 240 578.617 240H544.569ZM561.865 224.444H578.617C583.911 224.444 587.61 223.538 589.713 221.725C591.889 219.839 593.013 216.721 593.085 212.37C593.375 201.201 593.557 190.795 593.629 181.149C593.774 171.504 593.847 162.113 593.847 152.975C593.847 143.765 593.774 134.337 593.629 124.692C593.557 115.047 593.375 104.64 593.085 93.4717C593.013 89.1205 591.816 86.0384 589.495 84.2253C587.175 82.4123 583.15 81.5058 577.421 81.5058H561.865V224.444Z" fill="#34D399"/><path d="M452.611 240L471.322 65.9501H506.893L525.495 240H507.872L503.412 190.831H474.694L470.343 240H452.611ZM476.217 175.275H501.998L498.626 138.398L493.839 81.5058H484.484L479.589 138.507L476.217 175.275Z" fill="#34D399"/><path d="M383.097 240V65.9501H440.86V81.5058H400.393V144.708H438.684V160.263H400.393V224.444H440.86V240H383.097Z" fill="#34D399"/><path d="M292.162 240V65.9501H325.014C336.618 65.9501 344.958 68.1982 350.034 72.6945C355.111 77.1908 357.721 84.5879 357.866 94.8859C358.084 108.665 358.229 121.791 358.301 134.265C358.374 146.738 358.374 159.212 358.301 171.685C358.229 184.086 358.084 197.176 357.866 210.955C357.721 221.326 355.183 228.759 350.252 233.256C345.32 237.752 337.307 240 326.211 240H292.162ZM309.459 224.444H326.211C331.505 224.444 335.204 223.538 337.307 221.725C339.482 219.839 340.606 216.721 340.679 212.37C340.969 201.201 341.15 190.795 341.223 181.149C341.368 171.504 341.44 162.113 341.44 152.975C341.44 143.765 341.368 134.337 341.223 124.692C341.15 115.047 340.969 104.64 340.679 93.4717C340.606 89.1205 339.41 86.0384 337.089 84.2253C334.768 82.4123 330.744 81.5058 325.014 81.5058H309.459V224.444Z" fill="#34D399"/><path d="M955.193 65.9999V66.0497H1124.84V65.9999H1157.69C1169.29 66 1177.63 68.2403 1182.71 72.7402C1187.78 77.2301 1190.4 84.6298 1190.54 94.9296L1190.69 105.133C1190.82 115.224 1190.92 124.947 1190.97 134.3C1191.04 146.77 1191.04 159.25 1190.97 171.72C1190.9 184.12 1190.76 197.21 1190.54 210.99C1190.39 221.36 1187.85 228.79 1182.92 233.29C1177.99 237.78 1169.98 240.03 1158.88 240.03H921.143C910.053 240.03 902.033 237.79 897.103 233.29C892.173 228.79 889.643 221.36 889.493 210.99C889.273 197.21 889.123 184.12 889.053 171.72C888.983 159.25 888.983 146.78 889.053 134.31C889.133 121.84 889.273 108.71 889.493 94.9296C889.643 84.63 892.243 77.2204 897.323 72.7304C902.403 68.2405 910.743 65.99 922.342 65.9999H955.193Z" fill="#EF4444"/><path d="M1099.65 212V96.1899H1152.64V115.299H1121.37V143.527H1151.19V162.636H1121.37V192.891H1152.64V212H1099.65Z" fill="white"/><path d="M1035.46 212L1018.67 96.1899H1041.11L1049.65 154.24L1050.95 192.891H1057.9L1059.21 154.24L1067.75 96.1899H1090.19L1073.39 212H1035.46Z" fill="white"/><path d="M987.549 212V96.1898H1009.26V212H987.549Z" fill="white"/><path d="M928.173 212V96.1898H949.888V192.891H978.406V212H928.173Z" fill="white"/></svg></button>
          <nav class="flex shrink-0 items-baseline gap-5 whitespace-nowrap pb-0.5 text-base text-slate-400">
            <button data-action="go-home" class="${state.view === "home" || state.view === "detail" ? "font-medium text-slate-100" : "hover:text-slate-200"}">Markets</button>
            <button class="hover:text-slate-200">Live</button>
            <button class="hover:text-slate-200">Social</button>
            <button data-action="open-create-market" class="${state.view === "create" ? "font-medium text-slate-100" : "hover:text-slate-200"}">New Market</button>
          </nav>
          <div class="ml-auto flex shrink-0 items-center gap-2 pb-0.5">
            <input id="global-search" value="${state.search}" class="hidden h-9 w-[280px] rounded-full border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-300 transition focus:ring-2 lg:block xl:w-[380px]" placeholder="Trade on anything" />
            <button data-action="open-search" class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 lg:hidden">
              <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
            </button>
            <button data-action="open-wallet" class="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200">
              <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="4" width="22" height="16" rx="2" ry="2"/><line x1="1" y1="10" x2="23" y2="10"/></svg>
            </button>
            <div class="relative shrink-0">
              <button data-action="toggle-user-menu" class="flex h-9 w-9 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200 overflow-hidden">
                ${
                  state.nostrProfile?.picture && !state.profilePicError
                    ? `<img src="${state.nostrProfile.picture}" class="h-full w-full rounded-full object-cover" onerror="this.style.display='none';this.nextElementSibling.style.display='block'" /><svg style="display:none" class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`
                    : `<svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>`
                }
              </button>
              ${
                state.userMenuOpen
                  ? `<div class="absolute right-0 top-full z-50 mt-2 w-64 rounded-xl border border-slate-700 bg-slate-900 shadow-xl">
                ${
                  state.nostrNpub
                    ? `<div class="px-3 pb-1 pt-3">
                  <div class="mb-1.5 text-[11px] text-slate-500">Nostr Publishing ID</div>
                  <button data-action="copy-nostr-npub" class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition hover:bg-slate-800" title="Click to copy npub">
                    <span class="mono min-w-0 truncate text-xs text-slate-300">${state.nostrNpub}</span>
                    <svg class="h-3.5 w-3.5 shrink-0 text-slate-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                  </button>
                </div>`
                    : ""
                }
                <div class="px-3 pb-1 ${state.nostrNpub ? "pt-1 border-t border-slate-800" : "pt-3"}">
                  <div class="mb-1.5 text-[11px] text-slate-500">Display currency</div>
                  <div class="grid grid-cols-3 gap-1">
                    ${baseCurrencyOptions.map((c) => `<button data-action="set-currency" data-currency="${c}" class="rounded-md px-2 py-1 text-xs transition ${c === state.baseCurrency ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:bg-slate-800 hover:text-slate-200"}">${c}</button>`).join("")}
                  </div>
                </div>
                <div class="mt-1 border-t border-slate-800 py-1">
                  <button data-action="user-settings" class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-slate-300 hover:bg-slate-800 hover:text-slate-100">
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
                    Settings
                  </button>
                  <button data-action="user-logout" class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm text-slate-300 hover:bg-slate-800 hover:text-slate-100">
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
                    Log out
                  </button>
                </div>
              </div>`
                  : ""
              }
            </div>
          </div>
        </div>
      </div>
      <div class="border-t border-slate-800">
        <div class="phi-container py-2">
          <div id="category-row" class="flex items-center gap-1 overflow-x-auto whitespace-nowrap">
            ${categories
              .map((category) => {
                const active = state.activeCategory === category;
                return `<button data-category="${category}" class="rounded-full px-3 py-1.5 text-sm font-normal transition ${
                  active
                    ? "bg-slate-800/80 text-slate-100"
                    : "text-slate-500 hover:text-slate-300"
                }">${category}</button>`;
              })
              .join("")}
            <button data-action="open-help" class="ml-auto flex shrink-0 items-center gap-1.5 rounded-full px-3 py-1.5 text-sm font-normal text-slate-500 transition hover:text-slate-300">
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 11h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-5Zm0 0a9 9 0 1 1 18 0m0 0v5a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3Z"/><path d="M21 16v2a4 4 0 0 1-4 4h-5"/></svg>
              Help
            </button>
          </div>
        </div>
      </div>
    </header>
    ${
      state.searchOpen
        ? `<div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm lg:hidden">
      <div class="flex items-center gap-3 border-b border-slate-800 bg-slate-950 px-4 py-3">
        <input id="global-search-mobile" value="${state.search}" class="h-10 flex-1 rounded-full border border-slate-700 bg-slate-900 px-4 text-sm text-slate-200 outline-none ring-emerald-300 transition focus:ring-2" placeholder="Trade on anything" autofocus />
        <button data-action="close-search" class="shrink-0 text-sm text-slate-400 hover:text-slate-200">Cancel</button>
      </div>
    </div>`
        : ""
    }
    ${
      state.helpOpen
        ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Help</h2>
          <button data-action="close-help" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <p class="mt-4 text-sm text-slate-400">Help content coming soon.</p>
      </div>
    </div>`
        : ""
    }
    ${
      state.settingsOpen
        ? `<div class="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto bg-slate-950/80 backdrop-blur-sm py-8">
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8 my-auto">
        ${
          state.nostrReplacePanel
            ? `
        <div class="flex items-center justify-between">
          <button data-action="nostr-replace-back" class="flex items-center gap-1 text-sm text-slate-400 hover:text-slate-200 transition">
            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7"/></svg>
            Back
          </button>
          <button data-action="close-settings" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <div class="mt-5 space-y-5">
          <div>
            <p class="text-xs font-medium uppercase tracking-wider text-slate-500">${state.nostrNpub ? "Replace Nostr Keys" : "Set Up Nostr Identity"}</p>
            <p class="mt-1 text-xs text-slate-400">${state.nostrNpub ? "Your current identity will be permanently deleted. Choose how to set up your new identity." : "Import an existing key or generate a new one."}</p>
          </div>
          <div class="space-y-3">
            <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Import existing nsec</p>
            <div class="flex items-center gap-2">
              <input id="nostr-import-nsec" type="password" value="${state.nostrImportNsec}" placeholder="nsec1..." class="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono" />
              <button data-action="import-nostr-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.nostrImporting ? "disabled" : ""}>${state.nostrImporting ? "Importing..." : "Import"}</button>
            </div>
          </div>
          <div class="border-t border-slate-800 pt-4 space-y-3">
            <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Or generate a fresh keypair</p>
            <button data-action="generate-new-nostr-key" class="w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition">Generate New Keypair</button>
          </div>
        </div>
        `
            : `
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Settings</h2>
          <button data-action="close-settings" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <div class="mt-3 space-y-2">
          ${settingsAccordion(
            "nostr",
            "Nostr Identity",
            `
            <div class="space-y-3">
              <p class="text-xs text-slate-500">Used to publish markets and oracle attestations on Nostr.</p>
              <div class="space-y-2">
                <div class="flex items-center gap-2">
                  <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                    <div class="text-[10px] text-slate-500">npub (public)</div>
                    <div class="mono truncate text-xs text-slate-300">${state.nostrNpub ?? "Not initialized"}</div>
                  </div>
                  ${state.nostrNpub ? `<button data-action="copy-nostr-npub" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>` : ""}
                </div>
                ${
                  state.nostrNpub
                    ? `<div class="flex items-center gap-2">
                  <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                    <div class="text-[10px] text-slate-500">nsec (secret)</div>
                    ${
                      state.nostrNsecRevealed
                        ? `<div class="mono truncate text-xs text-rose-300">${state.nostrNsecRevealed}</div>`
                        : `<div class="text-xs text-slate-500">Hidden</div>`
                    }
                  </div>
                  ${
                    state.nostrNsecRevealed
                      ? `<button data-action="copy-nostr-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>`
                      : `<button data-action="reveal-nostr-nsec" class="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition">Reveal</button>`
                  }
                </div>`
                    : ""
                }
              </div>
              ${
                state.nostrNpub
                  ? `<div class="rounded-lg border border-amber-700/40 bg-amber-950/20 px-3 py-2">
                <p class="text-[11px] text-amber-300/90">Back up your nsec in a safe place — if lost, you cannot resolve markets you created.</p>
              </div>`
                  : ""
              }
              ${
                !state.nostrNpub
                  ? `<div class="space-y-3">
                    <div>
                      <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Import existing nsec</p>
                      <div class="mt-1 flex items-center gap-2">
                        <input id="nostr-import-nsec" type="password" value="${state.nostrImportNsec}" placeholder="nsec1..." class="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono" />
                        <button data-action="import-nostr-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.nostrImporting ? "disabled" : ""}>${state.nostrImporting ? "Importing..." : "Import"}</button>
                      </div>
                    </div>
                    <div class="border-t border-slate-800 pt-3">
                      <p class="text-[10px] font-medium uppercase tracking-wider text-slate-500">Or generate a fresh keypair</p>
                      <button data-action="generate-new-nostr-key" class="mt-1 w-full rounded-lg bg-emerald-400 px-4 py-2.5 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition">Generate New Keypair</button>
                    </div>
                  </div>`
                  : state.nostrReplacePrompt
                    ? `<div class="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
                      <p class="text-[11px] text-rose-300">This will permanently erase your current Nostr identity. Type <strong>DELETE</strong> to confirm.</p>
                      <div class="flex items-center gap-2">
                        <input id="nostr-replace-confirm" type="text" value="${state.nostrReplaceConfirm}" placeholder="Type DELETE" class="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase" autocomplete="off" />
                        <button data-action="nostr-replace-confirm" class="shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${state.nostrReplaceConfirm.trim().toUpperCase() === "DELETE" ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}" ${state.nostrReplaceConfirm.trim().toUpperCase() !== "DELETE" ? "disabled" : ""}>Continue</button>
                        <button data-action="nostr-replace-cancel" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                      </div>
                    </div>`
                    : `<button data-action="nostr-replace-start" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Replace Nostr Keys</button>`
              }
            </div>
          `,
          )}
          ${settingsAccordion(
            "wallet",
            "Wallet",
            `
            <div class="space-y-3">
              ${
                state.walletStatus === "not_created"
                  ? `<p class="text-xs text-slate-500">No wallet configured on this device.</p>
                   <button data-action="open-wallet" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Set Up Wallet</button>`
                  : `${
                      state.nostrNpub
                        ? `<div class="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">
                  <p class="text-[11px] font-medium uppercase tracking-wider text-slate-500">Nostr Relay Backup</p>
                  ${
                    state.nostrBackupStatus?.has_backup
                      ? `<div class="flex items-center gap-2">
                        <svg class="h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
                        <p class="text-xs text-emerald-400">Encrypted backup on ${state.nostrBackupStatus.relay_results.filter((r: RelayBackupResult) => r.has_backup).length} of ${state.nostrBackupStatus.relay_results.length} relays</p>
                      </div>
                      <div class="space-y-1">
                        ${state.nostrBackupStatus.relay_results
                          .map(
                            (
                              r: RelayBackupResult,
                            ) => `<div class="flex items-center gap-2 text-xs">
                          <span class="h-1.5 w-1.5 rounded-full ${r.has_backup ? "bg-emerald-400" : "bg-slate-600"}"></span>
                          <span class="mono text-slate-400">${r.url}</span>
                        </div>`,
                          )
                          .join("")}
                      </div>
                      ${
                        state.nostrBackupPrompt &&
                        state.walletStatus !== "unlocked"
                          ? `<div class="space-y-2">
                            <input id="settings-backup-password" type="password" maxlength="32" value="${state.nostrBackupPassword}" placeholder="Wallet password" class="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2" />
                            <div class="flex gap-2">
                              <button data-action="settings-backup-wallet" class="flex-1 rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Uploading..." : "Upload"}</button>
                              <button data-action="cancel-backup-prompt" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                            </div>
                          </div>`
                          : `<div class="flex gap-2">
                            <button data-action="settings-backup-wallet" class="flex-1 rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Uploading..." : "Re-upload to Relays"}</button>
                            <button data-action="delete-nostr-backup" class="shrink-0 rounded-lg border border-rose-700/40 px-3 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition" ${state.nostrBackupLoading ? "disabled" : ""}>Delete</button>
                          </div>`
                      }`
                      : `<p class="text-xs text-slate-400">Encrypt your recovery phrase with NIP-44 and store it on your Nostr relays. Only your nsec can decrypt it.</p>
                      ${
                        state.nostrBackupPrompt &&
                        state.walletStatus !== "unlocked"
                          ? `<div class="space-y-2">
                            <input id="settings-backup-password" type="password" maxlength="32" value="${state.nostrBackupPassword}" placeholder="Wallet password" class="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2" />
                            <div class="flex gap-2">
                              <button data-action="settings-backup-wallet" class="flex-1 rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Encrypting..." : "Upload"}</button>
                              <button data-action="cancel-backup-prompt" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                            </div>
                          </div>`
                          : `<button data-action="settings-backup-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${state.nostrBackupLoading ? "disabled" : ""}>${state.nostrBackupLoading ? "Encrypting..." : "Encrypt & Upload to Relays"}</button>`
                      }`
                  }
                  <details class="group">
                    <summary class="cursor-pointer text-[11px] text-slate-500 hover:text-slate-400 transition select-none">Why is this secure?</summary>
                    <div class="mt-2 space-y-1.5 text-[11px] text-slate-500">
                      <p><strong class="text-slate-400">NIP-44 encryption</strong> — Recovery phrase is encrypted using XChaCha20 + secp256k1 ECDH. Only your nsec can decrypt it.</p>
                      <p><strong class="text-slate-400">Self-encrypted</strong> — Encrypted to your own public key. Relay operators see only ciphertext.</p>
                      <p><strong class="text-slate-400">NIP-78 storage</strong> — Published as a kind 30078 addressable event, retrievable from any relay that has it.</p>
                      <p><strong class="text-slate-400">Relay redundancy</strong> — Sent to all your configured relays for resilience.</p>
                    </div>
                  </details>
                </div>`
                        : ""
                    }
                  <p class="text-xs text-slate-500">Remove the current wallet from this device. You can restore from a recovery phrase${state.nostrNpub ? " or Nostr backup" : ""}.</p>
                  ${
                    state.walletDeletePrompt
                      ? `<div class="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
                        <p class="text-[11px] text-rose-300">This will permanently remove your wallet. Type <strong>DELETE</strong> to confirm.</p>
                        <div class="flex items-center gap-2">
                          <input id="wallet-delete-confirm" type="text" value="${state.walletDeleteConfirm}" placeholder="Type DELETE" class="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase" autocomplete="off" />
                          <button data-action="wallet-delete-confirm" class="shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${state.walletDeleteConfirm.trim().toUpperCase() === "DELETE" ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}" ${state.walletDeleteConfirm.trim().toUpperCase() !== "DELETE" ? "disabled" : ""}>Continue</button>
                          <button data-action="wallet-delete-cancel" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                        </div>
                      </div>`
                      : `<button data-action="wallet-delete-start" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Remove Wallet</button>`
                  }`
              }
            </div>
          `,
          )}
          ${settingsAccordion(
            "relays",
            "Relays",
            `
            <div class="space-y-3">
              <p class="text-xs text-slate-500">Nostr relays used for publishing and fetching data.</p>
              <div class="space-y-1.5">
                ${state.relays
                  .map(
                    (
                      relay: RelayEntry,
                    ) => `<div class="flex items-center gap-2 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                  <div class="min-w-0 flex-1 truncate text-xs text-slate-300 mono">${relay.url}</div>
                  ${relay.has_backup ? `<svg class="h-3.5 w-3.5 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>` : ""}
                  ${
                    state.relays.length > 1
                      ? `<button data-action="remove-relay" data-relay="${relay.url}" class="shrink-0 text-slate-500 hover:text-rose-400 transition">
                    <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
                  </button>`
                      : ""
                  }
                </div>`,
                  )
                  .join("")}
              </div>
              <div class="flex items-center gap-2">
                <input id="relay-input" value="${state.relayInput}" placeholder="wss://relay.example.com" class="h-9 min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2 mono" />
                <button data-action="add-relay" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${state.relayLoading ? "disabled" : ""}>Add</button>
              </div>
              <button data-action="reset-relays" class="text-[10px] text-slate-500 hover:text-slate-300 transition">Reset to defaults</button>
            </div>
          `,
          )}
          ${
            DEV_MODE
              ? settingsAccordion(
                  "dev",
                  "Dev",
                  `
            <div class="space-y-2">
              <button data-action="dev-restart" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Restart App</button>
              ${
                state.devResetPrompt
                  ? `<div class="rounded-lg border border-rose-700/40 bg-rose-950/20 p-3 space-y-2">
                    <p class="text-[11px] text-rose-300">This will erase your <strong>Nostr identity</strong> and <strong>wallet</strong>. Type <strong>RESET</strong> to confirm.</p>
                    <div class="flex items-center gap-2">
                      <input id="dev-reset-confirm" type="text" value="${state.devResetConfirm}" placeholder="Type RESET" class="h-9 min-w-0 flex-1 rounded-lg border border-rose-700/40 bg-slate-900 px-3 text-xs text-rose-300 outline-none ring-rose-400 transition focus:ring-2 uppercase" autocomplete="off" />
                      <button data-action="dev-reset-confirm" class="shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${state.devResetConfirm.trim().toUpperCase() === "RESET" ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}" ${state.devResetConfirm.trim().toUpperCase() !== "RESET" ? "disabled" : ""}>Confirm</button>
                      <button data-action="dev-reset-cancel" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
                    </div>
                  </div>`
                  : `<button data-action="dev-reset-start" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Erase All App Data</button>`
              }
            </div>
          `,
                )
              : ""
          }
        </div>
        `
        }
      </div>
    </div>`
        : ""
    }
    ${
      state.logoutOpen
        ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Log Out</h2>
          <button data-action="close-logout" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <div class="mt-5 space-y-4">
          <div class="rounded-xl border border-slate-700 bg-slate-900/60 p-4">
            <p class="text-sm font-medium text-slate-200">Before you log out, make sure you have:</p>
            <ul class="mt-3 space-y-2 text-sm text-slate-400">
              <li class="flex items-start gap-2">
                <svg class="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/></svg>
                <span>Backed up your <strong class="text-slate-200">recovery phrase</strong> — this is the only way to restore your wallet</span>
              </li>
              <li class="flex items-start gap-2">
                <svg class="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/></svg>
                <span>Saved your <strong class="text-slate-200">unlock password</strong> — you'll need it to access your wallet again</span>
              </li>
            </ul>
          </div>
          <p class="text-xs text-slate-500"><strong class="text-slate-300">Deadcat.live does not hold user funds.</strong> If you lose your recovery phrase and password, your funds cannot be recovered.</p>
          <div class="flex gap-3">
            <button data-action="close-logout" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Cancel</button>
            <button data-action="confirm-logout" class="flex-1 rounded-xl bg-rose-500/20 py-2.5 text-sm font-medium text-rose-300 transition hover:bg-rose-500/30">Log Out</button>
          </div>
        </div>
      </div>
    </div>`
        : ""
    }
    ${renderBackupModal(state.walletLoading)}
  `;
}

function renderHome(): string {
  if (state.activeCategory !== "Trending") {
    return renderCategoryPage();
  }

  if (state.marketsLoading) {
    return `
      <div class="phi-container py-16 text-center">
        <p class="text-lg text-slate-400">Discovering markets from Nostr relays...</p>
      </div>
    `;
  }

  const trending = getTrendingMarkets();

  if (trending.length === 0) {
    return `
      <div class="phi-container py-16 text-center">
        <h2 class="mb-3 text-2xl font-semibold text-slate-100">No markets discovered</h2>
        <p class="mb-6 text-base text-slate-400">Be the first to create a prediction market on Liquid Testnet.</p>
        ${
          state.walletStatus !== "unlocked"
            ? `
          <p class="mb-4 text-sm text-amber-300">Set up your wallet first to start trading</p>
          <button data-action="nav-wallet" class="mr-3 rounded-xl border border-slate-600 px-6 py-3 text-base font-medium text-slate-200">Set Up Wallet</button>
        `
            : ""
        }
        <button data-action="open-create-market" class="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950">Create New Market</button>
        ${state.nostrPubkey ? `<p class="mt-4 text-xs text-slate-500">Identity: ${state.nostrPubkey.slice(0, 8)}...${state.nostrPubkey.slice(-8)}</p>` : ""}
      </div>
    `;
  }

  const featured = trending[state.trendingIndex % trending.length];
  const featuredNo = 1 - featured.yesPrice;
  const topMarkets = getFilteredMarkets().slice(0, 6);
  const topMovers = [...markets]
    .sort((a, b) => Math.abs(b.change24h) - Math.abs(a.change24h))
    .slice(0, 3);

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/60 p-[21px] lg:p-[34px]">
            <div class="mb-5 flex items-start justify-between gap-3">
              <h1 class="phi-title text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${featured.question}</h1>
              <div class="flex items-center gap-2">
                <button data-action="trending-prev" class="h-11 w-11 rounded-full border border-slate-700 text-xl text-slate-200">&#8249;</button>
                <p class="w-20 text-center text-sm font-normal text-slate-300">${state.trendingIndex + 1} of ${trending.length}</p>
                <button data-action="trending-next" class="h-11 w-11 rounded-full border border-slate-700 text-xl text-slate-200">&#8250;</button>
              </div>
            </div>

            <div class="grid gap-[21px] lg:grid-cols-[1fr_1.618fr]">
              <div>
                <p class="mb-3 text-sm font-medium ${featured.isLive ? "text-rose-300" : "text-slate-400"}">${featured.isLive ? "Live" : "Scheduled"}</p>
                <div class="mb-3 grid grid-cols-2 gap-2 text-xs text-slate-400">
                  <div class="rounded-lg bg-slate-900/60 p-2">State<br/><span class="text-slate-200">${stateLabel(featured.state)}</span></div>
                  <div class="rounded-lg bg-slate-900/60 p-2">Volume<br/><span class="text-slate-200">${formatVolumeBtc(featured.volumeBtc)}</span></div>
                </div>
                <div class="space-y-3 text-lg text-slate-200">
                  <div class="flex items-center justify-between"><span>Yes contract</span><button data-open-market="${featured.id}" data-open-side="yes" data-open-intent="buy" class="rounded-full border border-emerald-600 px-4 py-1 text-emerald-300 transition hover:bg-emerald-500/10">${formatProbabilityWithPercent(featured.yesPrice)}</button></div>
                  <div class="flex items-center justify-between"><span>No contract</span><button data-open-market="${featured.id}" data-open-side="no" data-open-intent="buy" class="rounded-full border border-rose-600 px-4 py-1 text-rose-300 transition hover:bg-rose-500/10">${formatProbabilityWithPercent(featuredNo)}</button></div>
                </div>
                <p class="mt-3 text-[15px] text-slate-400">${featured.description}</p>
                <button data-open-market="${featured.id}" class="mt-5 rounded-xl bg-emerald-300 px-5 py-2.5 text-base font-medium text-slate-950">Open contract</button>
              </div>
              <div>${chartSkeleton(featured)}</div>
            </div>
          </div>

          <section>
            <div class="mb-3 flex items-center justify-between">
              <h2 class="text-base font-medium text-slate-400">Top Markets</h2>
              <p class="text-sm text-slate-400">${topMarkets.length} shown</p>
            </div>
            <div class="grid gap-3 md:grid-cols-2">
              ${topMarkets
                .map((market) => {
                  const no = 1 - market.yesPrice;
                  return `
                    <button data-open-market="${market.id}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
                      <p class="mb-2 text-xs text-slate-500">${market.category} ${market.isLive ? "· LIVE" : ""}</p>
                      <p class="mb-3 max-h-14 overflow-hidden text-base font-normal text-slate-200">${market.question}</p>
                      <div class="flex items-center justify-between text-xs sm:text-sm">
                        <span class="text-emerald-300">Yes ${formatProbabilityWithPercent(market.yesPrice)}</span>
                        <span class="text-rose-300">No ${formatProbabilityWithPercent(no)}</span>
                        <span class="${market.change24h >= 0 ? "text-emerald-300" : "text-rose-300"}">${formatPercent(market.change24h)}</span>
                      </div>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>
        </section>

        <aside class="space-y-[13px]">
          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 text-base font-medium text-slate-400">Trending</h3>
            <div class="space-y-4">
              ${trending
                .slice(0, 3)
                .map((market, idx) => {
                  return `
                    <button data-open-market="${market.id}" class="w-full text-left">
                      <div class="flex items-start justify-between gap-2">
                        <p class="w-full text-sm font-normal text-slate-300">${idx + 1}. ${market.question}</p>
                        <p class="text-sm font-normal text-slate-100">${Math.round(market.yesPrice * 100)}%</p>
                      </div>
                      <p class="mt-1 text-xs text-slate-500">${market.category}</p>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 text-base font-medium text-slate-400">Top movers</h3>
            <div class="space-y-4">
              ${topMovers
                .map((market, idx) => {
                  return `
                    <button data-open-market="${market.id}" class="w-full text-left">
                      <div class="flex items-start justify-between gap-2">
                        <p class="w-full text-sm font-normal text-slate-300">${idx + 1}. ${market.question}</p>
                        <p class="text-sm font-normal ${market.change24h >= 0 ? "text-emerald-300" : "text-rose-300"}">${formatPercent(market.change24h)}</p>
                      </div>
                      <p class="mt-1 text-xs text-slate-500">${market.category}</p>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>
        </aside>
      </div>
    </div>
  `;
}

function renderCategoryPage(): string {
  const category = state.activeCategory as MarketCategory;
  const categoryMarkets = getFilteredMarkets();
  const liveContracts = categoryMarkets
    .filter((market) => market.isLive)
    .slice(0, 4);
  const highestLiquidity = [...categoryMarkets]
    .sort((a, b) => b.liquidityBtc - a.liquidityBtc)
    .slice(0, 4);
  const stateMix = categoryMarkets.reduce(
    (acc, market) => {
      acc[market.state] += 1;
      return acc;
    },
    { 0: 0, 1: 0, 2: 0, 3: 0 } as Record<CovenantState, number>,
  );

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="grid gap-[21px] xl:grid-cols-[233px_1fr_320px]">
        <aside class="hidden xl:block">
          <div class="space-y-1 text-sm text-slate-400">
            <button class="block w-full rounded-md bg-slate-900/70 px-2 py-2 text-left text-emerald-300">All markets</button>
            <button class="block w-full rounded-md px-2 py-2 text-left hover:bg-slate-900/40 hover:text-slate-200">Live now</button>
            <button class="block w-full rounded-md px-2 py-2 text-left hover:bg-slate-900/40 hover:text-slate-200">Resolved soon</button>
          </div>
        </aside>
        <section>
          <div class="mb-4 flex items-center justify-between">
            <h1 class="text-xl font-medium text-slate-100">${category}</h1>
            <div class="flex items-center gap-2 text-sm text-slate-400">
              <button class="rounded-full border border-slate-700 px-3 py-1.5">Trending</button>
              <button class="rounded-full border border-slate-700 px-3 py-1.5">Frequency</button>
            </div>
          </div>
          <div class="mb-4 grid gap-2 sm:grid-cols-3">
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs text-slate-500">Contracts</p>
              <p class="text-lg font-medium text-slate-100">${categoryMarkets.length}</p>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs text-slate-500">Live now</p>
              <p class="text-lg font-medium text-rose-300">${liveContracts.length}</p>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs text-slate-500">24h volume</p>
              <p class="text-lg font-medium text-slate-100">${formatVolumeBtc(
                categoryMarkets.reduce(
                  (sum, market) => sum + market.volumeBtc,
                  0,
                ),
              )}</p>
            </div>
          </div>
          <div class="grid gap-3 md:grid-cols-2">
            ${categoryMarkets
              .map((market) => {
                const no = 1 - market.yesPrice;
                return `
                  <button data-open-market="${market.id}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
                    <div class="mb-2 flex items-center justify-between text-sm">
                      <span class="text-xs text-slate-500">${market.category}</span>
                      <span class="${market.isLive ? "text-rose-300" : "text-slate-500"}">${market.isLive ? "LIVE" : "SCHEDULED"}</span>
                    </div>
                    <p class="mb-3 text-base font-normal text-slate-200">${market.question}</p>
                    <div class="flex items-center justify-between text-sm">
                      <span class="text-emerald-300">Yes ${formatProbabilityWithPercent(market.yesPrice)}</span>
                      <span class="text-rose-300">No ${formatProbabilityWithPercent(no)}</span>
                      <span class="${market.change24h >= 0 ? "text-emerald-300" : "text-rose-300"}">${formatPercent(market.change24h)}</span>
                    </div>
                    <p class="mt-2 text-xs text-slate-500">Volume ${formatVolumeBtc(market.volumeBtc)} · ${market.description}</p>
                  </button>
                `;
              })
              .join("")}
          </div>
        </section>
        <aside class="space-y-3">
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-sm font-medium text-slate-400">Live contracts</h3>
            <div class="space-y-3">
              ${
                liveContracts.length
                  ? liveContracts
                      .map(
                        (market) => `
                      <button data-open-market="${market.id}" class="w-full text-left">
                        <p class="text-sm font-normal text-slate-300">${market.question}</p>
                        <p class="mt-1 text-xs text-slate-500">Yes ${Math.round(
                          market.yesPrice * 100,
                        )}% · ${formatVolumeBtc(market.volumeBtc)} volume</p>
                      </button>`,
                      )
                      .join("")
                  : '<p class="text-sm text-slate-500">No live contracts in this category right now.</p>'
              }
            </div>
          </section>
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-sm font-medium text-slate-400">Highest liquidity</h3>
            <div class="space-y-3">
              ${highestLiquidity
                .map(
                  (market, idx) => `
                <button data-open-market="${market.id}" class="flex w-full items-start justify-between gap-2 text-left">
                  <p class="text-sm text-slate-300">${idx + 1}. ${market.question}</p>
                  <p class="text-sm font-normal text-emerald-300">${formatVolumeBtc(market.liquidityBtc)}</p>
                </button>`,
                )
                .join("")}
            </div>
          </section>
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-sm font-medium text-slate-400">State mix</h3>
            <div class="space-y-2 text-sm text-slate-300">
              <p class="flex items-center justify-between"><span>State 0 · Uninitialized</span><span>${stateMix[0]}</span></p>
              <p class="flex items-center justify-between"><span>State 1 · Unresolved</span><span>${stateMix[1]}</span></p>
              <p class="flex items-center justify-between"><span>State 2 · Resolved YES</span><span>${stateMix[2]}</span></p>
              <p class="flex items-center justify-between"><span>State 3 · Resolved NO</span><span>${stateMix[3]}</span></p>
            </div>
          </section>
        </aside>
      </div>
    </div>
  `;
}

function renderPathCard(
  label: string,
  enabled: boolean,
  formula: string,
  next: string,
): string {
  return `<div class="rounded-lg border px-3 py-2 ${
    enabled
      ? "border-slate-700 bg-slate-900/50 text-slate-300"
      : "border-slate-800 bg-slate-950/60 text-slate-400"
  }">
    <div class="mb-1 flex items-center justify-between gap-2">
      <p class="text-sm font-medium">${label}</p>
      <span class="status-chip ${enabled ? "border border-emerald-500/40 bg-emerald-500/10 text-emerald-300" : "border border-slate-700 bg-slate-800/60 text-slate-400"}">${enabled ? "Available" : "Locked"}</span>
    </div>
    <p class="text-xs">${formula}</p>
    <p class="mt-1 text-xs">${next}</p>
  </div>`;
}

function renderActionTicket(market: Market): string {
  const paths = getPathAvailability(market);
  const preview = getTradePreview(market);
  const executionPriceSats = Math.round(preview.executionPriceSats);
  const positions = getPositionContracts(market);
  const selectedPositionContracts =
    state.selectedSide === "yes" ? positions.yes : positions.no;
  const maxCloseContracts = Math.max(0.01, selectedPositionContracts);
  const ctaVerb = state.tradeIntent === "open" ? "Buy" : "Sell";
  const ctaTarget = state.selectedSide === "yes" ? "Yes" : "No";
  const ctaLabel = `${ctaVerb} ${ctaTarget}`;
  const fillabilityLabel =
    state.orderType === "limit"
      ? preview.fill.filledContracts <= 0
        ? "Resting only (not fillable now)"
        : preview.fill.isPartial
          ? "Partially fillable now"
          : "Fully fillable now"
      : preview.fill.isPartial
        ? "May partially fill"
        : "Expected to fill now";

  const issueCollateral = state.pairsInput * 2 * market.cptSats;
  const cancelCollateral = state.pairsInput * 2 * market.cptSats;
  const redeemRate = paths.redeem
    ? 2 * market.cptSats
    : paths.expiryRedeem
      ? market.cptSats
      : 0;
  const redeemCollateral = state.tokensInput * redeemRate;
  const yesDisplaySats = clampContractPriceSats(
    Math.round(market.yesPrice * SATS_PER_FULL_CONTRACT),
  );
  const noDisplaySats = SATS_PER_FULL_CONTRACT - yesDisplaySats;
  const estimatedExecutionFeeSats = Math.round(
    preview.notionalSats * EXECUTION_FEE_RATE,
  );
  const estimatedGrossPayoutSats = Math.floor(
    preview.requestedContracts * SATS_PER_FULL_CONTRACT,
  );
  const estimatedProfitSats = Math.max(
    0,
    estimatedGrossPayoutSats - preview.notionalSats,
  );
  const estimatedWinFeeSats =
    state.tradeIntent === "open"
      ? Math.round(estimatedProfitSats * WIN_FEE_RATE)
      : 0;
  const estimatedFeesSats = estimatedExecutionFeeSats + estimatedWinFeeSats;
  const estimatedNetIfCorrectSats = Math.max(
    0,
    estimatedGrossPayoutSats - estimatedFeesSats,
  );
  return `
    <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
      <p class="panel-subtitle">Contract Action Ticket</p>
      <p class="mb-3 mt-1 text-sm text-slate-300">Buy or sell with a cleaner ticket flow. Advanced covenant actions are below.</p>
      <div class="mb-3 flex items-center justify-between gap-3 border-b border-slate-800 pb-3">
        <div class="flex items-center gap-4">
          <button data-trade-intent="open" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Buy</button>
          <button data-trade-intent="close" class="border-b-2 pb-1 text-xl font-medium ${state.tradeIntent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Sell</button>
        </div>
        <div class="flex items-center gap-2 rounded-lg border border-slate-700 p-1">
          <button data-order-type="market" class="rounded px-3 py-1 text-sm ${state.orderType === "market" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Market</button>
          <button data-order-type="limit" class="rounded px-3 py-1 text-sm ${state.orderType === "limit" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Limit</button>
        </div>
      </div>
      <div class="mb-3 grid grid-cols-2 gap-2">
        <button data-side="yes" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "yes" ? (state.tradeIntent === "open" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">Yes ${yesDisplaySats} sats</button>
        <button data-side="no" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "no" ? (state.tradeIntent === "open" ? "border-rose-400 bg-rose-400/20 text-rose-200" : "border-slate-400 bg-slate-400/15 text-slate-200") : "border-slate-700 text-slate-300"}">No ${noDisplaySats} sats</button>
      </div>
      <div class="mb-3 flex items-center justify-between gap-2">
        <label class="text-xs text-slate-400">Amount</label>
        <div class="grid grid-cols-2 gap-2">
          <button data-size-mode="sats" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "sats" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">sats</button>
          <button data-size-mode="contracts" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "contracts" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">contracts</button>
        </div>
      </div>
      ${
        state.sizeMode === "sats"
          ? `
      <input id="trade-size-sats" type="text" inputmode="numeric" value="${state.tradeSizeSatsDraft}" class="mb-2 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
      `
          : `
      <input id="trade-size-contracts" type="number" min="0.01" max="${state.tradeIntent === "close" ? maxCloseContracts.toFixed(2) : "9999"}" step="0.01" value="${state.tradeContractsDraft}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
      ${
        state.tradeIntent === "close"
          ? `<div class="mb-3 flex items-center gap-2 text-sm">
        <button data-action="sell-25" class="rounded border border-slate-700 px-3 py-1 text-slate-300">25%</button>
        <button data-action="sell-50" class="rounded border border-slate-700 px-3 py-1 text-slate-300">50%</button>
        <button data-action="sell-max" class="rounded border border-slate-700 px-3 py-1 text-slate-300">Max</button>
      </div>`
          : ""
      }
      `
      }
      ${
        state.orderType === "limit"
          ? `
      <label for="limit-price" class="mb-1 block text-xs text-slate-400">Limit price (sats)</label>
      <div class="mb-3 grid grid-cols-[42px_1fr_42px] gap-2">
        <button data-action="step-limit-price" data-limit-price-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease limit price">-</button>
        <input id="limit-price" type="text" inputmode="numeric" pattern="[0-9]*" maxlength="2" value="${state.limitPriceDraft}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-limit-price" data-limit-price-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase limit price">+</button>
      </div>
      <p class="mb-3 text-xs text-slate-500">May not fill immediately; unfilled size rests on book. ${fillabilityLabel}. Matchable now: ${formatSats(preview.executedSats)}.</p>
      `
          : `<p class="mb-3 text-xs text-slate-500">Estimated avg fill: ${preview.fill.avgPriceSats.toFixed(1)} sats (range ${preview.fill.bestPriceSats}-${preview.fill.worstPriceSats}).</p>`
      }
      <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
        ${
          state.tradeIntent === "open"
            ? `<div class="flex items-center justify-between py-1"><span>You pay</span><span>${formatSats(preview.notionalSats)}</span></div>
        <div class="flex items-center justify-between py-1"><span>If filled & correct</span><span>${formatSats(estimatedNetIfCorrectSats)}</span></div>`
            : `<div class="flex items-center justify-between py-1"><span>You receive (if filled)</span><span>${formatSats(Math.max(0, preview.notionalSats - estimatedExecutionFeeSats))}</span></div>
        <div class="flex items-center justify-between py-1"><span>Position remaining (if filled)</span><span>${Math.max(0, selectedPositionContracts - preview.requestedContracts).toFixed(2)} contracts</span></div>`
        }
        <div class="flex items-center justify-between py-1"><span>Estimated fees</span><span>${formatSats(estimatedFeesSats)}</span></div>
        <div class="mt-1 flex items-center justify-between py-1 text-xs text-slate-500"><span>Price</span><span>${executionPriceSats} sats · Yes + No = ${SATS_PER_FULL_CONTRACT}</span></div>
      </div>
      <button data-action="submit-trade" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950">${ctaLabel}</button>
      <div class="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>You hold: YES ${positions.yes.toFixed(2)} · NO ${positions.no.toFixed(2)}</span>
        ${
          state.tradeIntent === "close"
            ? `<button data-action="sell-max" class="rounded border border-slate-700 px-2 py-1 text-slate-300">Sell max</button>`
            : ""
        }
      </div>
      ${
        state.tradeIntent === "close" &&
        preview.requestedContracts > selectedPositionContracts + 0.0001
          ? `<p class="mt-2 text-xs text-rose-300">Requested size exceeds your current ${state.selectedSide.toUpperCase()} position.</p>`
          : ""
      }
      <div class="mt-3 flex items-center gap-2">
        <button data-action="toggle-orderbook" class="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-300">${state.showOrderbook ? "Hide depth" : "Show depth"}</button>
        <button data-action="toggle-fee-details" class="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-300">${state.showFeeDetails ? "Hide fee details" : "Fee details"}</button>
      </div>
      ${
        state.showOrderbook
          ? `<div class="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-xs">
        <p class="mb-2 font-semibold text-slate-200">${state.tradeIntent === "open" ? "Asks (buy depth)" : "Bids (sell depth)"} · ${state.selectedSide.toUpperCase()}</p>
        <div class="space-y-1">
          ${getOrderbookLevels(market, state.selectedSide, state.tradeIntent)
            .map(
              (
                level,
                idx,
              ) => `<div class="flex items-center justify-between rounded ${idx === 0 ? "bg-slate-900/70" : ""} px-2 py-1">
            <span>${level.priceSats} sats</span>
            <span>${level.contracts.toFixed(2)} contracts</span>
          </div>`,
            )
            .join("")}
        </div>
      </div>`
          : ""
      }
      ${
        state.showFeeDetails
          ? `<div class="mt-3 rounded border border-slate-800 bg-slate-900/40 p-2 text-xs text-slate-400">
        <p>Execution fee: 1% of matched notional.</p>
        <p>Winning PnL fee: 2% of positive payout minus entry cost (buy only).</p>
        <p>Final fee depends on actual matched fills.</p>
      </div>`
          : ""
      }
      <section class="mt-4 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
        <div class="flex items-center justify-between">
          <p class="text-xs text-slate-500">Advanced actions</p>
          <button data-action="toggle-advanced-actions" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-300">${state.showAdvancedActions ? "Hide" : "Show"}</button>
        </div>
      </section>
      ${
        state.showAdvancedActions
          ? `
      <div class="mt-3 grid grid-cols-3 gap-2">
        <button data-tab="issue" class="rounded border px-3 py-2 text-sm ${state.actionTab === "issue" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Issue</button>
        <button data-tab="redeem" class="rounded border px-3 py-2 text-sm ${state.actionTab === "redeem" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Redeem</button>
        <button data-tab="cancel" class="rounded border px-3 py-2 text-sm ${state.actionTab === "cancel" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-300"}">Cancel</button>
      </div>
      ${
        state.actionTab === "issue"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.initialIssue ? "0 \u2192 1 Initial Issuance" : "1 \u2192 1 Subsequent Issuance"}</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Pairs to mint</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Required collateral</span><span>${formatSats(issueCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT (${state.pairsInput} * 2 * ${market.cptSats})</div>
        </div>
        <button data-action="submit-issue" ${paths.issue || paths.initialIssue ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.issue || paths.initialIssue ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit Issuance Transaction</button>
      </div>
      `
          : ""
      }

      ${
        state.actionTab === "redeem"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.redeem ? "Post-resolution redemption" : paths.expiryRedeem ? "Expiry redemption" : "Unavailable"}</p>
        <label for="tokens-input" class="mb-1 block text-xs text-slate-400">Tokens to burn</label>
        <input id="tokens-input" type="number" min="1" step="1" value="${state.tokensInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral withdrawn</span><span>${formatSats(redeemCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: tokens * ${paths.redeem ? "2*CPT" : paths.expiryRedeem ? "CPT" : "N/A"}</div>
        </div>
        <button data-action="submit-redeem" ${paths.redeem || paths.expiryRedeem ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.redeem || paths.expiryRedeem ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit redemption tx</button>
      </div>
      `
          : ""
      }

      ${
        state.actionTab === "cancel"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: 1 \u2192 1 Cancellation</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Matched YES/NO pairs to burn</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral refund</span><span>${formatSats(cancelCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT</div>
        </div>
        <button data-action="submit-cancel" ${paths.cancel ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.cancel ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit cancellation tx</button>
      </div>
      `
          : ""
      }
      `
          : ""
      }
      <p class="mt-3 text-xs text-slate-500">${state.nostrPubkey ? `Nostr identity: ${state.nostrPubkey.slice(0, 12)}...` : "Nostr identity not initialized."}</p>
    </aside>
  `;
}

function renderDetail(): string {
  const market = getSelectedMarket();
  const noPrice = 1 - market.yesPrice;
  const paths = getPathAvailability(market);
  const expired = isExpired(market);
  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const collateralPoolSats = market.collateralUtxos.reduce(
    (sum, utxo) => sum + utxo.amountSats,
    0,
  );

  return `
    <div class="phi-container py-6 lg:py-8">
      ${
        expired && market.state === 1
          ? `<div class="mb-4 rounded-xl border border-slate-600 bg-slate-900/60 px-4 py-3 text-sm text-slate-300">Market expired unresolved at height ${market.expiryHeight}. Expiry redemption path is active. Issuance and oracle resolve are disabled.</div>`
          : ""
      }
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
            <button data-action="go-home" class="mb-4 rounded-lg border border-slate-700 px-3 py-1 text-sm text-slate-300">Back to markets</button>
            <p class="mb-1 text-sm text-slate-400">${market.category} · ${stateBadge(market.state)} ${market.creationTxid ? `<button data-action="refresh-market-state" class="text-slate-500 hover:text-slate-300 text-xs transition cursor-pointer">[refresh]</button>` : ""} · <button data-action="open-nostr-event" data-nevent="${market.nevent}" class="text-violet-400 hover:text-violet-300 transition cursor-pointer">View on Nostr</button>${market.creationTxid ? ` · <button data-action="open-explorer-tx" data-txid="${market.creationTxid}" class="text-violet-400 hover:text-violet-300 transition cursor-pointer">Creation Tx</button>` : ""}</p>
            <h1 class="phi-title mb-2 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${market.question}</h1>
            <p class="mb-3 text-base text-slate-400">${market.description}</p>

            <div class="mb-4 grid gap-3 sm:grid-cols-3">
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Yes price<br/><span class="text-lg font-medium text-emerald-400">${formatProbabilityWithPercent(market.yesPrice)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">No price<br/><span class="text-lg font-medium text-rose-400">${formatProbabilityWithPercent(noPrice)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Settlement deadline<br/><span class="text-slate-100">Est. by ${formatSettlementDateTime(estimatedSettlementDate)}</span></div>
            </div>

            ${(() => {
              const pos = getPositionContracts(market);
              if (pos.yes === 0 && pos.no === 0) return "";
              return `<div class="mb-4 flex items-center gap-3 rounded-xl border border-slate-700 bg-slate-900/40 px-4 py-3 text-sm">
                <span class="text-slate-400">Your position</span>
                ${pos.yes > 0 ? `<span class="rounded bg-emerald-500/20 px-2 py-0.5 font-medium text-emerald-300">YES ${pos.yes.toLocaleString()}</span>` : ""}
                ${pos.no > 0 ? `<span class="rounded bg-red-500/20 px-2 py-0.5 font-medium text-red-300">NO ${pos.no.toLocaleString()}</span>` : ""}
              </div>`;
            })()}

            ${chartSkeleton(market)}
          </div>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p class="panel-subtitle">Advanced</p>
                <h3 class="panel-title text-lg">Protocol Details</h3>
                <p class="text-sm text-slate-400">Oracle, covenant paths, and collateral mechanics. These do not change your basic yes/no order entry flow.</p>
              </div>
              <button data-action="toggle-advanced-details" class="rounded-lg border border-slate-700 px-3 py-2 text-sm text-slate-200">${state.showAdvancedDetails ? "Hide details" : "Show details"}</button>
            </div>
          </section>

          ${
            state.showAdvancedDetails
              ? `
          <div class="grid gap-3 lg:grid-cols-2">
            <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Oracle</p>
              <h3 class="panel-title mb-2 text-lg">Oracle Attestation</h3>
              <div class="space-y-1 text-xs text-slate-300">
                <div class="kv-row"><span>ORACLE_PUBLIC_KEY</span><span class="mono">${market.oraclePubkey}</span></div>
                <div class="kv-row"><span>MARKET_ID</span><span class="mono">${market.marketId}</span></div>
                <div class="kv-row"><span>Block target</span><span class="mono">${formatBlockHeight(market.expiryHeight)}</span></div>
                <div class="kv-row"><span>Current height</span><span class="mono">${formatBlockHeight(market.currentHeight)}</span></div>
                <div class="kv-row"><span>Message domain</span><span class="mono">SHA256(MARKET_ID || outcome_byte)</span></div>
                <div class="kv-row"><span>Outcome bytes</span><span class="mono">YES=0x01, NO=0x00</span></div>
                <div class="kv-row"><span>Resolve status</span><span class="${market.resolveTx?.sigVerified ? "text-emerald-300" : "text-slate-400"}">${market.resolveTx ? `Attested ${market.resolveTx.outcome.toUpperCase()} @ ${market.resolveTx.height}` : "Unresolved"}</span></div>
                ${market.resolveTx ? `<div class="kv-row"><span>Signature hash</span><span class="mono">${market.resolveTx.signatureHash}</span></div><div class="kv-row"><span>Resolve tx</span><span class="mono">${market.resolveTx.txid}</span></div>` : ""}
              </div>
              ${
                state.nostrPubkey &&
                state.nostrPubkey === market.oraclePubkey &&
                market.state === 1 &&
                !market.resolveTx
                  ? `
              <div class="mt-3 rounded-lg border border-amber-700/60 bg-amber-950/20 p-3">
                <p class="mb-2 text-sm font-semibold text-amber-200">You are the oracle for this market</p>
                <div class="flex items-center gap-2">
                  <button data-action="oracle-attest-yes" class="rounded-lg bg-emerald-300 px-4 py-2 text-sm font-semibold text-slate-950">Resolve YES</button>
                  <button data-action="oracle-attest-no" class="rounded-lg bg-rose-400 px-4 py-2 text-sm font-semibold text-slate-950">Resolve NO</button>
                </div>
              </div>`
                  : ""
              }
              ${
                state.lastAttestationSig &&
                state.lastAttestationMarketId === market.marketId &&
                market.state === 1
                  ? `
              <div class="mt-3 rounded-lg border border-emerald-700/60 bg-emerald-950/20 p-3">
                <p class="mb-2 text-sm font-semibold text-emerald-200">Attestation published — execute on-chain resolution</p>
                <p class="mb-2 text-xs text-slate-300">Outcome: ${state.lastAttestationOutcome ? "YES" : "NO"} | Sig: ${state.lastAttestationSig.slice(0, 24)}...</p>
                <button data-action="execute-resolution" ${state.resolutionExecuting ? "disabled" : ""} class="w-full rounded-lg ${state.resolutionExecuting ? "bg-slate-700 text-slate-400" : "bg-emerald-300 text-slate-950"} px-4 py-2 text-sm font-semibold">${state.resolutionExecuting ? "Executing..." : "Execute Resolution On-Chain"}</button>
              </div>`
                  : ""
              }
            </section>

            <section class="rounded-[21px] border ${market.collateralUtxos.length === 1 ? "border-emerald-800" : "border-rose-800"} bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Integrity</p>
              <h3 class="panel-title mb-2 text-lg">Single-UTXO Integrity</h3>
              <p class="text-sm ${market.collateralUtxos.length === 1 ? "text-emerald-300" : "text-rose-300"}">${market.collateralUtxos.length === 1 ? "OK: exactly one collateral UTXO" : "ALERT: fragmented collateral UTXO set"}</p>
              <div class="mt-2 space-y-2 text-xs text-slate-300">
                ${market.collateralUtxos
                  .map(
                    (utxo) =>
                      `<p class="mono">${utxo.txid}:${utxo.vout} · ${formatSats(utxo.amountSats)}</p>`,
                  )
                  .join("")}
              </div>
              <p class="mt-2 text-xs text-slate-500">Collateral pool: ${formatSats(collateralPoolSats)} · ${stateLabel(market.state)}</p>
            </section>
          </div>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <p class="panel-subtitle">Accounting</p>
            <h3 class="panel-title mb-2 text-lg">Collateral Mechanics</h3>
            <div class="grid gap-2 md:grid-cols-2 text-sm">
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Issuance: <span class="text-slate-100">pairs * 2 * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Post-resolution redeem: <span class="text-slate-100">tokens * 2 * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Expiry redeem: <span class="text-slate-100">tokens * CPT</span></div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/40 p-3">Cancellation: <span class="text-slate-100">pairs * 2 * CPT</span></div>
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <p class="panel-subtitle">State Machine</p>
            <h3 class="panel-title mb-2 text-lg">Covenant Paths</h3>
            <div class="grid gap-2 md:grid-cols-2">
              ${renderPathCard("0 \u2192 1 Initial issuance", paths.initialIssue, "pairs * 2 * CPT", "Outputs move to state-1 address")}
              ${renderPathCard("1 \u2192 1 Subsequent issuance", paths.issue, "pairs * 2 * CPT", "Collateral UTXO reconsolidated")}
              ${renderPathCard("1 \u2192 2/3 Oracle resolve", paths.resolve, "state commit via oracle signature", "All covenant outputs move atomically")}
              ${renderPathCard("2/3 Redemption", paths.redeem, "tokens * 2 * CPT", "Winning side burns tokens")}
              ${renderPathCard("1 Expiry redemption", paths.expiryRedeem, "tokens * CPT", "Unresolved + expiry only")}
              ${renderPathCard("1 \u2192 1 Cancellation", paths.cancel, "pairs * 2 * CPT", "Equal YES/NO burn")}
            </div>
          </section>
          `
              : ""
          }
        </section>

        ${renderActionTicket(market)}
      </div>
    </div>
  `;
}

function renderCreateMarket(): string {
  const yesSats = Math.max(
    1,
    Math.min(
      SATS_PER_FULL_CONTRACT - 1,
      Math.round(state.createStartingYesSats),
    ),
  );
  const noSats = SATS_PER_FULL_CONTRACT - yesSats;
  const settlementLabel = state.createSettlementInput
    ? new Date(state.createSettlementInput).toLocaleString("en-US", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
      })
    : "Not set";

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="mx-auto grid max-w-[1180px] gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
          <div class="mb-5 flex items-center justify-between gap-3">
            <div>
              <p class="panel-subtitle">Prediction Contract</p>
              <h1 class="phi-title text-xl font-medium text-slate-100 lg:text-2xl">Create New Market</h1>
            </div>
            <button data-action="cancel-create-market" class="rounded-lg border border-slate-700 px-3 py-2 text-sm text-slate-300">Back</button>
          </div>

          <div class="space-y-4">
            <div>
              <label for="create-question" class="mb-1 block text-xs text-slate-400">Question</label>
              <input id="create-question" value="${state.createQuestion}" maxlength="140" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Will X happen by Y?" />
            </div>

            <div>
              <label for="create-description" class="mb-1 block text-xs text-slate-400">Settlement rule</label>
              <textarea id="create-description" rows="3" maxlength="280" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Define exactly how YES/NO resolves.">${state.createDescription}</textarea>
            </div>

            <div class="grid gap-4 md:grid-cols-2">
              <div>
                <label for="create-category" class="mb-1 block text-xs text-slate-400">Category</label>
                <select id="create-category" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm">
                  ${categories
                    .filter((item) => item !== "Trending")
                    .map(
                      (item) =>
                        `<option value="${item}" ${state.createCategory === item ? "selected" : ""}>${item}</option>`,
                    )
                    .join("")}
                </select>
              </div>

              <div>
                <label for="create-settlement" class="mb-1 block text-xs text-slate-400">Settlement deadline</label>
                <input id="create-settlement" type="datetime-local" value="${state.createSettlementInput}" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
              </div>
            </div>

            <div>
              <label for="create-resolution-source" class="mb-1 block text-xs text-slate-400">Resolution source</label>
              <input id="create-resolution-source" value="${state.createResolutionSource}" maxlength="120" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Official source (e.g., NHC advisory, FEC filing, exchange index)" />
            </div>

            <div>
              <label for="create-yes-sats" class="mb-1 block text-xs text-slate-400">Starting Yes price (sats out of 100)</label>
              <input id="create-yes-sats" type="number" min="1" max="99" step="1" value="${yesSats}" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
            </div>
          </div>
        </section>

        <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
          <p class="panel-subtitle">Preview</p>
          <h3 class="panel-title mb-3 text-lg">New Contract Ticket</h3>
          <div class="space-y-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
            <p class="text-sm text-slate-200">${state.createQuestion.trim() || "Your market question will appear here."}</p>
            <p class="text-xs text-slate-400">${state.createDescription.trim() || "Settlement rule summary will appear here."}</p>
            <div class="grid grid-cols-2 gap-2">
              <div class="rounded-lg border border-slate-800 bg-slate-900/60 p-2 text-center text-emerald-400">Yes ${yesSats} sats</div>
              <div class="rounded-lg border border-slate-800 bg-slate-900/60 p-2 text-center text-rose-400">No ${noSats} sats</div>
            </div>
            <p class="text-xs text-slate-400">Category: <span class="text-slate-200">${state.createCategory}</span></p>
            <p class="text-xs text-slate-400">Settlement deadline: <span class="text-slate-200">${settlementLabel}</span></p>
            <p class="text-xs text-slate-400">Resolution source: <span class="text-slate-200">${state.createResolutionSource.trim() || "Not set"}</span></p>
            <p class="text-xs text-slate-500">Yes + No = ${SATS_PER_FULL_CONTRACT} sats</p>
          </div>
          <button data-action="submit-create-market" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950 disabled:opacity-50" ${state.marketCreating ? "disabled" : ""}>${state.marketCreating ? "Creating Market..." : "Create Market"}</button>
          <p class="mt-2 text-xs text-slate-500">${state.marketCreating ? "Building transaction, broadcasting, and announcing. This may take a moment." : "Creates the on-chain contract and announces the market. Your key is the oracle signing key."}</p>
        </aside>
      </div>
    </div>
  `;
}

function formatLbtc(sats: number): string {
  if (state.walletUnit === "sats") {
    return sats.toLocaleString() + " L-sats";
  }
  const btc = sats / 100_000_000;
  return btc.toFixed(8) + " L-BTC";
}

async function fetchWalletStatus(): Promise<void> {
  try {
    const appState = await invoke<{
      walletStatus: "not_created" | "locked" | "unlocked";
      networkStatus: { network: string; policyAssetId: string };
    }>("get_app_state");
    state.walletStatus = appState.walletStatus;
    state.walletNetwork = appState.networkStatus.network as
      | "mainnet"
      | "testnet"
      | "regtest";
    state.walletPolicyAssetId = appState.networkStatus.policyAssetId;
  } catch (e) {
    console.warn("Failed to fetch app state:", e);
  }
}

async function refreshWallet(): Promise<void> {
  state.walletLoading = true;
  state.walletError = "";
  showOverlayLoader("Syncing wallet...");
  render();
  try {
    await invoke("sync_wallet");
    const [balance, txs, swaps] = await Promise.all([
      invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
      invoke<
        {
          txid: string;
          balanceChange: number;
          fee: number;
          height: number | null;
          timestamp: number | null;
          txType: string;
        }[]
      >("get_wallet_transactions"),
      invoke<PaymentSwap[]>("list_payment_swaps"),
    ]);
    state.walletBalance = balance.assets;
    state.walletTransactions = txs;
    state.walletSwaps = swaps;
  } catch (e) {
    state.walletError = String(e);
  }
  state.walletLoading = false;
  hideOverlayLoader();
  render();
}

const QR_LOGO_SVG =
  "data:image/svg+xml;base64," +
  btoa(
    '<svg width="334" height="341" viewBox="0 0 334 341" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.19 11.59C0.19 1.58004 13.98 -4.04996 21.51 3.46004L110.88 91.89C128.28 87.15 147.01 84.56 166.53 84.56C186.05 84.56 204.79 87.14 222.19 91.89L311.54 3.47004C319.06 -4.02996 332.86 1.59004 332.86 11.6V206.56C332.98 208.58 333.05 210.61 333.05 212.65C333.05 283.39 258.5 340.74 166.53 340.74C74.56 340.74 0 283.4 0 212.65C0 210.61 0.06 208.58 0.19 206.56V11.59Z" fill="black"/><path d="M128.46 239.55L154.85 265.26V267.59C154.85 279.12 146.28 288.5 135.74 288.51H116.57C111.62 288.51 107.6 292.47 107.6 297.33C107.6 302.19 111.63 306.16 116.57 306.16H135.74C146.7 306.16 157 301.08 163.98 292.54C170.95 301.07 181.25 306.16 192.22 306.16C212.66 306.16 229.28 288.86 229.28 267.59C229.28 262.72 225.25 258.76 220.3 258.76C215.35 258.76 211.32 262.72 211.32 267.59C211.32 279.12 202.75 288.51 192.21 288.51C181.67 288.51 173.1 279.13 173.1 267.59V265.21L199.44 239.55H128.44H128.46ZM90.2699 179.49L67.1499 156.37L56.3599 167.16L79.4799 190.28L56.4399 213.32L67.2299 224.11L90.2699 201.07L113.39 224.19L124.18 213.4L101.06 190.28L124.26 167.09L113.47 156.3L90.2699 179.5V179.49ZM250.25 158.27C256.89 164.96 261.31 176.78 261.31 190.24C261.31 202.78 257.48 213.89 251.59 220.76C277 217.42 295.9 204.74 295.9 189.6C295.9 174.46 276.33 161.34 250.26 158.27H250.25ZM224.79 158.45C199.45 161.82 180.61 174.48 180.61 189.59C180.61 204.7 198.79 216.92 223.46 220.55C217.66 213.66 213.91 202.65 213.91 190.23C213.91 176.9 218.24 165.17 224.78 158.45H224.79Z" fill="#34D399"/></svg>',
  );

async function generateQr(value: string): Promise<void> {
  try {
    const canvas = document.createElement("canvas");
    await QRCode.toCanvas(canvas, value, {
      errorCorrectionLevel: "H",
      margin: 4,
      scale: 8,
      color: { dark: "#0f172a", light: "#ffffff" },
    });
    const ctx = canvas.getContext("2d")!;
    const logoImg = new Image();
    logoImg.src = QR_LOGO_SVG;
    await new Promise<void>((resolve, reject) => {
      logoImg.onload = () => resolve();
      logoImg.onerror = () => reject();
    });
    const logoSize = Math.floor(canvas.width * 0.22);
    const x = Math.floor((canvas.width - logoSize) / 2);
    const y = Math.floor((canvas.height - logoSize) / 2);
    const pad = 10;
    ctx.fillStyle = "#ffffff";
    ctx.beginPath();
    ctx.roundRect(x - pad, y - pad, logoSize + pad * 2, logoSize + pad * 2, 6);
    ctx.fill();
    ctx.drawImage(logoImg, x, y, logoSize, logoSize);
    state.modalQr = canvas.toDataURL("image/png");
  } catch {
    state.modalQr = "";
  }
}

function resetReceiveState(): void {
  state.receiveAmount = "";
  state.receiveCreating = false;
  state.receiveError = "";
  state.receiveLightningSwap = null;
  state.receiveLiquidAddress = "";
  state.receiveBitcoinSwap = null;
  state.modalQr = "";
}

function resetSendState(): void {
  state.sendInvoice = "";
  state.sendLiquidAddress = "";
  state.sendLiquidAmount = "";
  state.sendBtcAmount = "";
  state.sendCreating = false;
  state.sendError = "";
  state.sentLightningSwap = null;
  state.sentLiquidResult = null;
  state.sentBitcoinSwap = null;
  state.modalQr = "";
}

function flowLabel(flow: string): string {
  switch (flow) {
    case "liquid_to_lightning":
      return "Lightning Send";
    case "lightning_to_liquid":
      return "Lightning Receive";
    case "bitcoin_to_liquid":
      return "Bitcoin Receive";
    case "liquid_to_bitcoin":
      return "Bitcoin Send";
    default:
      return flow;
  }
}

function formatSwapStatus(status: string): string {
  return status.replace(/[._]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function renderMnemonicGrid(mnemonic: string): string {
  const words = mnemonic.split(" ");
  return (
    '<div class="grid grid-cols-3 gap-2">' +
    words
      .map(
        (w, i) =>
          '<div class="flex items-baseline gap-2 rounded bg-slate-800 px-3 py-2">' +
          '<span class="text-xs text-slate-500 w-5 text-right shrink-0">' +
          (i + 1) +
          ".</span>" +
          '<span class="mono text-sm text-slate-100 whitespace-nowrap">' +
          w +
          "</span>" +
          "</div>",
      )
      .join("") +
    "</div>"
  );
}

function renderBackupModal(loading: boolean): string {
  if (!state.walletShowBackup) return "";

  const closeBtn =
    '<button data-action="hide-backup" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">' +
    '<svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>' +
    "</button>";

  let body: string;
  if (state.walletBackupMnemonic) {
    const backupStatus = state.nostrBackupStatus;
    const securityInfoHtml =
      '<details class="group">' +
      '<summary class="cursor-pointer text-xs text-slate-500 hover:text-slate-400 transition select-none">' +
      "Why is this secure?" +
      "</summary>" +
      '<div class="mt-2 space-y-1.5 text-xs text-slate-500">' +
      '<p><strong class="text-slate-400">NIP-44 encryption</strong> &mdash; Your recovery phrase is encrypted using the Nostr NIP-44 protocol (XChaCha20 + secp256k1 ECDH). Only your private key (nsec) can decrypt it.</p>' +
      '<p><strong class="text-slate-400">Self-encrypted</strong> &mdash; The backup is encrypted to your own public key, so no one else can read it &mdash; not even the relay operators.</p>' +
      '<p><strong class="text-slate-400">Stored as NIP-78</strong> &mdash; The encrypted data is published as a kind 30078 addressable event (application-specific data). It can be retrieved from any relay that has it.</p>' +
      '<p><strong class="text-slate-400">Relay redundancy</strong> &mdash; The backup is sent to all your configured relays, so it survives even if some go offline.</p>' +
      "</div>" +
      "</details>";
    const nostrBackupHtml = state.nostrNpub
      ? '<div class="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">' +
        '<p class="text-[11px] font-medium uppercase tracking-wider text-slate-500">Nostr Relay Backup</p>' +
        (backupStatus?.has_backup
          ? '<p class="text-xs text-emerald-400">Encrypted backup stored on ' +
            backupStatus.relay_results.filter(
              (r: RelayBackupResult) => r.has_backup,
            ).length +
            " of " +
            backupStatus.relay_results.length +
            " relays</p>"
          : '<p class="text-xs text-slate-400">Encrypt and store your recovery phrase on Nostr relays using NIP-44.</p>' +
            '<button data-action="nostr-backup-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300 transition"' +
            (state.nostrBackupLoading ? " disabled" : "") +
            ">" +
            (state.nostrBackupLoading
              ? "Encrypting..."
              : "Encrypt & Upload to Relays") +
            "</button>") +
        securityInfoHtml +
        "</div>"
      : "";
    body =
      renderMnemonicGrid(state.walletBackupMnemonic) +
      '<div class="flex gap-3">' +
      '<button data-action="copy-backup-mnemonic" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Copy to clipboard</button>' +
      '<button data-action="hide-backup" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Done</button>' +
      "</div>" +
      nostrBackupHtml;
  } else {
    body =
      '<p class="text-sm text-slate-400">Enter your wallet password to reveal your recovery phrase.</p>' +
      '<input id="wallet-backup-password" type="password" maxlength="32" value="' +
      state.walletBackupPassword +
      '" placeholder="Wallet password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
      '<button data-action="export-backup" class="w-full rounded-xl bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' +
      (loading ? " disabled" : "") +
      ">Show Recovery Phrase</button>";
  }

  return (
    '<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">' +
    '<div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">' +
    '<div class="flex items-center justify-between">' +
    '<h2 class="text-lg font-medium text-slate-100">Backup Recovery Phrase</h2>' +
    closeBtn +
    "</div>" +
    '<div class="mt-5 space-y-4">' +
    body +
    '<p class="text-xs text-slate-500"><strong class="text-slate-300">Deadcat.live does not hold user funds.</strong> If you lose your recovery phrase and password, your funds cannot be recovered.</p>' +
    "</div>" +
    "</div></div>"
  );
}

function renderCopyable(
  value: string,
  label: string,
  copyAction: string,
): string {
  return (
    '<div class="flex items-center gap-2">' +
    '<div class="flex-1 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">' +
    '<div class="text-xs text-slate-500">' +
    label +
    "</div>" +
    '<div class="mono text-xs text-slate-300 truncate">' +
    value +
    "</div>" +
    "</div>" +
    '<button data-action="' +
    copyAction +
    '" data-copy-value="' +
    value +
    '" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800">Copy</button>' +
    "</div>"
  );
}

function renderModalTabs(): string {
  const tabs: Array<"lightning" | "liquid" | "bitcoin"> = [
    "lightning",
    "liquid",
    "bitcoin",
  ];
  return (
    '<div class="flex rounded-lg border border-slate-700 bg-slate-900/50 p-1 gap-1">' +
    tabs
      .map((t) => {
        const active = state.walletModalTab === t;
        const label =
          t === "lightning"
            ? "Lightning"
            : t === "liquid"
              ? "Liquid"
              : "Bitcoin";
        return (
          '<button data-action="modal-tab" data-tab-value="' +
          t +
          '" class="flex-1 rounded-md px-3 py-2 text-sm font-semibold transition ' +
          (active
            ? "bg-slate-700 text-slate-100"
            : "text-slate-400 hover:text-slate-200") +
          '">' +
          label +
          "</button>"
        );
      })
      .join("") +
    "</div>"
  );
}

function renderReceiveModal(): string {
  const err = state.receiveError;
  const creating = state.receiveCreating;

  let content = "";

  if (state.walletModalTab === "lightning") {
    if (state.receiveLightningSwap) {
      const s = state.receiveLightningSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Invoice Ready</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.expectedOnchainAmountSat.toLocaleString() +
        " sats expected on Liquid</p>" +
        '<p class="text-xs text-slate-500">Expires: ' +
        new Date(s.invoiceExpiresAt).toLocaleString() +
        "</p>" +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(s.invoice, "BOLT11 Invoice", "copy-modal-value") +
        "</div>";
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create a Lightning invoice via Boltz swap. Funds settle as L-BTC.</p>' +
        '<div class="flex gap-2">' +
        '<button data-action="receive-preset" data-preset="1000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">1k</button>' +
        '<button data-action="receive-preset" data-preset="10000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">10k</button>' +
        '<button data-action="receive-preset" data-preset="100000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">100k</button>' +
        "</div>" +
        '<div class="flex gap-2">' +
        '<input id="receive-amount" type="number" value="' +
        state.receiveAmount +
        '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-lightning-receive" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating..." : "Create Invoice") +
        "</button>" +
        "</div>" +
        "</div>";
    }
  } else if (state.walletModalTab === "liquid") {
    if (state.receiveLiquidAddress) {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Send L-BTC to this address to fund your wallet.</p>' +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(
          state.receiveLiquidAddress,
          "Liquid Address",
          "copy-modal-value",
        ) +
        '<button data-action="generate-liquid-address" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">New Address</button>' +
        "</div>";
    } else {
      content =
        '<div class="flex flex-col items-center gap-4 py-4">' +
        '<p class="text-sm text-slate-400">Generate a Liquid address to receive L-BTC.</p>' +
        '<button data-action="generate-liquid-address" class="rounded-lg bg-emerald-400 px-6 py-3 font-medium text-slate-950 hover:bg-emerald-300">' +
        (creating ? "Generating..." : "Generate Address") +
        "</button>" +
        "</div>";
    }
  } else {
    // Bitcoin tab
    if (state.receiveBitcoinSwap) {
      const s = state.receiveBitcoinSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Bitcoin Deposit Address Ready</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.expectedAmountSat.toLocaleString() +
        " sats expected on Liquid</p>" +
        '<p class="text-xs text-slate-500">Timeout block: ' +
        s.timeoutBlockHeight +
        "</p>" +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(
          s.lockupAddress,
          "Bitcoin Lockup Address",
          "copy-modal-value",
        ) +
        (s.bip21
          ? renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value")
          : "") +
        "</div>";
    } else {
      const pair = state.receiveBtcPairInfo;
      const pairInfo = pair
        ? '<div class="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">' +
          "<div>Min: " +
          pair.minAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Max: " +
          pair.maxAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Fee: " +
          pair.feePercentage +
          "% + " +
          pair.fixedMinerFeeTotalSat.toLocaleString() +
          " sats fixed</div>" +
          "</div>"
        : "";
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create a Boltz chain swap. Send BTC on-chain to receive L-BTC.</p>' +
        pairInfo +
        '<div class="flex gap-2">' +
        '<input id="receive-amount" type="number" value="' +
        state.receiveAmount +
        '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-bitcoin-receive" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating..." : "Create Address") +
        "</button>" +
        "</div>" +
        "</div>";
    }
  }

  if (err) {
    content +=
      '<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">' +
      err +
      "</div>";
  }

  return content;
}

function renderSendModal(): string {
  const err = state.sendError;
  const creating = state.sendCreating;

  let content = "";

  if (state.walletModalTab === "lightning") {
    if (state.sentLightningSwap) {
      const s = state.sentLightningSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Swap Created</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.invoiceAmountSat.toLocaleString() +
        " sats</p>" +
        '<p class="text-xs text-slate-500">Waiting for lockup confirmation. Expires: ' +
        new Date(s.invoiceExpiresAt).toLocaleString() +
        "</p>" +
        renderCopyable(s.lockupAddress, "Lockup Address", "copy-modal-value") +
        renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value") +
        "</div>";
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Paste a BOLT11 Lightning invoice to pay via Boltz submarine swap.</p>' +
        '<input id="send-invoice" value="' +
        state.sendInvoice +
        '" placeholder="BOLT11 invoice" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="pay-lightning-invoice" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating Swap..." : "Pay via Lightning") +
        "</button>" +
        "</div>";
    }
  } else if (state.walletModalTab === "liquid") {
    if (state.sentLiquidResult) {
      const r = state.sentLiquidResult;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-emerald-300">Transaction Sent</p>' +
        '<p class="text-xs text-slate-400">Fee: ' +
        r.feeSat.toLocaleString() +
        " sats</p>" +
        renderCopyable(r.txid, "Transaction ID", "copy-modal-value") +
        "</div>";
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Send L-BTC directly to a Liquid address.</p>' +
        '<input id="send-liquid-address" value="' +
        state.sendLiquidAddress +
        '" placeholder="Liquid address" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<input id="send-liquid-amount" type="number" value="' +
        state.sendLiquidAmount +
        '" placeholder="Amount (sats)" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="send-liquid" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Sending..." : "Send L-BTC") +
        "</button>" +
        "</div>";
    }
  } else {
    // Bitcoin tab
    if (state.sentBitcoinSwap) {
      const s = state.sentBitcoinSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Chain Swap Created</p>' +
        '<p class="text-xs text-slate-400">Swap ' +
        s.id.slice(0, 8) +
        "... | " +
        s.expectedAmountSat.toLocaleString() +
        " sats expected on Bitcoin</p>" +
        '<p class="text-xs text-slate-500">Timeout block: ' +
        s.timeoutBlockHeight +
        "</p>" +
        (state.modalQr
          ? '<div class="flex justify-center"><img src="' +
            state.modalQr +
            '" alt="QR" class="w-56 h-56 rounded-lg" /></div>'
          : "") +
        renderCopyable(
          s.lockupAddress,
          "Liquid Lockup Address",
          "copy-modal-value",
        ) +
        (s.bip21
          ? renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value")
          : "") +
        "</div>";
    } else {
      const pair = state.sendBtcPairInfo;
      const pairInfo = pair
        ? '<div class="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">' +
          "<div>Min: " +
          pair.minAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Max: " +
          pair.maxAmountSat.toLocaleString() +
          " sats</div>" +
          "<div>Fee: " +
          pair.feePercentage +
          "% + " +
          pair.fixedMinerFeeTotalSat.toLocaleString() +
          " sats fixed</div>" +
          "</div>"
        : "";
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create an L-BTC to BTC chain swap via Boltz.</p>' +
        pairInfo +
        '<div class="flex gap-2">' +
        '<input id="send-btc-amount" type="number" value="' +
        state.sendBtcAmount +
        '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-bitcoin-send" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' +
        (creating ? " disabled" : "") +
        ">" +
        (creating ? "Creating..." : "Create Swap") +
        "</button>" +
        "</div>" +
        "</div>";
    }
  }

  if (err) {
    content +=
      '<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">' +
      err +
      "</div>";
  }

  return content;
}

function renderWalletModal(): string {
  if (state.walletModal === "none") return "";

  const title =
    state.walletModal === "receive" ? "Receive Funds" : "Send Funds";
  const subtitle =
    state.walletModal === "receive"
      ? "Choose a method to receive funds into your Liquid wallet."
      : "Send funds from your wallet via Lightning, Liquid, or Bitcoin.";
  const body =
    state.walletModal === "receive" ? renderReceiveModal() : renderSendModal();

  return `
    <div data-action="modal-backdrop" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div class="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div>
            <h3 class="text-lg font-medium text-slate-100">${title}</h3>
            <p class="text-xs text-slate-400">${subtitle}</p>
          </div>
          <button data-action="close-modal" class="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </div>
        <div class="space-y-4 p-6">
          ${renderModalTabs()}
          ${body}
        </div>
      </div>
    </div>
  `;
}

function renderWallet(): string {
  const loading = state.walletLoading;
  const error = state.walletError;

  const networkBadge =
    state.walletNetwork !== "mainnet"
      ? `<span class="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">${state.walletNetwork}</span>`
      : "";

  const errorHtml = error
    ? `<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">${error}</div>`
    : "";

  const loadingHtml = "";

  if (state.walletStatus === "not_created") {
    if (state.walletMnemonic) {
      return `
        <div class="phi-container py-8">
          <div class="mx-auto max-w-lg space-y-6">
            <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet Created ${networkBadge}</h2>
            <div class="rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
              <p class="text-sm font-medium text-slate-200">Back up your recovery phrase in a safe place.</p>
              ${renderMnemonicGrid(state.walletMnemonic)}
              <button data-action="copy-mnemonic" class="mt-2 w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Copy to clipboard</button>
            </div>
            ${errorHtml}
            <button data-action="dismiss-mnemonic" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">I've saved my recovery phrase</button>
          </div>
        </div>
      `;
    }

    const isCreate = !state.walletShowRestore;
    const isRestore = state.walletShowRestore;

    return `
      <div class="phi-container py-8">
        <div class="mx-auto max-w-lg space-y-6">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
          <p class="text-sm text-slate-400">Set up a Liquid (L-BTC) wallet to participate in markets.</p>
          ${errorHtml}

          ${
            state.nostrNpub && !loading
              ? `<button data-action="nostr-restore-wallet" class="w-full rounded-xl border border-slate-700 bg-slate-900/50 p-4 text-left transition hover:border-slate-600">
            <div class="flex items-center gap-3">
              <svg class="h-6 w-6 text-slate-500 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"/></svg>
              <div>
                <p class="text-sm font-medium text-slate-300">Restore from Nostr Backup</p>
                <p class="mt-0.5 text-xs text-slate-500">Fetch encrypted backup from your relays</p>
              </div>
            </div>
          </button>`
              : ""
          }

          <div class="grid grid-cols-2 gap-3">
            <button data-action="${isCreate || loading ? "" : "toggle-restore"}" class="rounded-xl border ${isCreate ? "border-emerald-500/50 bg-emerald-500/10" : "border-slate-700 bg-slate-900/50 hover:border-slate-600"} p-4 text-left transition ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>
              <svg class="h-6 w-6 ${isCreate ? "text-emerald-400" : "text-slate-500"}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/></svg>
              <p class="mt-2 text-sm font-medium ${isCreate ? "text-emerald-300" : "text-slate-300"}">Create New</p>
              <p class="mt-0.5 text-xs ${isCreate ? "text-emerald-400/60" : "text-slate-500"}">Generate a fresh wallet</p>
            </button>
            <button data-action="${isRestore || loading ? "" : "toggle-restore"}" class="rounded-xl border ${isRestore ? "border-emerald-500/50 bg-emerald-500/10" : "border-slate-700 bg-slate-900/50 hover:border-slate-600"} p-4 text-left transition ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>
              <svg class="h-6 w-6 ${isRestore ? "text-emerald-400" : "text-slate-500"}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"/></svg>
              <p class="mt-2 text-sm font-medium ${isRestore ? "text-emerald-300" : "text-slate-300"}">Restore</p>
              <p class="mt-0.5 text-xs ${isRestore ? "text-emerald-400/60" : "text-slate-500"}">From recovery phrase</p>
            </button>
          </div>

          ${
            isCreate
              ? `
            <div class="space-y-4 rounded-xl border border-slate-700 bg-slate-900/50 p-6">
              <div>
                <label for="wallet-password" class="text-xs font-medium text-slate-400">Encryption Password</label>
                <p class="mt-0.5 text-[11px] text-slate-500">Used to encrypt your wallet on this device.</p>
              </div>
              <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Enter a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
              <button data-action="create-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Creating..." : "Create Wallet"}</button>
            </div>
          `
              : `
            <div class="space-y-4 rounded-xl border border-slate-700 bg-slate-900/50 p-6">
              <div>
                <label for="wallet-restore-mnemonic" class="text-xs font-medium text-slate-400">Recovery Phrase</label>
                <p class="mt-0.5 text-[11px] text-slate-500">Enter your 12-word recovery phrase to restore your wallet.</p>
              </div>
              <textarea id="wallet-restore-mnemonic" placeholder="word1 word2 word3 ..." rows="3" class="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 focus:ring-2 mono disabled:opacity-50" ${loading ? "disabled" : ""}>${state.walletRestoreMnemonic}</textarea>
              <div>
                <label for="wallet-password" class="text-xs font-medium text-slate-400">Encryption Password</label>
                <p class="mt-0.5 text-[11px] text-slate-500">Set a password to encrypt the restored wallet.</p>
              </div>
              <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Enter a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
              <button data-action="restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore Wallet"}</button>
            </div>
          `
          }
        </div>
      </div>
    `;
  }

  if (state.walletStatus === "locked") {
    return `
      <div class="phi-container py-8">
        <div class="mx-auto max-w-lg space-y-6">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
          <p class="text-sm text-slate-400">Wallet locked. Enter your password to unlock.</p>
          ${errorHtml}
          ${loadingHtml}
          <div class="space-y-4 rounded-lg border border-slate-700 bg-slate-900/50 p-6">
            <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
            <button data-action="unlock-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Unlocking..." : "Unlock"}</button>
          </div>
          <details class="group">
            <summary class="cursor-pointer text-xs text-slate-500 hover:text-slate-400 transition select-none">Forgot your password?</summary>
            <div class="mt-3 rounded-lg border border-slate-800 bg-slate-900/50 p-4 space-y-3">
              <p class="text-xs text-slate-400">The password protects your wallet on this device only. If you've forgotten it, you can delete the wallet and restore it using either method below. <strong class="text-slate-300">Your funds are safe</strong> as long as you have your recovery phrase or nsec.</p>
              <div class="space-y-1.5">
                ${
                  state.nostrNpub
                    ? `<div class="flex items-start gap-2">
                  <svg class="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
                  <p class="text-xs text-slate-400"><strong class="text-slate-300">Restore from Nostr backup</strong> — If you backed up to relays, your nsec is all you need. No password required.</p>
                </div>`
                    : ""
                }
                <div class="flex items-start gap-2">
                  <svg class="mt-0.5 h-3.5 w-3.5 shrink-0 text-slate-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"/></svg>
                  <p class="text-xs text-slate-400"><strong class="text-slate-300">Restore from recovery phrase</strong> — Enter your 12-word seed phrase and set a new password.</p>
                </div>
              </div>
              <button data-action="forgot-password-delete" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Delete Wallet & Restore</button>
            </div>
          </details>
        </div>
      </div>
    `;
  }

  // Unlocked — clean dashboard
  const policyBalance =
    state.walletBalance && state.walletPolicyAssetId
      ? (state.walletBalance[state.walletPolicyAssetId] ?? 0)
      : 0;

  const creationTxToMarket = new Map(
    markets.filter((m) => m.creationTxid).map((m) => [m.creationTxid!, m.id]),
  );

  // Map token asset IDs to labels for display.
  // Market asset IDs are internal byte order; wallet balance keys are display order (reversed).
  const assetLabel = new Map<
    string,
    { side: string; question: string; marketId: string }
  >();
  for (const m of markets) {
    if (m.yesAssetId)
      assetLabel.set(reverseHex(m.yesAssetId), {
        side: "YES",
        question: m.question,
        marketId: m.id,
      });
    if (m.noAssetId)
      assetLabel.set(reverseHex(m.noAssetId), {
        side: "NO",
        question: m.question,
        marketId: m.id,
      });
  }

  // Token positions: non-policy assets with positive balance
  const tokenPositions = state.walletBalance
    ? Object.entries(state.walletBalance)
        .filter(([id, amt]) => id !== state.walletPolicyAssetId && amt > 0)
        .map(([id, amt]) => {
          const info = assetLabel.get(id);
          return { assetId: id, amount: amt, info };
        })
    : [];

  const txRows = state.walletTransactions
    .map((tx) => {
      const marketId = creationTxToMarket.get(tx.txid);
      const isCreation = !!marketId;
      const isIssuance = tx.txType === "issuance" || tx.txType === "reissuance";
      const sign = tx.balanceChange >= 0 ? "+" : "";
      const color =
        isCreation || isIssuance
          ? "text-violet-300"
          : tx.balanceChange >= 0
            ? "text-emerald-300"
            : "text-red-300";
      const icon =
        isCreation || isIssuance
          ? "&#9670;"
          : tx.balanceChange >= 0
            ? "&#8595;"
            : "&#8593;";
      let label = "";
      if (isCreation) {
        label =
          '<button data-open-market="' +
          marketId +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">Market Creation</button>';
      } else if (isIssuance) {
        label =
          '<span class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300">Issuance</span>';
      }
      const date = tx.timestamp
        ? new Date(tx.timestamp * 1000).toLocaleString()
        : "unconfirmed";
      const shortTxid = tx.txid.slice(0, 10) + "..." + tx.txid.slice(-6);
      return (
        '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm select-none">' +
        '<div class="flex items-center gap-2">' +
        '<span class="' +
        color +
        '">' +
        icon +
        "</span>" +
        '<button data-action="open-explorer-tx" data-txid="' +
        tx.txid +
        '" class="mono text-slate-400 hover:text-slate-200 transition cursor-pointer">' +
        shortTxid +
        "</button>" +
        label +
        '<span class="text-slate-500">' +
        date +
        "</span>" +
        "</div>" +
        '<div class="text-right">' +
        '<span class="' +
        color +
        '">' +
        sign +
        formatLbtc(tx.balanceChange) +
        "</span>" +
        (state.baseCurrency !== "BTC"
          ? '<div class="text-xs text-slate-500">' +
            satsToFiatStr(Math.abs(tx.balanceChange)) +
            "</div>"
          : "") +
        "</div>" +
        "</div>"
      );
    })
    .join("");

  const swapRows = state.walletSwaps
    .map((sw) => {
      return (
        '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
        "<div>" +
        '<span class="text-slate-300">' +
        flowLabel(sw.flow) +
        "</span>" +
        '<span class="ml-2 text-slate-500">' +
        sw.invoiceAmountSat.toLocaleString() +
        " sats</span>" +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<span class="text-xs text-slate-500">' +
        formatSwapStatus(sw.status) +
        "</span>" +
        '<button data-action="refresh-swap" data-swap-id="' +
        sw.id +
        '" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-400 hover:bg-slate-800">Refresh</button>' +
        "</div>" +
        "</div>"
      );
    })
    .join("");

  return `
    <div class="phi-container py-8">
      <div class="mx-auto max-w-2xl space-y-6">
        <div class="flex items-center justify-between">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
          <div class="flex gap-2">
            <button data-action="sync-wallet" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800" ${loading ? "disabled" : ""}>Sync</button>
            <button data-action="show-backup" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Backup</button>
            <button data-action="lock-wallet" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Lock</button>
          </div>
        </div>

        ${errorHtml}
        ${loadingHtml}

        <!-- Balance -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center">
          <div class="flex items-center justify-center gap-2 text-sm text-slate-400">
            <span>Balance</span>
            <button data-action="toggle-balance-hidden" class="text-slate-500 hover:text-slate-300" title="${state.walletBalanceHidden ? "Show balance" : "Hide balance"}">
              ${
                state.walletBalanceHidden
                  ? `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg>`
                  : `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>`
              }
            </button>
          </div>
          <div class="mt-1 text-3xl font-medium tracking-tight text-slate-100">${state.walletBalanceHidden ? "********" : formatLbtc(policyBalance)}</div>
          ${!state.walletBalanceHidden && state.baseCurrency !== "BTC" ? `<div class="mt-1 text-sm text-slate-400">${satsToFiatStr(policyBalance)}</div>` : ""}
          <div class="mt-3 flex items-center justify-center gap-1 rounded-full border border-slate-700 mx-auto w-fit text-xs">
            <button data-action="set-wallet-unit" data-unit="sats" class="rounded-full px-3 py-1 transition ${state.walletUnit === "sats" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-sats</button>
            <button data-action="set-wallet-unit" data-unit="btc" class="rounded-full px-3 py-1 transition ${state.walletUnit === "btc" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-BTC</button>
          </div>
        </div>

        ${
          tokenPositions.length === 0 && !state.walletBalanceHidden
            ? `
        <!-- No Positions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center">
          <p class="text-sm text-slate-400">No token positions yet</p>
          <p class="mt-1 text-xs text-slate-500">Issue tokens on a market to start trading</p>
        </div>
        `
            : ""
        }

        ${
          tokenPositions.length > 0 && !state.walletBalanceHidden
            ? `
        <!-- Token Positions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Token Positions</h3>
          ${tokenPositions
            .map((tp) => {
              const shortAsset =
                tp.assetId.slice(0, 8) + "..." + tp.assetId.slice(-4);
              if (tp.info) {
                const sideColor =
                  tp.info.side === "YES" ? "text-emerald-300" : "text-red-300";
                const sideBg =
                  tp.info.side === "YES"
                    ? "bg-emerald-500/20"
                    : "bg-red-500/20";
                const truncQ =
                  tp.info.question.length > 50
                    ? tp.info.question.slice(0, 50) + "..."
                    : tp.info.question;
                return (
                  '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                  '<div class="flex items-center gap-2">' +
                  '<span class="rounded ' +
                  sideBg +
                  " px-1.5 py-0.5 text-[10px] font-medium " +
                  sideColor +
                  '">' +
                  tp.info.side +
                  "</span>" +
                  '<button data-open-market="' +
                  tp.info.marketId +
                  '" class="text-slate-300 hover:text-slate-100 transition cursor-pointer text-left">' +
                  truncQ +
                  "</button>" +
                  "</div>" +
                  '<span class="mono text-slate-100">' +
                  tp.amount.toLocaleString() +
                  "</span>" +
                  "</div>"
                );
              }
              return (
                '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                '<span class="mono text-slate-400">' +
                shortAsset +
                "</span>" +
                '<span class="mono text-slate-100">' +
                tp.amount.toLocaleString() +
                "</span>" +
                "</div>"
              );
            })
            .join("")}
        </div>
        `
            : ""
        }

        <!-- Action Buttons -->
        <div class="grid grid-cols-2 gap-4">
          <button data-action="open-receive" class="flex items-center justify-center gap-3 rounded-xl border border-emerald-400/30 bg-emerald-900/20 px-6 py-4 font-semibold text-emerald-300 transition hover:bg-emerald-900/40">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/></svg>
            Receive
          </button>
          <button data-action="open-send" class="flex items-center justify-center gap-3 rounded-xl border border-slate-600 bg-slate-800/60 px-6 py-4 font-medium text-slate-200 transition hover:bg-slate-800">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/></svg>
            Send
          </button>
        </div>

        <!-- Transactions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Transactions</h3>
          ${
            state.walletTransactions.length === 0
              ? `<p class="text-sm text-slate-500">No transactions yet.</p>`
              : txRows
          }
        </div>

        <!-- Swaps -->
        ${
          state.walletSwaps.length > 0
            ? `
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Swaps</h3>
          ${swapRows}
        </div>
        `
            : ""
        }

        <!-- Backup modal rendered in renderTopShell -->
      </div>
    </div>
    ${renderWalletModal()}
  `;
}

function renderOnboarding(): string {
  const step = state.onboardingStep!;
  const loading = state.onboardingLoading;
  const errorHtml = state.onboardingError
    ? `<p class="text-sm text-red-400">${state.onboardingError}</p>`
    : "";

  const stepIndicator = `
    <div class="flex items-center gap-3 mb-6">
      <div class="flex items-center gap-2">
        <div class="h-8 w-8 rounded-full ${step === "nostr" || state.onboardingNostrDone ? "bg-emerald-400 text-slate-950" : "border border-slate-700 text-slate-500"} flex items-center justify-center text-sm font-medium">${state.onboardingNostrDone && step !== "nostr" ? '<svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>' : "1"}</div>
        <span class="text-sm ${step === "nostr" ? "text-slate-100" : "text-slate-500"}">Nostr Identity</span>
      </div>
      <div class="h-px flex-1 ${step === "wallet" ? "bg-emerald-400" : "bg-slate-700"}"></div>
      <div class="flex items-center gap-2">
        <div class="h-8 w-8 rounded-full ${step === "wallet" ? "bg-emerald-400 text-slate-950" : "border border-slate-700 text-slate-500"} flex items-center justify-center text-sm font-medium">2</div>
        <span class="text-sm ${step === "wallet" ? "text-slate-100" : "text-slate-500"}">Liquid Wallet</span>
      </div>
    </div>
  `;

  if (step === "nostr") {
    // After generation — show keys for backup
    if (state.onboardingNostrDone) {
      const nsecHtml = state.onboardingNostrGeneratedNsec
        ? `<div class="flex items-center gap-2">
            <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
              <div class="text-[10px] text-slate-500">nsec (secret)</div>
              ${
                state.onboardingNsecRevealed
                  ? `<div class="mono truncate text-xs text-rose-300">${state.onboardingNostrGeneratedNsec}</div>`
                  : `<div class="text-xs text-slate-500">Hidden</div>`
              }
            </div>
            ${
              state.onboardingNsecRevealed
                ? `<button data-action="onboarding-copy-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>`
                : `<button data-action="onboarding-reveal-nsec" class="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition">Reveal</button>`
            }
          </div>`
        : `<div class="flex items-center gap-2">
            <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
              <div class="text-[10px] text-slate-500">nsec (secret)</div>
              <div class="text-xs text-slate-500">Copied</div>
            </div>
          </div>`;

      return `
        <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
          ${stepIndicator}
          <h2 class="text-xl font-medium text-slate-100">Nostr Identity Created</h2>
          <p class="mt-2 text-sm text-slate-400">Back up your secret key (nsec) now. You will need it to resolve markets you create.</p>
          <div class="mt-4 rounded-lg border border-amber-700/40 bg-amber-950/20 px-3 py-2">
            <p class="text-[11px] text-amber-300/90">Save your nsec in a secure location. If you lose it, you cannot resolve markets created with this identity.</p>
          </div>
          <div class="mt-4 space-y-2">
            <div class="flex items-center gap-2">
              <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                <div class="text-[10px] text-slate-500">npub (public)</div>
                <div class="mono truncate text-xs text-slate-300">${state.nostrNpub}</div>
              </div>
              <button data-action="onboarding-copy-npub" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>
            </div>
            ${nsecHtml}
          </div>
          <button data-action="onboarding-nostr-continue" class="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">Continue to Wallet Setup</button>
        </div>
      `;
    }

    // Initial nostr step — choose generate or import
    const modeGenerate = state.onboardingNostrMode === "generate";
    return `
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        ${stepIndicator}
        <h2 class="text-xl font-medium text-slate-100">Welcome to Deadcat Live!</h2>
        <p class="mt-2 text-sm text-slate-400">Set up your Nostr identity. This keypair is used to publish and resolve prediction markets.</p>
        ${errorHtml}
        <div class="mt-5 flex gap-2">
          <button data-action="onboarding-set-nostr-mode" data-mode="generate" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${modeGenerate ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"}">Generate new</button>
          <button data-action="onboarding-set-nostr-mode" data-mode="import" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${!modeGenerate ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"}">Import existing</button>
        </div>
        ${
          modeGenerate
            ? `
          <button data-action="onboarding-generate-nostr" class="mt-5 w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>${loading ? "Generating..." : "Generate Keypair"}</button>
        `
            : `
          <div class="mt-5 space-y-3">
            <input id="onboarding-nostr-nsec" type="password" value="${state.onboardingNostrNsec}" placeholder="nsec1..." class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono" />
            <button data-action="onboarding-import-nostr" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>${loading ? "Importing..." : "Import & Continue"}</button>
          </div>
        `
        }
      </div>
    `;
  }

  // Step 2: Wallet
  if (
    state.onboardingWalletMnemonic &&
    state.onboardingWalletMode === "create"
  ) {
    // Show mnemonic backup after creation
    return `
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        ${stepIndicator}
        <h2 class="text-xl font-medium text-slate-100">Wallet Created</h2>
        <p class="mt-2 text-sm text-slate-400">Back up your recovery phrase in a safe place.</p>
        <div class="mt-4 rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
          ${renderMnemonicGrid(state.onboardingWalletMnemonic)}
          <button data-action="onboarding-copy-mnemonic" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Copy to clipboard</button>
        </div>
        <button data-action="onboarding-wallet-done" class="mt-5 w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">I've saved my recovery phrase</button>
      </div>
    `;
  }

  const wMode = state.onboardingWalletMode;
  const modeCreate = wMode === "create";
  const modeRestore = wMode === "restore";
  const modeNostrRestore = wMode === "nostr-restore";

  // Scanning indicator
  const scanningHtml = state.onboardingBackupScanning
    ? '<div class="mt-4 flex items-center gap-2 rounded-lg border border-slate-700 bg-slate-900/50 px-4 py-3"><div class="h-4 w-4 animate-spin rounded-full border-2 border-slate-600 border-t-emerald-400"></div><p class="text-sm text-slate-400">Scanning relays for existing wallet backup...</p></div>'
    : "";

  // Nostr backup found banner
  const backupFoundHtml =
    state.onboardingBackupFound && !state.onboardingBackupScanning
      ? '<button data-action="onboarding-set-wallet-mode" data-mode="nostr-restore" class="mt-4 w-full rounded-xl border ' +
        (modeNostrRestore
          ? "border-emerald-500/50 bg-emerald-500/10"
          : "border-emerald-700/40 bg-emerald-950/20 hover:border-emerald-600/50") +
        ' p-4 text-left transition">' +
        '<div class="flex items-center gap-3">' +
        '<svg class="h-6 w-6 text-emerald-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"/></svg>' +
        "<div>" +
        '<p class="text-sm font-medium text-emerald-300">Wallet backup found on your relays</p>' +
        '<p class="mt-0.5 text-xs text-emerald-400/60">Restore your existing Liquid wallet from your encrypted Nostr backup</p>' +
        "</div>" +
        "</div></button>"
      : "";

  // Mode-specific form content
  let formHtml = "";
  if (modeNostrRestore) {
    formHtml = `
      <div class="mt-5 space-y-3">
        <p class="text-sm text-slate-400">Your encrypted wallet backup will be fetched from your Nostr relays and decrypted locally. Set a password to protect the wallet on this device.</p>
        <input id="onboarding-wallet-password" type="password" maxlength="32" value="${state.onboardingWalletPassword}" placeholder="Set a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <button data-action="onboarding-nostr-restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore from Nostr Backup"}</button>
      </div>`;
  } else if (modeCreate) {
    formHtml = `
      <div class="mt-5 space-y-3">
        <input id="onboarding-wallet-password" type="password" maxlength="32" value="${state.onboardingWalletPassword}" placeholder="Set a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <button data-action="onboarding-create-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Creating..." : "Create Wallet"}</button>
      </div>`;
  } else {
    formHtml = `
      <div class="mt-5 space-y-3">
        <textarea id="onboarding-wallet-mnemonic" placeholder="Enter your 12-word recovery phrase" rows="3" class="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""}>${state.onboardingWalletMnemonic}</textarea>
        <input id="onboarding-wallet-password" type="password" maxlength="32" value="${state.onboardingWalletPassword}" placeholder="Set a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <button data-action="onboarding-restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore & Finish"}</button>
      </div>`;
  }

  return `
    <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
      ${stepIndicator}
      <h2 class="text-xl font-medium text-slate-100">Set Up Your Wallet</h2>
      <p class="mt-2 text-sm text-slate-400">Create a new Liquid (L-BTC) wallet or restore from an existing recovery phrase.</p>
      ${errorHtml}
      ${scanningHtml}
      ${backupFoundHtml}
      <div class="mt-5 flex gap-2">
        <button data-action="${modeCreate || loading ? "" : "onboarding-set-wallet-mode"}" data-mode="create" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${modeCreate ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"} ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>Create new</button>
        <button data-action="${modeRestore || loading ? "" : "onboarding-set-wallet-mode"}" data-mode="restore" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${modeRestore ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"} ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>Restore existing</button>
      </div>
      ${formHtml}
    </div>
  `;
}

function render(): void {
  if (state.onboardingStep !== null) {
    app.innerHTML = `<div class="min-h-screen text-slate-100 flex items-center justify-center">${renderOnboarding()}</div>`;
    return;
  }
  const html = `
    <div class="min-h-screen text-slate-100">
      ${renderTopShell()}
      <main>${state.view === "wallet" ? renderWallet() : state.view === "home" ? renderHome() : state.view === "detail" ? renderDetail() : renderCreateMarket()}</main>
    </div>
  `;
  app.innerHTML = html;
}

async function syncCurrentHeightFromLwk(network: WalletNetwork): Promise<void> {
  try {
    const tip = await invoke<ChainTipResponse>("fetch_chain_tip", { network });
    if (!Number.isFinite(tip.height) || tip.height <= 0) return;

    let didChange = false;
    for (const market of markets) {
      if (market.currentHeight !== tip.height) {
        market.currentHeight = tip.height;
        didChange = true;
      }
    }

    if (didChange) {
      render();
      updateEstClockLabels();
    }
  } catch (error) {
    console.warn("Failed to sync chain tip from LWK:", error);
  }
}

function updateEstClockLabels(): void {
  const labels = document.querySelectorAll<HTMLElement>("[data-est-label]");
  if (!labels.length) return;
  labels.forEach((label) => {
    const offsetHours = Number(label.dataset.offsetHours ?? "0");
    const timestamp = Date.now() - offsetHours * 60 * 60 * 1000;
    label.textContent = formatEstTime(new Date(timestamp));
  });
}

function openMarket(
  marketId: string,
  options?: { side?: Side; intent?: TradeIntent },
): void {
  const market = getMarketById(marketId);
  const nextSide = options?.side ?? "yes";
  const nextIntent = options?.intent ?? "open";
  const positions = getPositionContracts(market);
  const selectedPosition = nextSide === "yes" ? positions.yes : positions.no;

  state.selectedMarketId = market.id;
  state.view = "detail";
  state.selectedSide = nextSide;
  state.orderType = "market";
  state.actionTab = "trade";
  state.tradeIntent = nextIntent;
  state.sizeMode = nextIntent === "close" ? "contracts" : "sats";
  state.showAdvancedDetails = false;
  state.showAdvancedActions = false;
  state.showOrderbook = false;
  state.showFeeDetails = false;
  state.tradeSizeSats = 10000;
  state.tradeSizeSatsDraft = formatSatsInput(10000);
  state.tradeContracts =
    nextIntent === "close"
      ? Math.max(0.01, Math.min(selectedPosition, selectedPosition / 2))
      : 10;
  state.tradeContractsDraft = state.tradeContracts.toFixed(2);
  setLimitPriceSats(getBasePriceSats(market, nextSide));
  render();
}

function ticketActionAllowed(market: Market, tab: ActionTab): boolean {
  const paths = getPathAvailability(market);
  if (tab === "trade") return true;
  if (tab === "issue") return paths.initialIssue || paths.issue;
  if (tab === "redeem") return paths.redeem || paths.expiryRedeem;
  return paths.cancel;
}

async function finishOnboarding(): Promise<void> {
  state.onboardingStep = null;
  state.onboardingWalletPassword = "";
  state.onboardingWalletMnemonic = "";
  state.onboardingNostrNsec = "";
  state.onboardingNostrGeneratedNsec = "";
  state.onboardingNsecRevealed = false;
  state.onboardingNostrDone = false;
  state.onboardingError = "";
  state.onboardingBackupFound = false;
  state.onboardingBackupScanning = false;

  await fetchWalletStatus();
  render();

  if (state.walletStatus === "unlocked") {
    void refreshWallet();
  }
  await loadMarkets();
  state.marketsLoading = false;
  render();
  void syncCurrentHeightFromLwk("liquid-testnet");

  // Fetch relay list + backup status in background (same as initApp boot path)
  if (state.nostrNpub) {
    invoke<string[]>("fetch_nip65_relay_list")
      .then((relays) => {
        state.relays = relays.map((u) => ({ url: u, has_backup: false }));
        invoke<NostrBackupStatus>("check_nostr_backup")
          .then((status) => {
            state.nostrBackupStatus = status;
            if (status.relay_results) {
              state.relays = state.relays.map((r) => ({
                ...r,
                has_backup:
                  status.relay_results.find(
                    (rr: RelayBackupResult) => rr.url === r.url,
                  )?.has_backup ?? false,
              }));
            }
            render();
          })
          .catch(() => {});
      })
      .catch(() => {});

    invoke<NostrProfile | null>("fetch_nostr_profile")
      .then((profile) => {
        if (profile) {
          state.nostrProfile = profile;
          render();
        }
      })
      .catch(() => {});
  }
}

function dismissSplash(): void {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("fade-out");
  splash.addEventListener("transitionend", () => splash.remove(), {
    once: true,
  });
}

async function initApp(): Promise<void> {
  render();
  updateEstClockLabels();

  // Track when the minimum loader animation time has elapsed (2 full cycles = 4.8s)
  const splashReady = new Promise<void>((r) => setTimeout(r, 4800));

  // 1. Try to load existing Nostr identity (no auto-generation)
  let hasNostrIdentity = false;
  try {
    const identity = await invoke<IdentityResponse | null>(
      "init_nostr_identity",
    );
    if (identity) {
      state.nostrPubkey = identity.pubkey_hex;
      state.nostrNpub = identity.npub;
      hasNostrIdentity = true;
    }
  } catch (error) {
    console.warn("Failed to load nostr identity:", error);
  }

  // 1b. If we have identity, fetch relay list and profile in background
  if (hasNostrIdentity) {
    // Fetch NIP-65 relay list (non-blocking)
    invoke<string[]>("fetch_nip65_relay_list")
      .then((relays) => {
        state.relays = relays.map((u) => ({ url: u, has_backup: false }));
        // After relays are loaded, check backup status
        invoke<NostrBackupStatus>("check_nostr_backup")
          .then((status) => {
            state.nostrBackupStatus = status;
            if (status.relay_results) {
              state.relays = state.relays.map((r) => ({
                ...r,
                has_backup:
                  status.relay_results.find(
                    (rr: RelayBackupResult) => rr.url === r.url,
                  )?.has_backup ?? false,
              }));
            }
            render();
          })
          .catch(() => {});
      })
      .catch(() => {
        // Fall back to defaults
        state.relays = [
          { url: "wss://relay.damus.io", has_backup: false },
          { url: "wss://relay.primal.net", has_backup: false },
        ];
      });

    // Fetch kind 0 profile (non-blocking)
    invoke<NostrProfile | null>("fetch_nostr_profile")
      .then((profile) => {
        if (profile) {
          state.nostrProfile = profile;
          render();
        }
      })
      .catch(() => {});
  }

  // 2. Fetch wallet status
  await fetchWalletStatus();

  // 3. Determine onboarding state
  const needsNostr = !hasNostrIdentity;
  const needsWallet = state.walletStatus === "not_created";

  if (needsNostr || needsWallet) {
    state.onboardingStep = needsNostr ? "nostr" : "wallet";
    if (!needsNostr) {
      state.onboardingNostrDone = true;
    }
    render();
    // Wait for loader animation to finish before revealing onboarding
    await splashReady;
    dismissSplash();
    // If going straight to wallet step with existing identity, auto-scan for backup
    if (!needsNostr && needsWallet) {
      state.onboardingBackupScanning = true;
      render();
      invoke<NostrBackupStatus>("check_nostr_backup")
        .then((status) => {
          if (status.has_backup) {
            state.onboardingBackupFound = true;
            state.onboardingWalletMode = "nostr-restore";
          }
        })
        .catch(() => {})
        .finally(() => {
          state.onboardingBackupScanning = false;
          render();
        });
    }
    return; // Don't load markets or start background tasks yet
  }

  // 4. Normal boot — both identity and wallet exist
  if (state.walletStatus === "unlocked") {
    void refreshWallet();
  }

  await Promise.all([loadMarkets(), splashReady]);
  state.marketsLoading = false;
  render();
  dismissSplash();

  void syncCurrentHeightFromLwk("liquid-testnet");
}

initApp();
setInterval(updateEstClockLabels, 1_000);
setInterval(() => {
  if (state.onboardingStep === null) {
    void syncCurrentHeightFromLwk("liquid-testnet");
  }
}, 60_000);

// Auto-refresh wallet balance every 60s when unlocked (cached only, no Electrum sync)
setInterval(() => {
  if (
    state.onboardingStep === null &&
    state.walletStatus === "unlocked" &&
    !state.walletLoading
  ) {
    (async () => {
      try {
        const balance = await invoke<{ assets: Record<string, number> }>(
          "get_wallet_balance",
        );
        state.walletBalance = balance.assets;
        if (state.view === "wallet") render();
      } catch (_) {
        // Silent — don't disrupt the user
      }
    })();
  }
}, 60_000);

app.addEventListener("click", (event) => {
  const target = event.target as HTMLElement;
  const categoryEl = target.closest("[data-category]") as HTMLElement | null;
  const openMarketEl = target.closest(
    "[data-open-market]",
  ) as HTMLElement | null;
  const actionEl = target.closest("[data-action]") as HTMLElement | null;
  const sideEl = target.closest("[data-side]") as HTMLElement | null;
  const tradeChoiceEl = target.closest(
    "[data-trade-choice]",
  ) as HTMLElement | null;
  const tradeIntentEl = target.closest(
    "[data-trade-intent]",
  ) as HTMLElement | null;
  const sizeModeEl = target.closest("[data-size-mode]") as HTMLElement | null;
  const tradeSizePresetEl = target.closest(
    "[data-trade-size-sats]",
  ) as HTMLElement | null;
  const tradeSizeDeltaEl = target.closest(
    "[data-trade-size-delta]",
  ) as HTMLElement | null;
  const orderTypeEl = target.closest("[data-order-type]") as HTMLElement | null;
  const tabEl = target.closest("[data-tab]") as HTMLElement | null;

  const category = categoryEl?.getAttribute(
    "data-category",
  ) as NavCategory | null;
  const openMarketId = openMarketEl?.getAttribute("data-open-market") ?? null;
  const openSide = openMarketEl?.getAttribute("data-open-side") as Side | null;
  const openIntentRaw = openMarketEl?.getAttribute("data-open-intent");
  const action = actionEl?.getAttribute("data-action") ?? null;
  const side = sideEl?.getAttribute("data-side") as Side | null;
  const tradeChoiceRaw =
    tradeChoiceEl?.getAttribute("data-trade-choice") ?? null;
  const tradeIntent = tradeIntentEl?.getAttribute(
    "data-trade-intent",
  ) as TradeIntent | null;
  const sizeMode = sizeModeEl?.getAttribute(
    "data-size-mode",
  ) as SizeMode | null;
  const tradeSizePreset = Number(
    tradeSizePresetEl?.getAttribute("data-trade-size-sats") ?? "",
  );
  const tradeSizeDelta = Number(
    tradeSizeDeltaEl?.getAttribute("data-trade-size-delta") ?? "",
  );
  const limitPriceDelta = Number(
    actionEl?.getAttribute("data-limit-price-delta") ?? "",
  );
  const orderType = orderTypeEl?.getAttribute(
    "data-order-type",
  ) as OrderType | null;
  const tab = tabEl?.getAttribute("data-tab") as ActionTab | null;

  // Close user menu on any click that isn't inside the menu
  if (
    state.userMenuOpen &&
    action !== "toggle-user-menu" &&
    action !== "user-settings" &&
    action !== "user-logout" &&
    action !== "set-currency" &&
    action !== "copy-nostr-npub"
  ) {
    // Check if click is inside the dropdown
    const inMenu = target
      .closest("[data-action='toggle-user-menu']")
      ?.parentElement?.contains(target);
    if (!inMenu) {
      state.userMenuOpen = false;
      render();
    }
  }

  if (category) {
    state.activeCategory = category;
    state.view = "home";
    render();
    return;
  }

  if (openMarketId) {
    const openIntent: TradeIntent | undefined =
      openIntentRaw === "sell"
        ? "close"
        : openIntentRaw === "buy"
          ? "open"
          : openIntentRaw === "open" || openIntentRaw === "close"
            ? openIntentRaw
            : undefined;
    openMarket(openMarketId, {
      side: openSide ?? undefined,
      intent: openIntent,
    });
    return;
  }

  // ── Onboarding actions ────────────────────────────────────────────

  if (action === "onboarding-set-nostr-mode") {
    state.onboardingNostrMode = (actionEl?.getAttribute("data-mode") ??
      "generate") as "generate" | "import";
    state.onboardingError = "";
    render();
    return;
  }

  if (action === "onboarding-generate-nostr") {
    state.onboardingLoading = true;
    state.onboardingError = "";
    render();
    (async () => {
      try {
        const identity = await invoke<IdentityResponse>(
          "generate_nostr_identity",
        );
        state.nostrPubkey = identity.pubkey_hex;
        state.nostrNpub = identity.npub;
        const nsec = await invoke<string>("export_nostr_nsec");
        state.onboardingNostrGeneratedNsec = nsec;
        state.onboardingNostrDone = true;
      } catch (e) {
        state.onboardingError = String(e);
      }
      state.onboardingLoading = false;
      render();
    })();
    return;
  }

  if (action === "onboarding-import-nostr") {
    const nsecInput = state.onboardingNostrNsec.trim();
    if (!nsecInput) {
      state.onboardingError = "Paste an nsec to import.";
      render();
      return;
    }
    state.onboardingLoading = true;
    state.onboardingError = "";
    render();
    (async () => {
      try {
        const identity = await invoke<IdentityResponse>("import_nostr_nsec", {
          nsec: nsecInput,
        });
        state.nostrPubkey = identity.pubkey_hex;
        state.nostrNpub = identity.npub;
        state.onboardingNostrDone = true;
        state.onboardingStep = "wallet";
        // Auto-scan relays for existing wallet backup
        state.onboardingBackupScanning = true;
        state.onboardingLoading = false;
        render();
        try {
          const status = await invoke<NostrBackupStatus>("check_nostr_backup");
          if (status.has_backup) {
            state.onboardingBackupFound = true;
            state.onboardingWalletMode = "nostr-restore";
          }
        } catch (_) {
          /* scan failed silently */
        }
        state.onboardingBackupScanning = false;
        render();
        return;
      } catch (e) {
        state.onboardingError = String(e);
      }
      state.onboardingLoading = false;
      render();
    })();
    return;
  }

  if (action === "onboarding-copy-npub") {
    if (state.nostrNpub) {
      void navigator.clipboard.writeText(state.nostrNpub);
      showToast("Copied npub to clipboard");
    }
    return;
  }

  if (action === "onboarding-reveal-nsec") {
    state.onboardingNsecRevealed = true;
    render();
    return;
  }

  if (action === "onboarding-copy-nsec") {
    if (state.onboardingNostrGeneratedNsec) {
      void navigator.clipboard.writeText(state.onboardingNostrGeneratedNsec);
      state.onboardingNsecRevealed = false;
      state.onboardingNostrGeneratedNsec = "";
      showToast("Copied nsec to clipboard");
      render();
    }
    return;
  }

  if (action === "onboarding-nostr-continue") {
    state.onboardingStep = "wallet";
    state.onboardingError = "";
    render();
    return;
  }

  if (action === "onboarding-set-wallet-mode") {
    state.onboardingWalletMode = (actionEl?.getAttribute("data-mode") ??
      "create") as "create" | "restore" | "nostr-restore";
    state.onboardingError = "";
    render();
    return;
  }

  if (action === "onboarding-create-wallet") {
    if (!state.onboardingWalletPassword) {
      state.onboardingError = "Password is required.";
      render();
      return;
    }
    state.onboardingLoading = true;
    state.onboardingError = "";
    showOverlayLoader("Creating wallet...");
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("create_wallet", {
          password: state.onboardingWalletPassword,
        });
        state.onboardingWalletMnemonic = mnemonic;
      } catch (e) {
        state.onboardingError = String(e);
      }
      state.onboardingLoading = false;
      hideOverlayLoader();
      render();
    })();
    return;
  }

  if (action === "onboarding-copy-mnemonic") {
    if (state.onboardingWalletMnemonic) {
      void navigator.clipboard.writeText(state.onboardingWalletMnemonic);
      showToast("Copied recovery phrase to clipboard");
    }
    return;
  }

  if (action === "onboarding-wallet-done") {
    showOverlayLoader("Loading markets...");
    (async () => {
      await finishOnboarding();
      hideOverlayLoader();
    })();
    return;
  }

  if (action === "onboarding-restore-wallet") {
    if (
      !state.onboardingWalletMnemonic.trim() ||
      !state.onboardingWalletPassword
    ) {
      state.onboardingError = "Recovery phrase and password are required.";
      render();
      return;
    }
    state.onboardingLoading = true;
    state.onboardingError = "";
    showOverlayLoader("Restoring wallet...");
    render();
    (async () => {
      try {
        await invoke("restore_wallet", {
          mnemonic: state.onboardingWalletMnemonic.trim(),
          password: state.onboardingWalletPassword,
        });
        updateOverlayMessage("Scanning blockchain...");
        await invoke("sync_wallet");
        updateOverlayMessage("Loading markets...");
        showToast("Wallet restored!", "success");
        await finishOnboarding();
        hideOverlayLoader();
      } catch (e) {
        state.onboardingError = String(e);
        state.onboardingLoading = false;
        hideOverlayLoader();
        render();
      }
    })();
    return;
  }

  if (action === "onboarding-nostr-restore-wallet") {
    if (!state.onboardingWalletPassword) {
      state.onboardingError = "Password is required.";
      render();
      return;
    }
    state.onboardingLoading = true;
    state.onboardingError = "";
    showOverlayLoader("Fetching backup from relays...");
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("restore_mnemonic_from_nostr");
        updateOverlayMessage("Restoring wallet...");
        await invoke("restore_wallet", {
          mnemonic: mnemonic.trim(),
          password: state.onboardingWalletPassword,
        });
        updateOverlayMessage("Scanning blockchain...");
        await invoke("sync_wallet");
        updateOverlayMessage("Loading markets...");
        showToast("Wallet restored from Nostr backup!", "success");
        await finishOnboarding();
        hideOverlayLoader();
      } catch (e) {
        state.onboardingError = String(e);
        state.onboardingLoading = false;
        hideOverlayLoader();
        render();
      }
    })();
    return;
  }

  // ── App actions ──────────────────────────────────────────────────

  if (action === "go-home") {
    state.view = "home";
    render();
    return;
  }

  if (action === "toggle-user-menu") {
    state.userMenuOpen = !state.userMenuOpen;
    render();
    return;
  }

  if (action === "open-search") {
    state.searchOpen = true;
    render();
    return;
  }

  if (action === "close-search") {
    state.searchOpen = false;
    render();
    return;
  }

  if (action === "open-help") {
    state.helpOpen = true;
    render();
    return;
  }

  if (action === "close-help") {
    state.helpOpen = false;
    render();
    return;
  }

  if (action === "copy-nostr-npub") {
    if (state.nostrNpub) {
      void navigator.clipboard.writeText(state.nostrNpub);
      showToast("Copied npub to clipboard");
    }
    return;
  }

  if (action === "set-currency") {
    const currency = actionEl?.getAttribute(
      "data-currency",
    ) as BaseCurrency | null;
    if (currency) {
      state.baseCurrency = currency;
      render();
    }
    return;
  }

  if (action === "user-settings") {
    state.userMenuOpen = false;
    state.settingsOpen = true;
    render();
    return;
  }

  if (action === "toggle-settings-section") {
    const section = actionEl?.getAttribute("data-section");
    if (section) {
      state.settingsSection[section] = !state.settingsSection[section];
      render();
    }
    return;
  }

  if (action === "close-settings") {
    state.settingsOpen = false;
    state.nostrNsecRevealed = null;
    state.nostrReplacePrompt = false;
    state.nostrReplacePanel = false;
    state.nostrReplaceConfirm = "";
    state.nostrImportNsec = "";
    state.walletDeletePrompt = false;
    state.walletDeleteConfirm = "";
    state.devResetPrompt = false;
    state.devResetConfirm = "";
    render();
    return;
  }

  if (action === "reveal-nostr-nsec") {
    (async () => {
      try {
        const nsec = await invoke<string>("export_nostr_nsec");
        state.nostrNsecRevealed = nsec;
        render();
      } catch (e) {
        showToast("Failed to export nsec: " + String(e), "error");
      }
    })();
    return;
  }

  if (action === "copy-nostr-nsec") {
    if (state.nostrNsecRevealed) {
      void navigator.clipboard.writeText(state.nostrNsecRevealed);
      state.nostrNsecRevealed = null;
      showToast("Copied nsec to clipboard");
      render();
    }
    return;
  }

  if (action === "nostr-replace-start") {
    state.nostrReplacePrompt = true;
    state.nostrReplaceConfirm = "";
    render();
    const input = document.getElementById(
      "nostr-replace-confirm",
    ) as HTMLInputElement | null;
    input?.focus();
    return;
  }

  if (action === "nostr-replace-cancel") {
    state.nostrReplacePrompt = false;
    state.nostrReplaceConfirm = "";
    render();
    return;
  }

  if (action === "nostr-replace-confirm") {
    if (state.nostrReplaceConfirm.trim().toUpperCase() !== "DELETE") return;
    (async () => {
      try {
        await invoke("delete_nostr_identity");
        state.nostrPubkey = null;
        state.nostrNpub = null;
        state.nostrNsecRevealed = null;
        state.nostrReplacePanel = true;
        state.nostrReplacePrompt = false;
        state.nostrReplaceConfirm = "";
      } catch (e) {
        showToast("Failed to delete identity: " + String(e), "error");
      }
      render();
    })();
    return;
  }

  if (action === "nostr-replace-back") {
    state.nostrReplacePanel = false;
    state.nostrImportNsec = "";
    render();
    return;
  }

  if (action === "import-nostr-nsec") {
    const nsecInput = state.nostrImportNsec.trim();
    if (!nsecInput) {
      showToast("Paste an nsec to import", "error");
      return;
    }
    state.nostrImporting = true;
    render();
    (async () => {
      try {
        const identity = await invoke<{ pubkey_hex: string; npub: string }>(
          "import_nostr_nsec",
          { nsec: nsecInput },
        );
        state.nostrPubkey = identity.pubkey_hex;
        state.nostrNpub = identity.npub;
        state.nostrImportNsec = "";
        state.nostrNsecRevealed = null;
        state.nostrReplacePanel = false;
        state.nostrReplaceConfirm = "";
        showToast("Nostr key imported successfully", "success");
      } catch (e) {
        showToast("Import failed: " + String(e), "error");
      }
      state.nostrImporting = false;
      render();
    })();
    return;
  }

  if (action === "generate-new-nostr-key") {
    (async () => {
      try {
        const identity = await invoke<IdentityResponse>(
          "generate_nostr_identity",
        );
        state.nostrPubkey = identity.pubkey_hex;
        state.nostrNpub = identity.npub;
        state.nostrNsecRevealed = null;
        state.nostrReplacePanel = false;
        state.nostrReplaceConfirm = "";
        showToast("New Nostr keypair generated", "success");
      } catch (e) {
        showToast("Failed: " + String(e), "error");
      }
      render();
    })();
    return;
  }

  if (action === "dev-restart") {
    state.settingsOpen = false;
    render();
    const splash = document.createElement("div");
    splash.id = "splash";
    splash.innerHTML = loaderHtml();
    document.body.appendChild(splash);
    setTimeout(() => {
      splash.classList.add("fade-out");
      splash.addEventListener("transitionend", () => splash.remove(), {
        once: true,
      });
    }, 4800);
    return;
  }

  if (action === "dev-reset-start") {
    state.devResetPrompt = true;
    state.devResetConfirm = "";
    render();
    return;
  }

  if (action === "dev-reset-cancel") {
    state.devResetPrompt = false;
    state.devResetConfirm = "";
    render();
    return;
  }

  if (action === "dev-reset-confirm") {
    if (state.devResetConfirm.trim().toUpperCase() !== "RESET") return;
    (async () => {
      try {
        await invoke("delete_nostr_identity");
        try {
          await invoke("delete_wallet");
        } catch (_) {
          /* no wallet is fine */
        }
        state.nostrPubkey = null;
        state.nostrNpub = null;
        state.nostrNsecRevealed = null;
        state.walletBalance = null;
        state.walletTransactions = [];
        state.walletSwaps = [];
        state.walletPassword = "";
        state.walletMnemonic = "";
        state.settingsOpen = false;
        state.devResetPrompt = false;
        state.devResetConfirm = "";
        await fetchWalletStatus();
        state.onboardingStep = "nostr";
        render();
        showToast("App data erased", "success");
      } catch (e) {
        showToast("Reset failed: " + String(e), "error");
      }
    })();
    return;
  }

  // ── Relay management handlers ──

  if (action === "add-relay") {
    const input = document.getElementById(
      "relay-input",
    ) as HTMLInputElement | null;
    const url = (input?.value ?? state.relayInput).trim();
    if (!url) return;
    if (!url.startsWith("wss://") && !url.startsWith("ws://")) {
      showToast("Relay URL must start with wss://", "error");
      return;
    }
    state.relayLoading = true;
    render();
    (async () => {
      try {
        const list = await invoke<string[]>("add_relay", { url });
        state.relays = list.map((u) => ({ url: u, has_backup: false }));
        state.relayInput = "";
        showToast("Relay added", "success");
      } catch (e) {
        showToast("Failed to add relay: " + String(e), "error");
      } finally {
        state.relayLoading = false;
        render();
      }
    })();
    return;
  }

  if (action === "remove-relay") {
    const url = actionEl?.dataset.relay;
    if (!url) return;
    state.relayLoading = true;
    render();
    (async () => {
      try {
        const list = await invoke<string[]>("remove_relay", { url });
        state.relays = list.map((u) => ({ url: u, has_backup: false }));
        showToast("Relay removed", "success");
      } catch (e) {
        showToast("Failed to remove relay: " + String(e), "error");
      } finally {
        state.relayLoading = false;
        render();
      }
    })();
    return;
  }

  if (action === "reset-relays") {
    state.relayLoading = true;
    render();
    (async () => {
      try {
        await invoke("set_relay_list", {
          relays: ["wss://relay.damus.io", "wss://relay.primal.net"],
        });
        state.relays = [
          { url: "wss://relay.damus.io", has_backup: false },
          { url: "wss://relay.primal.net", has_backup: false },
        ];
        showToast("Relays reset to defaults", "success");
      } catch (e) {
        showToast("Failed to reset relays: " + String(e), "error");
      } finally {
        state.relayLoading = false;
        render();
      }
    })();
    return;
  }

  // ── Nostr backup handlers ──

  if (action === "nostr-backup-wallet") {
    state.nostrBackupLoading = true;
    render();
    (async () => {
      try {
        await invoke("backup_mnemonic_to_nostr", { password: "" });
        // Refresh backup status
        const status = await invoke<NostrBackupStatus>("check_nostr_backup");
        state.nostrBackupStatus = status;
        // Update relay backup indicators
        if (status.relay_results) {
          state.relays = state.relays.map((r) => ({
            ...r,
            has_backup:
              status.relay_results.find(
                (rr: RelayBackupResult) => rr.url === r.url,
              )?.has_backup ?? false,
          }));
        }
        showToast("Wallet backed up to Nostr relays", "success");
      } catch (e) {
        showToast("Backup failed: " + String(e), "error");
      } finally {
        state.nostrBackupLoading = false;
        render();
      }
    })();
    return;
  }

  if (action === "cancel-backup-prompt") {
    state.nostrBackupPrompt = false;
    state.nostrBackupPassword = "";
    render();
    return;
  }

  if (action === "settings-backup-wallet") {
    // If wallet is locked and password prompt not yet shown, show it
    if (state.walletStatus !== "unlocked" && !state.nostrBackupPrompt) {
      state.nostrBackupPrompt = true;
      render();
      return;
    }
    const password =
      state.walletStatus === "unlocked" ? "" : state.nostrBackupPassword;
    if (state.walletStatus !== "unlocked" && !password) {
      showToast("Enter your wallet password", "error");
      return;
    }
    state.nostrBackupLoading = true;
    render();
    (async () => {
      try {
        await invoke("backup_mnemonic_to_nostr", { password });
        const status = await invoke<NostrBackupStatus>("check_nostr_backup");
        state.nostrBackupStatus = status;
        if (status.relay_results) {
          state.relays = state.relays.map((r) => ({
            ...r,
            has_backup:
              status.relay_results.find(
                (rr: RelayBackupResult) => rr.url === r.url,
              )?.has_backup ?? false,
          }));
        }
        state.nostrBackupPassword = "";
        state.nostrBackupPrompt = false;
        showToast("Wallet backed up to Nostr relays", "success");
      } catch (e) {
        showToast("Backup failed: " + String(e), "error");
      } finally {
        state.nostrBackupLoading = false;
        render();
      }
    })();
    return;
  }

  if (action === "delete-nostr-backup") {
    if (
      !confirm(
        "Delete your encrypted wallet backup from all relays? You can re-upload it later.",
      )
    )
      return;
    state.nostrBackupLoading = true;
    render();
    (async () => {
      try {
        await invoke("delete_nostr_backup");
        const status = await invoke<NostrBackupStatus>("check_nostr_backup");
        state.nostrBackupStatus = status;
        if (status.relay_results) {
          state.relays = state.relays.map((r) => ({
            ...r,
            has_backup:
              status.relay_results.find(
                (rr: RelayBackupResult) => rr.url === r.url,
              )?.has_backup ?? false,
          }));
        }
        showToast("Backup deletion request sent to relays", "success");
      } catch (e) {
        showToast("Delete failed: " + String(e), "error");
      } finally {
        state.nostrBackupLoading = false;
        render();
      }
    })();
    return;
  }

  if (action === "nostr-restore-wallet") {
    showOverlayLoader("Fetching backup from relays...");
    (async () => {
      try {
        const mnemonic = await invoke<string>("restore_mnemonic_from_nostr");
        hideOverlayLoader();
        // Pre-fill the mnemonic in the restore form
        state.walletShowRestore = true;
        state.walletRestoreMnemonic = mnemonic;
        render();
        showToast("Recovery phrase retrieved from Nostr", "success");
      } catch (e) {
        hideOverlayLoader();
        showToast("No backup found: " + String(e), "error");
      }
    })();
    return;
  }

  if (action === "user-logout") {
    state.userMenuOpen = false;
    state.logoutOpen = true;
    render();
    return;
  }

  if (action === "close-logout") {
    state.logoutOpen = false;
    render();
    return;
  }

  if (action === "confirm-logout") {
    state.logoutOpen = false;
    (async () => {
      try {
        await invoke("lock_wallet");
        await fetchWalletStatus();
        state.walletBalance = null;
        state.walletTransactions = [];
        state.walletSwaps = [];
        state.walletPassword = "";
        state.walletModal = "none";
        state.walletShowBackup = false;
        state.walletBackupMnemonic = "";
        state.walletBackupPassword = "";
        resetReceiveState();
        resetSendState();
        state.view = "home";
      } catch (e) {
        console.warn("Failed to lock wallet:", e);
      }
      render();
    })();
    return;
  }

  if (action === "open-create-market") {
    state.view = "create";
    render();
    return;
  }

  if (action === "open-wallet") {
    state.walletError = "";
    state.walletPassword = "";
    state.settingsOpen = false;
    state.view = "wallet";
    render();
    // If already unlocked with cached balance, just do a silent background sync
    if (state.walletStatus === "unlocked" && state.walletBalance) {
      void invoke("sync_wallet")
        .then(async () => {
          const balance = await invoke<{ assets: Record<string, number> }>(
            "get_wallet_balance",
          );
          state.walletBalance = balance.assets;
          const txs = await invoke<
            {
              txid: string;
              balanceChange: number;
              fee: number;
              height: number | null;
              timestamp: number | null;
              txType: string;
            }[]
          >("get_wallet_transactions");
          state.walletTransactions = txs;
          render();
        })
        .catch(() => {});
    } else {
      void fetchWalletStatus().then(() => {
        render();
        if (state.walletStatus === "unlocked") {
          void refreshWallet();
        }
      });
    }
    return;
  }

  if (action === "create-wallet") {
    if (!state.walletPassword) {
      state.walletError = "Password is required.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Creating wallet...");
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("create_wallet", {
          password: state.walletPassword,
        });
        state.walletMnemonic = mnemonic;
        state.walletPassword = "";
        await fetchWalletStatus();
        // Stay on not_created so mnemonic screen shows
        state.walletStatus = "not_created";
      } catch (e) {
        state.walletError = String(e);
      }
      state.walletLoading = false;
      hideOverlayLoader();
      render();
    })();
    return;
  }

  if (action === "dismiss-mnemonic") {
    state.walletMnemonic = "";
    state.walletStatus = "locked";
    state.walletPassword = "";
    render();
    return;
  }

  if (action === "toggle-restore") {
    state.walletShowRestore = !state.walletShowRestore;
    state.walletError = "";
    render();
    return;
  }

  if (action === "restore-wallet") {
    if (!state.walletRestoreMnemonic.trim() || !state.walletPassword) {
      state.walletError = "Recovery phrase and password are required.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Restoring wallet...");
    render();
    (async () => {
      try {
        await invoke("restore_wallet", {
          mnemonic: state.walletRestoreMnemonic.trim(),
          password: state.walletPassword,
        });
        state.walletRestoreMnemonic = "";
        state.walletPassword = "";
        updateOverlayMessage("Scanning blockchain...");
        await invoke("sync_wallet");
        await fetchWalletStatus();
        if (state.walletStatus === "unlocked") {
          const balance = await invoke<{ assets: Record<string, number> }>(
            "get_wallet_balance",
          );
          state.walletBalance = balance.assets;
          const txs = await invoke<
            {
              txid: string;
              balanceChange: number;
              fee: number;
              height: number | null;
              timestamp: number | null;
              txType: string;
            }[]
          >("get_wallet_transactions");
          state.walletTransactions = txs;
        }
        state.walletLoading = false;
        hideOverlayLoader();
        render();
        showToast("Wallet restored successfully", "success");
      } catch (e) {
        state.walletError = String(e);
        state.walletLoading = false;
        hideOverlayLoader();
        render();
      }
    })();
    return;
  }

  if (action === "unlock-wallet") {
    if (!state.walletPassword) {
      state.walletError = "Password is required.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Unlocking wallet...");
    render();
    (async () => {
      try {
        await invoke("unlock_wallet", { password: state.walletPassword });
        state.walletPassword = "";
        await fetchWalletStatus();
        // Load cached wallet data instantly (no Electrum sync)
        const [balance, txs, swaps] = await Promise.all([
          invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
          invoke<
            {
              txid: string;
              balanceChange: number;
              fee: number;
              height: number | null;
              timestamp: number | null;
              txType: string;
            }[]
          >("get_wallet_transactions"),
          invoke<PaymentSwap[]>("list_payment_swaps"),
        ]);
        state.walletBalance = balance.assets;
        state.walletTransactions = txs;
        state.walletSwaps = swaps;
        state.walletLoading = false;
        hideOverlayLoader();
        render();
        // Background Electrum sync — updates balances when done
        invoke("sync_wallet")
          .then(async () => {
            const [freshBalance, freshTxs] = await Promise.all([
              invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
              invoke<
                {
                  txid: string;
                  balanceChange: number;
                  fee: number;
                  height: number | null;
                  timestamp: number | null;
                  txType: string;
                }[]
              >("get_wallet_transactions"),
            ]);
            state.walletBalance = freshBalance.assets;
            state.walletTransactions = freshTxs;
            render();
          })
          .catch(() => {
            /* silent background sync failure */
          });
      } catch (e) {
        state.walletError = String(e);
        state.walletLoading = false;
        hideOverlayLoader();
        render();
      }
    })();
    return;
  }

  if (action === "lock-wallet") {
    (async () => {
      try {
        await invoke("lock_wallet");
        await fetchWalletStatus();
        state.walletBalance = null;
        state.walletTransactions = [];
        state.walletSwaps = [];
        state.walletPassword = "";
        state.walletShowBackup = false;
        state.walletBackupMnemonic = "";
        state.walletBackupPassword = "";
        state.walletModal = "none";
        resetReceiveState();
        resetSendState();
        render();
      } catch (e) {
        state.walletError = String(e);
        render();
      }
    })();
    return;
  }

  if (action === "wallet-delete-start") {
    state.walletDeletePrompt = true;
    state.walletDeleteConfirm = "";
    render();
    return;
  }

  if (action === "wallet-delete-cancel") {
    state.walletDeletePrompt = false;
    state.walletDeleteConfirm = "";
    render();
    return;
  }

  if (action === "wallet-delete-confirm") {
    if (state.walletDeleteConfirm.trim().toUpperCase() !== "DELETE") return;
    (async () => {
      try {
        await invoke("delete_wallet");
        await fetchWalletStatus();
        state.walletBalance = null;
        state.walletTransactions = [];
        state.walletSwaps = [];
        state.walletPassword = "";
        state.walletMnemonic = "";
        state.walletModal = "none";
        state.walletShowBackup = false;
        state.walletBackupMnemonic = "";
        state.walletBackupPassword = "";
        resetReceiveState();
        resetSendState();
        state.walletDeletePrompt = false;
        state.walletDeleteConfirm = "";
        state.settingsOpen = false;
        showToast("Wallet removed", "success");
      } catch (e) {
        showToast("Failed to remove wallet: " + String(e), "error");
      }
      render();
    })();
    return;
  }

  if (action === "forgot-password-delete") {
    (async () => {
      try {
        await invoke("delete_wallet");
        await fetchWalletStatus();
        state.walletBalance = null;
        state.walletTransactions = [];
        state.walletSwaps = [];
        state.walletPassword = "";
        state.walletMnemonic = "";
        state.walletModal = "none";
        state.walletShowBackup = false;
        state.walletBackupMnemonic = "";
        state.walletBackupPassword = "";
        resetReceiveState();
        resetSendState();
        showToast(
          "Wallet removed — restore from backup or recovery phrase",
          "info",
        );
      } catch (e) {
        showToast("Failed to remove wallet: " + String(e), "error");
      }
      render();
    })();
    return;
  }

  if (action === "toggle-balance-hidden") {
    state.walletBalanceHidden = !state.walletBalanceHidden;
    render();
    return;
  }

  if (action === "set-wallet-unit") {
    const unit = actionEl?.getAttribute("data-unit") as "sats" | "btc" | null;
    if (unit) {
      state.walletUnit = unit;
      render();
    }
    return;
  }

  if (action === "sync-wallet") {
    void refreshWallet();
    return;
  }

  if (action === "open-explorer-tx") {
    const txid = actionEl?.getAttribute("data-txid");
    if (txid) {
      const base =
        state.walletNetwork === "testnet"
          ? "https://blockstream.info/liquidtestnet"
          : "https://blockstream.info/liquid";
      void openUrl(base + "/tx/" + txid);
    }
    return;
  }

  if (action === "open-nostr-event") {
    const nevent = actionEl?.getAttribute("data-nevent");
    if (nevent) {
      void openUrl("https://njump.me/" + nevent);
    }
    return;
  }

  if (action === "open-receive") {
    state.walletModal = "receive";
    state.walletModalTab = "lightning";
    resetReceiveState();
    render();
    (async () => {
      try {
        const pairs = await invoke<BoltzChainSwapPairsInfo>(
          "get_chain_swap_pairs",
        );
        state.receiveBtcPairInfo = pairs.bitcoinToLiquid;
      } catch {
        /* ignore */
      }
      render();
    })();
    return;
  }

  if (action === "open-send") {
    state.walletModal = "send";
    state.walletModalTab = "lightning";
    resetSendState();
    render();
    (async () => {
      try {
        const pairs = await invoke<BoltzChainSwapPairsInfo>(
          "get_chain_swap_pairs",
        );
        state.sendBtcPairInfo = pairs.liquidToBitcoin;
      } catch {
        /* ignore */
      }
      render();
    })();
    return;
  }

  if (action === "close-modal") {
    state.walletModal = "none";
    resetReceiveState();
    resetSendState();
    render();
    return;
  }

  if (action === "modal-backdrop" && actionEl === target) {
    state.walletModal = "none";
    resetReceiveState();
    resetSendState();
    render();
    return;
  }

  if (action === "modal-tab") {
    const tab = actionEl?.getAttribute("data-tab-value") as
      | "lightning"
      | "liquid"
      | "bitcoin"
      | null;
    if (tab) {
      state.walletModalTab = tab;
      state.modalQr = "";
      render();
    }
    return;
  }

  if (action === "receive-preset") {
    const preset = actionEl?.getAttribute("data-preset") ?? "";
    if (preset) {
      state.receiveAmount = preset;
      render();
    }
    return;
  }

  if (action === "create-lightning-receive") {
    const amt = Math.floor(Number(state.receiveAmount) || 0);
    if (amt <= 0) {
      state.receiveError = "Enter a valid amount.";
      render();
      return;
    }
    state.receiveCreating = true;
    state.receiveError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzLightningReceiveCreated>(
          "create_lightning_receive",
          { amountSat: amt },
        );
        state.receiveLightningSwap = swap;
        await generateQr(swap.invoice);
      } catch (e) {
        state.receiveError = String(e);
      }
      state.receiveCreating = false;
      render();
    })();
    return;
  }

  if (action === "generate-liquid-address") {
    (async () => {
      try {
        const addr = await invoke<{ address: string }>("get_wallet_address", {
          index: state.receiveLiquidAddressIndex,
        });
        state.receiveLiquidAddress = addr.address;
        await generateQr(addr.address);
        state.receiveLiquidAddressIndex += 1;
      } catch (e) {
        state.receiveError = String(e);
      }
      render();
    })();
    return;
  }

  if (action === "create-bitcoin-receive") {
    const amt = Math.floor(Number(state.receiveAmount) || 0);
    if (amt <= 0) {
      state.receiveError = "Enter a valid amount.";
      render();
      return;
    }
    state.receiveCreating = true;
    state.receiveError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzChainSwapCreated>(
          "create_bitcoin_receive",
          { amountSat: amt },
        );
        state.receiveBitcoinSwap = swap;
        const addr = swap.lockupAddress;
        await generateQr(swap.bip21 || addr);
      } catch (e) {
        state.receiveError = String(e);
      }
      state.receiveCreating = false;
      render();
    })();
    return;
  }

  if (action === "pay-lightning-invoice") {
    const invoice = state.sendInvoice.trim();
    if (!invoice) {
      state.sendError = "Paste a BOLT11 invoice.";
      render();
      return;
    }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzSubmarineSwapCreated>(
          "pay_lightning_invoice",
          { invoice },
        );
        state.sentLightningSwap = swap;
      } catch (e) {
        state.sendError = String(e);
      }
      state.sendCreating = false;
      render();
    })();
    return;
  }

  if (action === "send-liquid") {
    const address = state.sendLiquidAddress.trim();
    const amountSat = Math.floor(Number(state.sendLiquidAmount) || 0);
    if (!address || amountSat <= 0) {
      state.sendError = "Enter address and amount.";
      render();
      return;
    }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const result = await invoke<{ txid: string; feeSat: number }>(
          "send_lbtc",
          {
            address,
            amountSat,
            feeRate: null,
          },
        );
        state.sentLiquidResult = { txid: result.txid, feeSat: result.feeSat };
      } catch (e) {
        state.sendError = String(e);
      }
      state.sendCreating = false;
      render();
    })();
    return;
  }

  if (action === "create-bitcoin-send") {
    const amt = Math.floor(Number(state.sendBtcAmount) || 0);
    if (amt <= 0) {
      state.sendError = "Enter a valid amount.";
      render();
      return;
    }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzChainSwapCreated>(
          "create_bitcoin_send",
          { amountSat: amt },
        );
        state.sentBitcoinSwap = swap;
        const addr = swap.claimLockupAddress;
        await generateQr(swap.bip21 || addr);
      } catch (e) {
        state.sendError = String(e);
      }
      state.sendCreating = false;
      render();
    })();
    return;
  }

  if (action === "copy-modal-value") {
    const val = actionEl?.getAttribute("data-copy-value") ?? "";
    if (val) void navigator.clipboard.writeText(val);
    return;
  }

  if (action === "refresh-swap") {
    const swapId = actionEl?.getAttribute("data-swap-id") ?? "";
    if (!swapId) return;
    (async () => {
      try {
        await invoke("refresh_payment_swap_status", { swapId });
        const swaps = await invoke<PaymentSwap[]>("list_payment_swaps");
        state.walletSwaps = swaps;
      } catch (e) {
        state.walletError = String(e);
      }
      render();
    })();
    return;
  }

  if (action === "copy-mnemonic") {
    void navigator.clipboard.writeText(state.walletMnemonic);
    return;
  }

  if (action === "show-backup") {
    state.walletShowBackup = true;
    state.walletBackupMnemonic = "";
    state.walletBackupPassword = "";
    state.walletError = "";
    render();
    return;
  }

  if (action === "hide-backup") {
    state.walletShowBackup = false;
    state.walletBackupMnemonic = "";
    state.walletBackupPassword = "";
    render();
    return;
  }

  if (action === "export-backup") {
    if (!state.walletBackupPassword) {
      state.walletError = "Password is required to export recovery phrase.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Decrypting backup...");
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("get_wallet_mnemonic", {
          password: state.walletBackupPassword,
        });
        state.walletBackupMnemonic = mnemonic;
        state.walletBackupPassword = "";
        state.walletBackedUp = true;
      } catch (e) {
        state.walletError = String(e);
      }
      state.walletLoading = false;
      hideOverlayLoader();
      render();
    })();
    return;
  }

  if (action === "copy-backup-mnemonic") {
    void navigator.clipboard.writeText(state.walletBackupMnemonic);
    return;
  }

  if (action === "cancel-create-market") {
    state.view = "home";
    render();
    return;
  }

  if (action === "oracle-attest-yes" || action === "oracle-attest-no") {
    const market = getSelectedMarket();
    const outcomeYes = action === "oracle-attest-yes";
    const outcomeLabel = outcomeYes ? "YES" : "NO";
    const confirmed = window.confirm(
      `Resolve "${market.question}" as ${outcomeLabel}?\n\nThis publishes a Schnorr signature to Nostr that permanently attests the outcome. This cannot be undone.`,
    );
    if (!confirmed) return;

    (async () => {
      try {
        const result = await invoke<AttestationResult>("oracle_attest", {
          marketIdHex: market.marketId,
          outcomeYes,
        });
        // Save attestation for on-chain execution
        state.lastAttestationSig = result.signature_hex;
        state.lastAttestationOutcome = outcomeYes;
        state.lastAttestationMarketId = market.marketId;
        market.resolveTx = {
          txid: result.nostr_event_id,
          outcome: outcomeYes ? "yes" : "no",
          sigVerified: true,
          height: market.currentHeight,
          signatureHash: result.signature_hex.slice(0, 16) + "...",
        };
        showToast(
          `Attestation published to Nostr! Now execute on-chain to finalize.`,
          "success",
        );
        render();
      } catch (error) {
        window.alert(`Failed to attest: ${error}`);
      }
    })();
    return;
  }

  if (action === "execute-resolution") {
    const market = getSelectedMarket();
    if (!state.lastAttestationSig || state.lastAttestationOutcome === null) {
      showToast("No attestation available to execute", "error");
      return;
    }
    const outcomeYes = state.lastAttestationOutcome;
    const confirmed = window.confirm(
      `Execute on-chain resolution for "${market.question}"?\n\nOutcome: ${outcomeYes ? "YES" : "NO"}\nThis submits a Liquid transaction that transitions the covenant state.`,
    );
    if (!confirmed) return;

    state.resolutionExecuting = true;
    render();
    (async () => {
      try {
        const result = await invoke<{
          txid: string;
          previous_state: number;
          new_state: number;
          outcome_yes: boolean;
        }>("resolve_market", {
          contractParamsJson: marketToContractParamsJson(market),
          outcomeYes,
          oracleSignatureHex: state.lastAttestationSig,
        });
        market.state = result.outcome_yes ? 2 : 3;
        state.lastAttestationSig = null;
        state.lastAttestationOutcome = null;
        state.lastAttestationMarketId = null;
        showToast(
          `Resolution executed! txid: ${result.txid.slice(0, 16)}... State: ${result.new_state}`,
          "success",
        );
        await refreshWallet();
      } catch (error) {
        showToast(`Resolution failed: ${error}`, "error");
      } finally {
        state.resolutionExecuting = false;
        render();
      }
    })();
    return;
  }

  if (action === "refresh-market-state") {
    const market = getSelectedMarket();
    if (!market.creationTxid) {
      showToast("Market has no on-chain creation tx", "error");
      return;
    }
    showToast("Querying on-chain market state...", "info");
    (async () => {
      try {
        const result = await invoke<{ state: number }>("get_market_state", {
          contractParamsJson: marketToContractParamsJson(market),
        });
        market.state = result.state as CovenantState;
        showToast(`Market state: ${stateLabel(market.state)}`, "success");
        render();
      } catch (error) {
        showToast(`State query failed: ${error}`, "error");
      }
    })();
    return;
  }

  if (action === "toggle-advanced-details") {
    state.showAdvancedDetails = !state.showAdvancedDetails;
    render();
    return;
  }

  if (action === "toggle-advanced-actions") {
    state.showAdvancedActions = !state.showAdvancedActions;
    if (state.showAdvancedActions && state.actionTab === "trade") {
      state.actionTab = "issue";
    }
    render();
    return;
  }

  if (action === "toggle-orderbook") {
    state.showOrderbook = !state.showOrderbook;
    render();
    return;
  }

  if (action === "toggle-fee-details") {
    state.showFeeDetails = !state.showFeeDetails;
    render();
    return;
  }

  if (action === "use-cashout") {
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const closeSide: Side = positions.yes >= positions.no ? "yes" : "no";
    const available = closeSide === "yes" ? positions.yes : positions.no;
    state.tradeIntent = "close";
    state.sizeMode = "contracts";
    state.selectedSide = closeSide;
    state.tradeContracts = Math.max(0.01, Math.min(available, available / 2));
    state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    setLimitPriceSats(getBasePriceSats(market, closeSide));
    render();
    return;
  }

  if (action === "sell-max") {
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const available =
      state.selectedSide === "yes" ? positions.yes : positions.no;
    state.tradeIntent = "close";
    state.sizeMode = "contracts";
    state.tradeContracts = Math.max(0.01, available);
    state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    render();
    return;
  }

  if (action === "sell-25" || action === "sell-50") {
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const available =
      state.selectedSide === "yes" ? positions.yes : positions.no;
    const ratio = action === "sell-25" ? 0.25 : 0.5;
    state.tradeIntent = "close";
    state.sizeMode = "contracts";
    state.tradeContracts = Math.max(0.01, available * ratio);
    state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    render();
    return;
  }

  if (action === "trending-prev") {
    const total = getTrendingMarkets().length;
    state.trendingIndex = (state.trendingIndex - 1 + total) % total;
    render();
    return;
  }

  if (action === "trending-next") {
    const total = getTrendingMarkets().length;
    state.trendingIndex = (state.trendingIndex + 1) % total;
    render();
    return;
  }

  if (side) {
    state.selectedSide = side;
    const market = getSelectedMarket();
    setLimitPriceSats(getBasePriceSats(market, side));
    render();
    return;
  }

  if (tradeChoiceRaw) {
    const [intentRaw, sideRaw] = tradeChoiceRaw.split(":");
    const intent = intentRaw as TradeIntent;
    const pickedSide = sideRaw as Side;
    if (
      (intent === "open" || intent === "close") &&
      (pickedSide === "yes" || pickedSide === "no")
    ) {
      state.tradeIntent = intent;
      state.selectedSide = pickedSide;
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const available = pickedSide === "yes" ? positions.yes : positions.no;
      setLimitPriceSats(getBasePriceSats(market, pickedSide));
      if (intent === "close") {
        state.sizeMode = "contracts";
        state.tradeContracts = Math.max(
          0.01,
          Math.min(Math.max(0.01, available), state.tradeContracts),
        );
        state.tradeContractsDraft = state.tradeContracts.toFixed(2);
      }
      render();
      return;
    }
  }

  if (tradeIntent) {
    state.tradeIntent = tradeIntent;
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const available =
      state.selectedSide === "yes" ? positions.yes : positions.no;
    if (tradeIntent === "close") {
      state.sizeMode = "contracts";
      state.tradeContracts = Math.max(
        0.01,
        Math.min(Math.max(0.01, available), state.tradeContracts),
      );
      state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    }
    render();
    return;
  }

  if (sizeMode) {
    state.sizeMode = sizeMode;
    render();
    return;
  }

  if (Number.isFinite(tradeSizePreset) && tradeSizePreset > 0) {
    state.sizeMode = "sats";
    state.tradeSizeSats = Math.floor(tradeSizePreset);
    state.tradeSizeSatsDraft = formatSatsInput(state.tradeSizeSats);
    render();
    return;
  }

  if (Number.isFinite(tradeSizeDelta) && tradeSizeDelta !== 0) {
    state.sizeMode = "sats";
    const current = Math.max(
      1,
      Math.floor(Number(state.tradeSizeSatsDraft.replace(/,/g, "")) || 1),
    );
    const next = Math.max(1, current + Math.floor(tradeSizeDelta));
    state.tradeSizeSats = next;
    state.tradeSizeSatsDraft = formatSatsInput(next);
    render();
    return;
  }

  if (
    action === "step-limit-price" &&
    Number.isFinite(limitPriceDelta) &&
    limitPriceDelta !== 0
  ) {
    const currentSats = clampContractPriceSats(
      state.limitPriceDraft.length > 0
        ? Number(state.limitPriceDraft)
        : state.limitPrice * SATS_PER_FULL_CONTRACT,
    );
    setLimitPriceSats(currentSats + Math.sign(limitPriceDelta));
    render();
    return;
  }

  if (orderType) {
    state.orderType = orderType;
    if (orderType === "limit") {
      commitLimitPriceDraft();
    }
    render();
    return;
  }

  if (tab) {
    const market = getSelectedMarket();
    if (ticketActionAllowed(market, tab)) {
      state.actionTab = tab;
      render();
    }
    return;
  }

  if (
    action === "submit-trade" ||
    action === "submit-issue" ||
    action === "submit-redeem" ||
    action === "submit-cancel" ||
    action === "submit-create-market"
  ) {
    if (action === "submit-create-market") {
      const question = state.createQuestion.trim();
      const description = state.createDescription.trim();
      const source = state.createResolutionSource.trim();
      if (
        !question ||
        !description ||
        !source ||
        !state.createSettlementInput
      ) {
        window.alert(
          "Complete question, settlement rule, source, and settlement deadline before creating.",
        );
        return;
      }
      const yesSats = Math.max(
        1,
        Math.min(
          SATS_PER_FULL_CONTRACT - 1,
          Math.round(state.createStartingYesSats),
        ),
      );
      const deadlineUnix = Math.floor(
        new Date(state.createSettlementInput).getTime() / 1000,
      );

      state.marketCreating = true;
      render();
      (async () => {
        try {
          const result = await invoke<DiscoveredMarket>(
            "create_contract_onchain",
            {
              request: {
                question,
                description,
                category: state.createCategory,
                resolution_source: source,
                starting_yes_price: yesSats,
                settlement_deadline_unix: deadlineUnix,
                collateral_per_token: 5000,
              },
            },
          );
          // Ingest the newly created market into the store
          await invoke("ingest_discovered_markets", { markets: [result] });
          markets.push(discoveredToMarket(result));
          state.view = "home";
          state.createQuestion = "";
          state.createDescription = "";
          state.createResolutionSource = "";
          state.createSettlementInput = defaultSettlementInput();
          state.createStartingYesSats = 50;
          showToast(
            `Market created! txid: ${result.creation_txid ?? "unknown"}`,
            "success",
          );
        } catch (error) {
          showToast(`Failed to create market: ${error}`, "error");
        } finally {
          state.marketCreating = false;
          render();
        }
      })();
      return;
    }

    const market = getSelectedMarket();
    if (action === "submit-trade") {
      const preview = getTradePreview(market);
      const pairs = Math.max(1, Math.floor(preview.requestedContracts));

      if (state.tradeIntent === "open") {
        // Buy = Issue pairs (mint YES+NO tokens, user keeps the side they want)
        if (!market.creationTxid) {
          showToast("Market has no creation txid — cannot trade", "error");
          return;
        }
        const paths = getPathAvailability(market);
        if (!paths.issue && !paths.initialIssue) {
          showToast("Market is not in a tradeable state for issuance", "error");
          return;
        }
        const collateralNeeded = pairs * 2 * market.cptSats;
        const confirmed = window.confirm(
          `Issue ${pairs} token pair(s) for "${market.question.slice(0, 50)}"?\n\nYou will receive ${pairs} YES + ${pairs} NO tokens.\nCollateral required: ${formatSats(collateralNeeded)}\n\nProceed?`,
        );
        if (!confirmed) return;

        showToast(`Issuing ${pairs} pair(s)...`, "info");
        (async () => {
          try {
            const result = await issueTokens(market, pairs);
            showToast(
              `Tokens issued! txid: ${result.txid.slice(0, 16)}...`,
              "success",
            );
            await refreshWallet();
          } catch (error) {
            showToast(`Issuance failed: ${error}`, "error");
          }
        })();
      } else {
        // Sell = Cancel pairs (burn equal YES+NO → reclaim collateral)
        const position = getPositionContracts(market);
        const maxPairs = Math.min(position.yes, position.no);
        if (maxPairs <= 0) {
          showToast(
            "You need both YES and NO tokens to cancel pairs. Use Advanced Actions for single-side operations.",
            "error",
          );
          return;
        }
        const actualPairs = Math.min(pairs, maxPairs);
        const refund = actualPairs * 2 * market.cptSats;
        const confirmed = window.confirm(
          `Cancel ${actualPairs} token pair(s) for "${market.question.slice(0, 50)}"?\n\nBurns ${actualPairs} YES + ${actualPairs} NO tokens.\nCollateral refund: ${formatSats(refund)}\n\nProceed?`,
        );
        if (!confirmed) return;

        showToast(`Cancelling ${actualPairs} pair(s)...`, "info");
        (async () => {
          try {
            const result = await invoke<{
              txid: string;
              previous_state: number;
              new_state: number;
              pairs_burned: number;
              is_full_cancellation: boolean;
            }>("cancel_tokens", {
              contractParamsJson: marketToContractParamsJson(market),
              pairs: actualPairs,
            });
            showToast(
              `Pairs cancelled! txid: ${result.txid.slice(0, 16)}... (${result.is_full_cancellation ? "full" : "partial"})`,
              "success",
            );
            await refreshWallet();
          } catch (error) {
            showToast(`Cancellation failed: ${error}`, "error");
          }
        })();
      }
      return;
    }

    if (action === "submit-issue") {
      const pairs = Math.max(1, Math.floor(state.pairsInput));
      if (!market.creationTxid) {
        showToast("Market has no creation txid — cannot issue tokens", "error");
        return;
      }
      showToast(
        `Issuing ${pairs} pair(s) for ${market.question.slice(0, 40)}...`,
        "info",
      );
      (async () => {
        try {
          const result = await issueTokens(market, pairs);
          showToast(
            `Tokens issued! txid: ${result.txid.slice(0, 16)}...`,
            "success",
          );
        } catch (error) {
          showToast(`Issuance failed: ${error}`, "error");
        }
      })();
      return;
    }

    if (action === "submit-cancel") {
      const pairs = Math.max(1, Math.floor(state.pairsInput));
      showToast(
        `Cancelling ${pairs} pair(s) for ${market.question.slice(0, 40)}...`,
        "info",
      );
      (async () => {
        try {
          const result = await invoke<{
            txid: string;
            previous_state: number;
            new_state: number;
            pairs_burned: number;
            is_full_cancellation: boolean;
          }>("cancel_tokens", {
            contractParamsJson: marketToContractParamsJson(market),
            pairs,
          });
          showToast(
            `Tokens cancelled! txid: ${result.txid.slice(0, 16)}... (${result.is_full_cancellation ? "full" : "partial"})`,
            "success",
          );
          await refreshWallet();
        } catch (error) {
          showToast(`Cancellation failed: ${error}`, "error");
        }
      })();
      return;
    }

    if (action === "submit-redeem") {
      const tokens = Math.max(1, Math.floor(state.tokensInput));
      const paths = getPathAvailability(market);

      if (paths.redeem) {
        showToast(`Redeeming ${tokens} winning token(s)...`, "info");
        (async () => {
          try {
            const result = await invoke<{
              txid: string;
              previous_state: number;
              tokens_redeemed: number;
              payout_sats: number;
            }>("redeem_tokens", {
              contractParamsJson: marketToContractParamsJson(market),
              tokens,
            });
            showToast(
              `Redeemed! txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
              "success",
            );
            await refreshWallet();
          } catch (error) {
            showToast(`Redemption failed: ${error}`, "error");
          }
        })();
      } else if (paths.expiryRedeem) {
        // For expiry redemption, determine which token side the user holds
        const yesBalance =
          state.walletBalance?.[reverseHex(market.yesAssetId)] ?? 0;
        // Use whichever side the user holds (prefer YES if both)
        const tokenAssetHex =
          yesBalance > 0 ? market.yesAssetId : market.noAssetId;

        showToast(`Redeeming ${tokens} expired token(s)...`, "info");
        (async () => {
          try {
            const result = await invoke<{
              txid: string;
              previous_state: number;
              tokens_redeemed: number;
              payout_sats: number;
            }>("redeem_expired", {
              contractParamsJson: marketToContractParamsJson(market),
              tokenAssetHex: tokenAssetHex,
              tokens,
            });
            showToast(
              `Expired tokens redeemed! txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
              "success",
            );
            await refreshWallet();
          } catch (error) {
            showToast(`Expiry redemption failed: ${error}`, "error");
          }
        })();
      } else {
        showToast("No redemption path available for this market", "error");
      }
      return;
    }
  }
});

app.addEventListener("input", (event) => {
  const target = event.target as HTMLInputElement;

  if (target.id === "onboarding-nostr-nsec") {
    state.onboardingNostrNsec = target.value;
    return;
  }

  if (target.id === "onboarding-wallet-password") {
    state.onboardingWalletPassword = target.value;
    return;
  }

  if (target.id === "onboarding-wallet-mnemonic") {
    state.onboardingWalletMnemonic = (
      target as unknown as HTMLTextAreaElement
    ).value;
    return;
  }

  if (target.id === "global-search" || target.id === "global-search-mobile") {
    state.search = target.value;
    if (state.view === "home") render();
    return;
  }

  if (target.id === "trade-size-sats") {
    state.tradeSizeSatsDraft = target.value;
    return;
  }

  if (target.id === "trade-size-contracts") {
    state.tradeContractsDraft = target.value;
    return;
  }

  if (target.id === "limit-price") {
    state.limitPriceDraft = target.value.replace(/[^\d]/g, "").slice(0, 2);
    return;
  }

  if (target.id === "pairs-input") {
    state.pairsInput = Math.max(1, Math.floor(Number(target.value) || 1));
    render();
    return;
  }

  if (target.id === "tokens-input") {
    state.tokensInput = Math.max(1, Math.floor(Number(target.value) || 1));
    render();
    return;
  }

  if (target.id === "wallet-password") {
    state.walletPassword = target.value;
    return;
  }

  if (target.id === "wallet-restore-mnemonic") {
    state.walletRestoreMnemonic = (
      target as unknown as HTMLTextAreaElement
    ).value;
    return;
  }

  if (target.id === "nostr-import-nsec") {
    state.nostrImportNsec = target.value;
    return;
  }

  if (target.id === "nostr-replace-confirm") {
    state.nostrReplaceConfirm = target.value;
    const confirmBtn = document.querySelector(
      "[data-action='nostr-replace-confirm']",
    ) as HTMLButtonElement | null;
    if (confirmBtn) {
      const enabled = target.value.trim().toUpperCase() === "DELETE";
      confirmBtn.disabled = !enabled;
      confirmBtn.className = `shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${enabled ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`;
    }
    return;
  }

  if (target.id === "wallet-delete-confirm") {
    state.walletDeleteConfirm = target.value;
    const confirmBtn = document.querySelector(
      "[data-action='wallet-delete-confirm']",
    ) as HTMLButtonElement | null;
    if (confirmBtn) {
      const enabled = target.value.trim().toUpperCase() === "DELETE";
      confirmBtn.disabled = !enabled;
      confirmBtn.className = `shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${enabled ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`;
    }
    return;
  }

  if (target.id === "dev-reset-confirm") {
    state.devResetConfirm = target.value;
    const confirmBtn = document.querySelector(
      "[data-action='dev-reset-confirm']",
    ) as HTMLButtonElement | null;
    if (confirmBtn) {
      const enabled = target.value.trim().toUpperCase() === "RESET";
      confirmBtn.disabled = !enabled;
      confirmBtn.className = `shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${enabled ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`;
    }
    return;
  }

  if (target.id === "relay-input") {
    state.relayInput = target.value;
    return;
  }

  if (target.id === "receive-amount") {
    state.receiveAmount = target.value;
    return;
  }

  if (target.id === "send-invoice") {
    state.sendInvoice = target.value;
    return;
  }

  if (target.id === "send-liquid-address") {
    state.sendLiquidAddress = target.value;
    return;
  }

  if (target.id === "send-liquid-amount") {
    state.sendLiquidAmount = target.value;
    return;
  }

  if (target.id === "send-btc-amount") {
    state.sendBtcAmount = target.value;
    return;
  }

  if (target.id === "wallet-backup-password") {
    state.walletBackupPassword = target.value;
    return;
  }

  if (target.id === "settings-backup-password") {
    state.nostrBackupPassword = target.value;
    return;
  }

  if (target.id === "create-question") {
    state.createQuestion = target.value;
    return;
  }

  if (target.id === "create-description") {
    state.createDescription = target.value;
    return;
  }

  if (target.id === "create-category") {
    state.createCategory = target.value as MarketCategory;
    return;
  }

  if (target.id === "create-settlement") {
    state.createSettlementInput = target.value;
    return;
  }

  if (target.id === "create-resolution-source") {
    state.createResolutionSource = target.value;
    return;
  }

  if (target.id === "create-yes-sats") {
    const parsed = Math.round(Number(target.value) || 50);
    state.createStartingYesSats = Math.max(
      1,
      Math.min(SATS_PER_FULL_CONTRACT - 1, parsed),
    );
    return;
  }
});

app.addEventListener("keydown", (event) => {
  const target = event.target as HTMLInputElement;
  if (target.id === "limit-price") {
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const delta = event.key === "ArrowUp" ? 1 : -1;
      const currentSats = clampContractPriceSats(
        state.limitPriceDraft.length > 0
          ? Number(state.limitPriceDraft)
          : state.limitPrice * SATS_PER_FULL_CONTRACT,
      );
      setLimitPriceSats(currentSats + delta);
      render();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      commitLimitPriceDraft();
      render();
      return;
    }
  }

  if (event.key !== "Enter") return;

  if (target.id === "onboarding-nostr-nsec") {
    event.preventDefault();
    const btn = document.querySelector(
      "[data-action='onboarding-import-nostr']",
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "onboarding-wallet-password") {
    event.preventDefault();
    const actionName =
      state.onboardingWalletMode === "nostr-restore"
        ? "onboarding-nostr-restore-wallet"
        : state.onboardingWalletMode === "create"
          ? "onboarding-create-wallet"
          : "onboarding-restore-wallet";
    const btn = document.querySelector(
      `[data-action='${actionName}']`,
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "nostr-replace-confirm") {
    event.preventDefault();
    if (state.nostrReplaceConfirm.trim().toUpperCase() === "DELETE") {
      const btn = document.querySelector(
        "[data-action='nostr-replace-confirm']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "wallet-delete-confirm") {
    event.preventDefault();
    if (state.walletDeleteConfirm.trim().toUpperCase() === "DELETE") {
      const btn = document.querySelector(
        "[data-action='wallet-delete-confirm']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "dev-reset-confirm") {
    event.preventDefault();
    if (state.devResetConfirm.trim().toUpperCase() === "RESET") {
      const btn = document.querySelector(
        "[data-action='dev-reset-confirm']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "wallet-backup-password") {
    event.preventDefault();
    const btn = document.querySelector(
      "[data-action='export-backup']",
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "settings-backup-password") {
    event.preventDefault();
    const btn = document.querySelector(
      "[data-action='settings-backup-wallet']",
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "wallet-password") {
    event.preventDefault();
    if (state.walletStatus === "not_created") {
      if (state.walletShowRestore) {
        target
          .closest("[data-action='restore-wallet']")
          ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        const btn = document.querySelector(
          "[data-action='restore-wallet']",
        ) as HTMLElement | null;
        btn?.click();
      } else {
        const btn = document.querySelector(
          "[data-action='create-wallet']",
        ) as HTMLElement | null;
        btn?.click();
      }
    } else if (state.walletStatus === "locked") {
      const btn = document.querySelector(
        "[data-action='unlock-wallet']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "trade-size-sats") {
    event.preventDefault();
    commitTradeSizeSatsDraft();
    render();
    return;
  }

  if (target.id === "trade-size-contracts") {
    event.preventDefault();
    commitTradeContractsDraft(getSelectedMarket());
    render();
  }
});

app.addEventListener("focusout", (event) => {
  const target = event.target as HTMLInputElement;

  if (target.id === "trade-size-sats") {
    commitTradeSizeSatsDraft();
    render();
    return;
  }

  if (target.id === "trade-size-contracts") {
    commitTradeContractsDraft(getSelectedMarket());
    render();
    return;
  }

  if (target.id === "limit-price") {
    commitLimitPriceDraft();
    const nextFocus = (event as FocusEvent).relatedTarget as HTMLElement | null;
    if (nextFocus?.closest("[data-action='step-limit-price']")) {
      return;
    }
    render();
    return;
  }

  if (
    target.id === "create-question" ||
    target.id === "create-description" ||
    target.id === "create-category" ||
    target.id === "create-settlement" ||
    target.id === "create-resolution-source" ||
    target.id === "create-yes-sats"
  ) {
    if (target.id === "create-yes-sats") {
      const parsed = Math.round(Number(target.value) || 50);
      state.createStartingYesSats = Math.max(
        1,
        Math.min(SATS_PER_FULL_CONTRACT - 1, parsed),
      );
    }
    if (state.view === "create") render();
  }
});
