import { create } from "zustand";
import type {
  ActionTab,
  AppNetwork,
  BaseCurrency,
  BoltzChainSwapCreated,
  BoltzChainSwapPairInfo,
  BoltzLightningReceiveCreated,
  BoltzSubmarineSwapCreated,
  ChartTimescale,
  LimitSellWarning,
  LiquidSendResult,
  LmsrPoolInfo,
  MarketCategory,
  NavCategory,
  NostrBackupStatus,
  NostrProfile,
  OrderType,
  OwnOrderSummary,
  PriceHistoryEntry,
  QuoteMarketTradeResult,
  RelayEntry,
  Side,
  SizeMode,
  TradeIntent,
  TradeQuoteSnapshot,
  ViewMode,
  WalletData,
} from "../types";

function defaultSettlementInput(): string {
  const inThirtyDays = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000);
  const year = inThirtyDays.getFullYear();
  const month = String(inThirtyDays.getMonth() + 1).padStart(2, "0");
  const day = String(inThirtyDays.getDate()).padStart(2, "0");
  const hours = String(inThirtyDays.getHours()).padStart(2, "0");
  const minutes = String(inThirtyDays.getMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}T${hours}:${minutes}`;
}

// ── Navigation slice ────────────────────────────────────────────────
export interface NavigationSlice {
  view: ViewMode;
  previousView: ViewMode | null;
  activeCategory: NavCategory;
  search: string;
  trendingIndex: number;
  userMenuOpen: boolean;
  searchOpen: boolean;
  helpOpen: boolean;
  settingsOpen: boolean;
  settingsSection: Record<string, boolean>;
  logoutOpen: boolean;
  walletSessionPassword: string;
  logoutBackedUp: boolean;
  logoutBackupError: string;
  logoutPasswordInput: string;
  logoutBackupDownloaded: boolean;
  profileOpen: boolean;
  closeConfirmOpen: boolean;
  onboardingSuccess: boolean;
  showMiniWallet: boolean;
  showLbtcLabel: boolean;
  marketMakerMode: boolean;
  baseCurrency: BaseCurrency;
}

// ── Trading slice ───────────────────────────────────────────────────
export interface TradingSlice {
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
  tradeQuoteLoading: boolean;
  tradeExecuteLoading: boolean;
  cancellingOrderId: string | null;
  tradeQuoteSnapshot: TradeQuoteSnapshot | null;
  tradeError: string | null;
  tradeSizeSats: number;
  tradeSizeSatsDraft: string;
  tradeContracts: number;
  tradeContractsDraft: string;
  limitPrice: number;
  limitPriceDraft: string;
  buyLimitComposerOpen: boolean;
  limitSellOverrideAccepted: boolean;
  limitSellWarning: LimitSellWarning | null;
  limitSellWarningInfo: string;
  limitSellGuardVersion: number;
  limitSellGuardChecking: boolean;
  tradeQuoteModalOpen: boolean;
  tradeQuoteExecuting: boolean;
  tradeQuoteData: QuoteMarketTradeResult | null;
  tradeQuoteError: string;
  tradeQuoteNowUnix: number;
  pairsInput: number;
  tokensInput: number;
  ownOrders: OwnOrderSummary[];
  myPools: LmsrPoolInfo[];
  poolCreateOpen: boolean;
  priceHistory: Map<string, PriceHistoryEntry[]>;
}

// ── Wallet UI slice ─────────────────────────────────────────────────
export interface WalletUiSlice {
  walletOpen: boolean;
  walletTokenPage: number;
  walletTxPage: number;
  walletStatus: "not_created" | "locked" | "unlocked";
  walletNetwork: AppNetwork;
  walletData: WalletData | null;
  walletPolicyAssetId: string;
  walletPassword: string;
  walletPasswordConfirm: string;
  walletMnemonic: string;
  walletRestoreMnemonic: string;
  walletShowRestore: boolean;
  walletShowCreate: boolean;
  walletUtxosExpanded: boolean;
  walletError: string;
  walletLoading: boolean;
  walletUnit: "sats" | "btc";
  walletBalanceHidden: boolean;
  walletDeletePrompt: boolean;
  walletDeleteConfirm: string;
  nostrBackupNsecInput: string;
  nostrBackupNsecPrompt: boolean;
  nostrSetupExpanded: boolean;
}

// ── Wallet modals slice ─────────────────────────────────────────────
export interface WalletModalsSlice {
  walletModal: "none" | "receive" | "send";
  walletModalTab: "lightning" | "liquid" | "bitcoin";
  modalQr: string;
  receiveAmount: string;
  receiveCreating: boolean;
  receiveError: string;
  receiveLightningSwap: BoltzLightningReceiveCreated | null;
  receiveLiquidAddress: string;
  receiveLiquidLoading: boolean;
  receiveBitcoinSwap: BoltzChainSwapCreated | null;
  receiveBtcPairInfo: BoltzChainSwapPairInfo | null;
  sendInvoice: string;
  sendLiquidAddress: string;
  sendLiquidAmount: string;
  sendBtcAmount: string;
  sendCreating: boolean;
  sendError: string;
  sentLightningSwap: BoltzSubmarineSwapCreated | null;
  sentLiquidResult: LiquidSendResult | null;
  sentBitcoinSwap: BoltzChainSwapCreated | null;
  sendBtcPairInfo: BoltzChainSwapPairInfo | null;
}

// ── Market creation slice ───────────────────────────────────────────
export interface MarketCreationSlice {
  createQuestion: string;
  createDescription: string;
  createCategory: MarketCategory;
  createCategoryOpen: boolean;
  createResolutionSource: string;
  createCptSats: number;
  createSettlementInput: string;
  createSettlementPickerOpen: boolean;
  createSettlementPickerDropdown: string;
  createSettlementViewYear: number;
  createSettlementViewMonth: number;
  marketCreating: boolean;
  marketsLoading: boolean;
}

// ── Onboarding slice ────────────────────────────────────────────────
export interface OnboardingSlice {
  setupModalOpen: boolean;
  setupRequires: "wallet" | "identity+wallet" | null;
  onboardingStep: "nostr" | "wallet" | null;
  onboardingNostrMode: "generate" | "import" | "restore";
  onboardingNostrNsec: string;
  onboardingNostrGeneratedNsec: string;
  onboardingNsecRevealed: boolean;
  onboardingNsecAcknowledged: boolean;
  onboardingNostrDone: boolean;
  onboardingPendingPubkey: string;
  onboardingPendingNpub: string;
  onboardingProfileStep: boolean;
  onboardingNostrDisplayName: string;
  onboardingProfilePhotoDataUrl: string;
  onboardingKeysOpen: boolean;
  onboardingRestoreFileContent: string;
  onboardingRestorePassword: string;
  onboardingRestoreMnemonic: string;
  onboardingRestoreNsec: string;
  onboardingBackupDownloaded: boolean;
  onboardingBackupFileContent: string;
  onboardingWalletMode: "create" | "restore" | "nostr-restore";
  onboardingWalletPasswordStep: boolean;
  onboardingWalletPassword: string;
  onboardingWalletPasswordConfirm: string;
  onboardingWalletMnemonic: string;
  onboardingMnemonicVerifyStep: boolean;
  onboardingMnemonicVerifyIndices: number[];
  onboardingMnemonicVerifyInputs: string[];
  onboardingError: string;
  onboardingLoading: boolean;
  onboardingBackupFound: boolean;
  onboardingBackupScanning: boolean;
  onboardingPasswordRevealed: boolean;
  onboardingWalletOnly: boolean;
  onboardingWalletName: string;
  onboardingSelectedWalletDTag: string;
  walletRecovering: boolean;
  walletRecoveryPhase: number;
  walletRecoverySummary: {
    markets: number;
    positions: number;
    orders: number;
    pools: number;
    estimatedSats: number;
  } | null;
  nostrPubkey: string | null;
  nostrNpub: string | null;
  nostrNsecRevealed: string | null;
  nostrImportNsec: string;
  nostrImporting: boolean;
  nostrReplacePrompt: boolean;
  nostrReplacePanel: boolean;
  nostrReplaceConfirm: string;
  nostrBackupStatus: NostrBackupStatus | null;
  nostrBackupLoading: boolean;
  nostrBackupPassword: string;
  nostrBackupPrompt: boolean;
  relays: RelayEntry[];
  relayInput: string;
  relayLoading: boolean;
  nostrProfile: NostrProfile | null;
  profilePicError: boolean;
  nostrEventModal: boolean;
  nostrEventJson: string | null;
  nostrEventNevent: string | null;
  attestationLoading: boolean;
  lastAttestationSig: string | null;
  lastAttestationOutcome: boolean | null;
  lastAttestationMarketId: string | null;
  resolutionExecuting: boolean;
  devResetPrompt: boolean;
  devResetConfirm: string;
}

// ── Chart UI slice ──────────────────────────────────────────────────
export interface ChartUiSlice {
  chartHoverMarketId: string | null;
  chartHoverX: number | null;
  chartTimescale: ChartTimescale;
  chartAspectHome: number;
  chartAspectDetail: number;
}

// ── Persistence ─────────────────────────────────────────────────────
export interface PersistenceSlice {
  recentTxLabels: Map<string, { label: string; marketId: string }>;
}

// ── Combined store type ─────────────────────────────────────────────
export type StoreState = NavigationSlice &
  TradingSlice &
  WalletUiSlice &
  WalletModalsSlice &
  MarketCreationSlice &
  OnboardingSlice &
  ChartUiSlice &
  PersistenceSlice;

const inThirtyDays = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000);

export const useStore = create<StoreState>()(() => ({
  // ── Navigation ──────────────────────────────────────────────────
  view: "home",
  previousView: null,
  activeCategory: "Trending",
  search: "",
  trendingIndex: 0,
  userMenuOpen: false,
  searchOpen: false,
  helpOpen: false,
  settingsOpen: false,
  settingsSection: { nostr: true, relays: false, wallet: false, dev: false },
  logoutOpen: false,
  walletSessionPassword: "",
  logoutBackedUp: false,
  logoutBackupError: "",
  logoutPasswordInput: "",
  logoutBackupDownloaded: false,
  profileOpen: false,
  closeConfirmOpen: false,
  onboardingSuccess: false,
  showMiniWallet: true,
  showLbtcLabel: false,
  marketMakerMode: false,
  baseCurrency: "BTC" as BaseCurrency,

  // ── Trading ─────────────────────────────────────────────────────
  selectedMarketId: "mkt-3",
  selectedSide: "yes",
  orderType: "market",
  actionTab: "trade",
  tradeIntent: "open",
  sizeMode: "sats",
  showAdvancedDetails: false,
  showAdvancedActions: false,
  showOrderbook: false,
  showFeeDetails: false,
  tradeQuoteLoading: false,
  tradeExecuteLoading: false,
  cancellingOrderId: null,
  tradeQuoteSnapshot: null,
  tradeError: null,
  tradeSizeSats: 10000,
  tradeSizeSatsDraft: "10,000",
  tradeContracts: 10,
  tradeContractsDraft: "10",
  limitPrice: 0.5,
  limitPriceDraft: "50",
  buyLimitComposerOpen: false,
  limitSellOverrideAccepted: false,
  limitSellWarning: null,
  limitSellWarningInfo: "",
  limitSellGuardVersion: 0,
  limitSellGuardChecking: false,
  tradeQuoteModalOpen: false,
  tradeQuoteExecuting: false,
  tradeQuoteData: null,
  tradeQuoteError: "",
  tradeQuoteNowUnix: Math.floor(Date.now() / 1000),
  pairsInput: 10,
  tokensInput: 25,
  ownOrders: [],
  myPools: [],
  poolCreateOpen: false,
  priceHistory: new Map(),

  // ── Wallet UI ───────────────────────────────────────────────────
  walletOpen: false,
  walletTokenPage: 0,
  walletTxPage: 0,
  walletStatus: "not_created",
  walletNetwork: "testnet",
  walletData: null,
  walletPolicyAssetId: "",
  walletPassword: "",
  walletPasswordConfirm: "",
  walletMnemonic: "",
  walletRestoreMnemonic: "",
  walletShowRestore: false,
  walletShowCreate: false,
  walletUtxosExpanded: false,
  walletError: "",
  walletLoading: false,
  walletUnit: "sats",
  walletBalanceHidden: false,
  walletDeletePrompt: false,
  walletDeleteConfirm: "",
  nostrBackupNsecInput: "",
  nostrBackupNsecPrompt: false,
  nostrSetupExpanded: false,

  // ── Wallet modals ───────────────────────────────────────────────
  walletModal: "none",
  walletModalTab: "lightning",
  modalQr: "",
  receiveAmount: "",
  receiveCreating: false,
  receiveError: "",
  receiveLightningSwap: null,
  receiveLiquidAddress: "",
  receiveLiquidLoading: false,
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

  // ── Market creation ─────────────────────────────────────────────
  createQuestion: "",
  createDescription: "",
  createCategory: "Bitcoin",
  createCategoryOpen: false,
  createResolutionSource: "",
  createCptSats: 5000,
  createSettlementInput: defaultSettlementInput(),
  createSettlementPickerOpen: false,
  createSettlementPickerDropdown: "",
  createSettlementViewYear: inThirtyDays.getFullYear(),
  createSettlementViewMonth: inThirtyDays.getMonth(),
  marketCreating: false,
  marketsLoading: true,

  // ── Onboarding ──────────────────────────────────────────────────
  setupModalOpen: false,
  setupRequires: null,
  onboardingStep: null,
  onboardingNostrMode: "generate",
  onboardingNostrNsec: "",
  onboardingNostrGeneratedNsec: "",
  onboardingNsecRevealed: false,
  onboardingNsecAcknowledged: false,
  onboardingNostrDone: false,
  onboardingPendingPubkey: "",
  onboardingPendingNpub: "",
  onboardingProfileStep: false,
  onboardingNostrDisplayName: "",
  onboardingProfilePhotoDataUrl: "",
  onboardingKeysOpen: false,
  onboardingRestoreFileContent: "",
  onboardingRestorePassword: "",
  onboardingRestoreMnemonic: "",
  onboardingRestoreNsec: "",
  onboardingBackupDownloaded: false,
  onboardingBackupFileContent: "",
  onboardingWalletMode: "create",
  onboardingWalletPasswordStep: false,
  onboardingWalletPassword: "",
  onboardingWalletPasswordConfirm: "",
  onboardingWalletMnemonic: "",
  onboardingMnemonicVerifyStep: false,
  onboardingMnemonicVerifyIndices: [],
  onboardingMnemonicVerifyInputs: [],
  onboardingError: "",
  onboardingLoading: false,
  onboardingBackupFound: false,
  onboardingBackupScanning: false,
  onboardingPasswordRevealed: false,
  onboardingWalletOnly: false,
  onboardingWalletName: "My Wallet",
  onboardingSelectedWalletDTag: "",
  walletRecovering: false,
  walletRecoveryPhase: 0,
  walletRecoverySummary: null,
  nostrPubkey: null,
  nostrNpub: null,
  nostrNsecRevealed: null,
  nostrImportNsec: "",
  nostrImporting: false,
  nostrReplacePrompt: false,
  nostrReplacePanel: false,
  nostrReplaceConfirm: "",
  nostrBackupStatus: null,
  nostrBackupLoading: false,
  nostrBackupPassword: "",
  nostrBackupPrompt: false,
  relays: [],
  relayInput: "",
  relayLoading: false,
  nostrProfile: null,
  profilePicError: false,
  nostrEventModal: false,
  nostrEventJson: null,
  nostrEventNevent: null,
  attestationLoading: false,
  lastAttestationSig: null,
  lastAttestationOutcome: null,
  lastAttestationMarketId: null,
  resolutionExecuting: false,
  devResetPrompt: false,
  devResetConfirm: "",

  // ── Chart UI ────────────────────────────────────────────────────
  chartHoverMarketId: null,
  chartHoverX: null,
  chartTimescale: "25B",
  chartAspectHome: 2.8,
  chartAspectDetail: 3.35,

  // ── Persistence ─────────────────────────────────────────────────
  recentTxLabels: new Map(),
}));
