import type {
  ActionTab,
  BaseCurrency,
  BoltzChainSwapCreated,
  BoltzChainSwapPairInfo,
  BoltzLightningReceiveCreated,
  BoltzSubmarineSwapCreated,
  Market,
  MarketCategory,
  NavCategory,
  NostrBackupStatus,
  NostrProfile,
  OrderType,
  RelayEntry,
  Side,
  SizeMode,
  TradeIntent,
  ViewMode,
  WalletData,
} from "./types.ts";

export function createWalletData(): WalletData {
  return {
    balance: {},
    transactions: [],
    utxos: [],
    swaps: [],
    backupWords: [],
    backedUp: false,
    showBackup: false,
    backupPassword: "",
  };
}

export const app = document.querySelector<HTMLDivElement>(
  "#app",
) as HTMLDivElement;
export const DEV_MODE = import.meta.env.DEV;

export const EXECUTION_FEE_RATE = 0.01;
export const WIN_FEE_RATE = 0.02;
export const SATS_PER_FULL_CONTRACT = 100;

export const categories: NavCategory[] = [
  "Trending",
  "Politics",
  "Sports",
  "Culture",
  "Bitcoin",
  "Weather",
  "Macro",
  "My Markets",
];

export const baseCurrencyOptions: BaseCurrency[] = [
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

export const fxRates: Record<BaseCurrency, number> = {
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

export let markets: Market[] = [];
export function setMarkets(m: Market[]): void {
  markets = m;
}

export function defaultSettlementInput(): string {
  const inThirtyDays = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000);
  const year = inThirtyDays.getFullYear();
  const month = String(inThirtyDays.getMonth() + 1).padStart(2, "0");
  const day = String(inThirtyDays.getDate()).padStart(2, "0");
  const hours = String(inThirtyDays.getHours()).padStart(2, "0");
  const minutes = String(inThirtyDays.getMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}T${hours}:${minutes}`;
}

export const state: {
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
  createCategoryOpen: boolean;
  createResolutionSource: string;
  createSettlementInput: string;
  createSettlementPickerOpen: boolean;
  createSettlementPickerDropdown: string;
  createSettlementViewYear: number;
  createSettlementViewMonth: number;
  walletStatus: "not_created" | "locked" | "unlocked";
  walletNetwork: "mainnet" | "testnet" | "regtest";
  walletData: WalletData | null;
  walletPolicyAssetId: string;
  walletMnemonic: string;
  walletUtxosExpanded: boolean;
  walletError: string;
  walletLoading: boolean;
  walletPassword: string;
  walletRestoreMnemonic: string;
  walletShowRestore: boolean;
  walletModal: "none" | "receive" | "send";
  walletModalTab: "lightning" | "liquid" | "bitcoin";
  modalQr: string;
  receiveAmount: string;
  receiveCreating: boolean;
  receiveError: string;
  receiveLightningSwap: BoltzLightningReceiveCreated | null;
  receiveLiquidAddress: string;
  receiveLiquidAddressIndex: number;
  receiveBitcoinSwap: BoltzChainSwapCreated | null;
  receiveBtcPairInfo: BoltzChainSwapPairInfo | null;
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
  showMiniWallet: boolean;
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
  nostrBackupStatus: NostrBackupStatus | null;
  nostrBackupLoading: boolean;
  nostrBackupPassword: string;
  nostrBackupPrompt: boolean;
  relays: RelayEntry[];
  relayInput: string;
  relayLoading: boolean;
  nostrProfile: NostrProfile | null;
  profilePicError: boolean;
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
  nostrEventModal: boolean;
  nostrEventJson: string | null;
  nostrEventNevent: string | null;
  chartHoverMarketId: string | null;
  chartHoverX: number | null;
  chartTimescale: "1H" | "3H" | "6H" | "12H" | "1D";
  chartAspectHome: number;
  chartAspectDetail: number;
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
  createCategoryOpen: false,
  createResolutionSource: "",
  createSettlementInput: defaultSettlementInput(),
  createSettlementPickerOpen: false,
  createSettlementPickerDropdown: "",
  createSettlementViewYear: new Date(
    Date.now() + 30 * 24 * 60 * 60 * 1000,
  ).getFullYear(),
  createSettlementViewMonth: new Date(
    Date.now() + 30 * 24 * 60 * 60 * 1000,
  ).getMonth(),
  walletStatus: "not_created",
  walletNetwork: "testnet",
  walletData: null,
  walletPolicyAssetId: "",
  walletMnemonic: "",
  walletUtxosExpanded: false,
  walletError: "",
  walletLoading: false,
  walletPassword: "",
  walletRestoreMnemonic: "",
  walletShowRestore: false,
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
  showMiniWallet: true,
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
  nostrEventModal: false,
  nostrEventJson: null,
  nostrEventNevent: null,
  chartHoverMarketId: null,
  chartHoverX: null,
  chartTimescale: "3H",
  chartAspectHome: 2.8,
  chartAspectDetail: 3.35,
};
