import "./style.css";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";

const app = document.querySelector<HTMLDivElement>("#app")!;

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
  id: string; flow: string; invoiceAmountSat: number;
  expectedOnchainAmountSat: number; invoice: string;
  invoiceExpiresAt: string; invoiceExpirySeconds: number;
};

type BoltzSubmarineSwapCreated = {
  id: string; flow: string; invoiceAmountSat: number;
  expectedAmountSat: number; lockupAddress: string;
  bip21: string; invoiceExpiresAt: string; invoiceExpirySeconds: number;
};

type BoltzChainSwapCreated = {
  id: string; flow: string; amountSat: number;
  expectedAmountSat: number; lockupAddress: string;
  claimLockupAddress: string; timeoutBlockHeight: number;
  bip21: string | null;
};

type BoltzChainSwapPairInfo = {
  pairHash: string; minAmountSat: number; maxAmountSat: number;
  feePercentage: number; minerFeeLockupSat: number;
  minerFeeClaimSat: number; minerFeeServerSat: number;
  fixedMinerFeeTotalSat: number;
};

type BoltzChainSwapPairsInfo = {
  bitcoinToLiquid: BoltzChainSwapPairInfo;
  liquidToBitcoin: BoltzChainSwapPairInfo;
};

type PaymentSwap = {
  id: string; flow: string; network: string; status: string;
  invoiceAmountSat: number; expectedAmountSat: number | null;
  lockupAddress: string | null; invoice: string | null;
  invoiceExpiresAt: string | null; lockupTxid: string | null;
  createdAt: string; updatedAt: string;
};

type Market = {
  id: string;
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
const LARGE_ORDER_SATS_GUARDRAIL = 10000;

const categories: NavCategory[] = [
  "Trending",
  "Politics",
  "Sports",
  "Culture",
  "Bitcoin",
  "Weather",
  "Macro",
];

const markets: Market[] = [
  {
    id: "mkt-1",
    question: "Will BTC close above $120,000 by Dec 31, 2026?",
    category: "Bitcoin",
    description:
      "Resolved using a median close basket from major spot exchanges.",
    resolutionSource: "Exchange close basket",
    isLive: false,
    state: 1,
    marketId: "9db7d4d4...a1ce",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3650000,
    currentHeight: 3634120,
    cptSats: 5000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:5a1f...3b21",
    noAssetId: "NO:7cb4...2f88",
    yesReissuanceToken: "YRT:1b11...e2ac",
    noReissuanceToken: "NRT:2f88...b190",
    collateralUtxos: [{ txid: "5fd1...e92b", vout: 0, amountSats: 293000000 }],
    yesPrice: 0.57,
    change24h: 4.2,
    volumeBtc: 184.3,
    liquidityBtc: 24.2,
  },
  {
    id: "mkt-2",
    question: "Will Team Orbit win the 2026 basketball finals?",
    category: "Sports",
    description:
      "Resolves when the official league result publishes the champion.",
    resolutionSource: "Official league result",
    isLive: true,
    state: 1,
    marketId: "6d41c93f...8b4a",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3621000,
    currentHeight: 3620890,
    cptSats: 5000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:98a3...4420",
    noAssetId: "NO:9f3d...1ac3",
    yesReissuanceToken: "YRT:be31...770a",
    noReissuanceToken: "NRT:d993...0c14",
    collateralUtxos: [{ txid: "4ce1...17a9", vout: 1, amountSats: 107000000 }],
    yesPrice: 0.33,
    change24h: -3.5,
    volumeBtc: 54.4,
    liquidityBtc: 7.8,
  },
  {
    id: "mkt-3",
    question: "Will candidate Redwood win their party nomination in 2028?",
    category: "Politics",
    description:
      "Settlement follows official nominee certification from party convention.",
    resolutionSource: "Party convention certification",
    isLive: false,
    state: 1,
    marketId: "5a90ccde...cd44",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 4015000,
    currentHeight: 3634120,
    cptSats: 5000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:7ab1...2310",
    noAssetId: "NO:7ab1...2311",
    yesReissuanceToken: "YRT:a9de...2930",
    noReissuanceToken: "NRT:d1e9...fa70",
    collateralUtxos: [{ txid: "f111...78ce", vout: 0, amountSats: 225400000 }],
    yesPrice: 0.41,
    change24h: 6.8,
    volumeBtc: 112.7,
    liquidityBtc: 15.4,
  },
  {
    id: "mkt-4",
    question: "Will NYC record more than 10 inches of snow in Feb 2026?",
    category: "Weather",
    description:
      "Based on NOAA monthly snowfall at the Central Park weather station.",
    resolutionSource: "NOAA Central Park station",
    isLive: true,
    state: 3,
    marketId: "44b1e1aa...39ff",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3610000,
    currentHeight: 3634120,
    cptSats: 3000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:11c0...b8e1",
    noAssetId: "NO:11c0...b8e2",
    yesReissuanceToken: "YRT:4900...3ef1",
    noReissuanceToken: "NRT:fb2e...9a14",
    collateralUtxos: [{ txid: "2af8...71de", vout: 0, amountSats: 62000000 }],
    resolveTx: {
      txid: "aa83...d1ef",
      outcome: "no",
      sigVerified: true,
      height: 3610024,
      signatureHash: "SHA256(44b1e1aa...39ff||00)",
    },
    yesPrice: 0.29,
    change24h: -0.9,
    volumeBtc: 28.5,
    liquidityBtc: 4.1,
  },
  {
    id: "mkt-5",
    question: "Will the Fed cut rates at the next FOMC decision?",
    category: "Macro",
    description: "Binary outcome based on official target range cut vs no cut.",
    resolutionSource: "Federal Reserve statement",
    isLive: false,
    state: 1,
    marketId: "19bc40da...ccb1",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3620000,
    currentHeight: 3634120,
    cptSats: 4000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:b441...7d11",
    noAssetId: "NO:b441...7d12",
    yesReissuanceToken: "YRT:c510...90ea",
    noReissuanceToken: "NRT:2a30...ca7d",
    collateralUtxos: [{ txid: "5511...0abc", vout: 0, amountSats: 121000000 }],
    yesPrice: 0.39,
    change24h: 2.1,
    volumeBtc: 91.2,
    liquidityBtc: 11.5,
  },
  {
    id: "mkt-6",
    question:
      "Will a major AI model release rank #1 on app stores within 30 days?",
    category: "Culture",
    description:
      "Outcome measured by top free overall ranking in US app stores.",
    resolutionSource: "Public app store rankings",
    isLive: false,
    state: 1,
    marketId: "43bc8dd2...ff10",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3640000,
    currentHeight: 3634120,
    cptSats: 3500,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:a341...12d4",
    noAssetId: "NO:a341...12d5",
    yesReissuanceToken: "YRT:5a32...3bc9",
    noReissuanceToken: "NRT:44bd...1290",
    collateralUtxos: [
      { txid: "f001...77ac", vout: 0, amountSats: 43000000 },
      { txid: "f001...77ac", vout: 1, amountSats: 31000000 },
    ],
    yesPrice: 0.35,
    change24h: 5.4,
    volumeBtc: 21.7,
    liquidityBtc: 3.3,
  },
  {
    id: "mkt-7",
    question: "Will BTC trade above $150,000 before Jan 1, 2027?",
    category: "Bitcoin",
    description:
      "Resolves YES if any major USD spot venue prints 150,000+ before deadline.",
    resolutionSource: "Multi-exchange high print basket",
    isLive: false,
    state: 1,
    marketId: "73ab1f09...c33a",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3702200,
    currentHeight: 3634120,
    cptSats: 5000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:ec14...5411",
    noAssetId: "NO:ec14...5412",
    yesReissuanceToken: "YRT:ec14...5413",
    noReissuanceToken: "NRT:ec14...5414",
    collateralUtxos: [{ txid: "cf90...11ab", vout: 0, amountSats: 188000000 }],
    yesPrice: 0.44,
    change24h: 3.1,
    volumeBtc: 97.9,
    liquidityBtc: 12.4,
  },
  {
    id: "mkt-8",
    question: "Will ETH/BTC close above 0.070 by June 30, 2026?",
    category: "Bitcoin",
    description:
      "Resolves from daily close ratio composite across top centralized venues.",
    resolutionSource: "ETH/BTC close composite",
    isLive: false,
    state: 1,
    marketId: "8a20de11...998f",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3665200,
    currentHeight: 3634120,
    cptSats: 4000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:7134...8f21",
    noAssetId: "NO:7134...8f22",
    yesReissuanceToken: "YRT:7134...8f23",
    noReissuanceToken: "NRT:7134...8f24",
    collateralUtxos: [{ txid: "81ae...2bc1", vout: 0, amountSats: 96200000 }],
    yesPrice: 0.28,
    change24h: -2.4,
    volumeBtc: 55.2,
    liquidityBtc: 8.6,
  },
  {
    id: "mkt-9",
    question: "Will candidate Redwood choose Vega as running mate?",
    category: "Politics",
    description:
      "Resolved on official campaign filing naming the vice-presidential nominee.",
    resolutionSource: "FEC filing + campaign announcement",
    isLive: false,
    state: 1,
    marketId: "db4122f0...77bc",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3942000,
    currentHeight: 3634120,
    cptSats: 4500,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:41d8...9011",
    noAssetId: "NO:41d8...9012",
    yesReissuanceToken: "YRT:41d8...9013",
    noReissuanceToken: "NRT:41d8...9014",
    collateralUtxos: [{ txid: "9dc2...0a44", vout: 1, amountSats: 116000000 }],
    yesPrice: 0.36,
    change24h: 1.9,
    volumeBtc: 63.4,
    liquidityBtc: 8.2,
  },
  {
    id: "mkt-10",
    question: "Will the governing coalition lose its majority this year?",
    category: "Politics",
    description:
      "YES if official parliamentary seat count drops below majority threshold.",
    resolutionSource: "Official parliamentary records",
    isLive: false,
    state: 1,
    marketId: "67f0aa31...80d2",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3814400,
    currentHeight: 3634120,
    cptSats: 4000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:b1c2...33e1",
    noAssetId: "NO:b1c2...33e2",
    yesReissuanceToken: "YRT:b1c2...33e3",
    noReissuanceToken: "NRT:b1c2...33e4",
    collateralUtxos: [{ txid: "0911...a0f4", vout: 0, amountSats: 138500000 }],
    yesPrice: 0.52,
    change24h: -1.1,
    volumeBtc: 78.6,
    liquidityBtc: 9.7,
  },
  {
    id: "mkt-11",
    question: "Will Team Orbit win game 4 tonight?",
    category: "Sports",
    description:
      "Live event contract resolves from official league game book final score.",
    resolutionSource: "Official game book",
    isLive: true,
    state: 1,
    marketId: "2a4ce9d3...33a2",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3634300,
    currentHeight: 3634120,
    cptSats: 3000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:24d4...6611",
    noAssetId: "NO:24d4...6612",
    yesReissuanceToken: "YRT:24d4...6613",
    noReissuanceToken: "NRT:24d4...6614",
    collateralUtxos: [{ txid: "22aa...9914", vout: 0, amountSats: 54400000 }],
    yesPrice: 0.61,
    change24h: 7.2,
    volumeBtc: 39.8,
    liquidityBtc: 5.2,
  },
  {
    id: "mkt-12",
    question: "Will Harbor City FC finish top-4 this season?",
    category: "Sports",
    description:
      "Season standings contract resolved when league table is finalized.",
    resolutionSource: "Official league standings",
    isLive: false,
    state: 1,
    marketId: "ccf120f8...04a3",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3726000,
    currentHeight: 3634120,
    cptSats: 3500,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:50bd...111a",
    noAssetId: "NO:50bd...111b",
    yesReissuanceToken: "YRT:50bd...111c",
    noReissuanceToken: "NRT:50bd...111d",
    collateralUtxos: [{ txid: "7b2f...34de", vout: 1, amountSats: 84700000 }],
    yesPrice: 0.47,
    change24h: 2.8,
    volumeBtc: 45.6,
    liquidityBtc: 6.9,
  },
  {
    id: "mkt-13",
    question: "Will a sci-fi film win Best Picture at the 2027 Oscars?",
    category: "Culture",
    description:
      "Resolves using Academy official Best Picture winner publication.",
    resolutionSource: "Academy awards official results",
    isLive: false,
    state: 1,
    marketId: "11fe8cc1...44bf",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3760500,
    currentHeight: 3634120,
    cptSats: 3000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:9f44...2be1",
    noAssetId: "NO:9f44...2be2",
    yesReissuanceToken: "YRT:9f44...2be3",
    noReissuanceToken: "NRT:9f44...2be4",
    collateralUtxos: [{ txid: "55f2...22a7", vout: 0, amountSats: 61000000 }],
    yesPrice: 0.24,
    change24h: 1.3,
    volumeBtc: 33.7,
    liquidityBtc: 4.4,
  },
  {
    id: "mkt-14",
    question: "Will a new album from Nova X chart #1 in the US this year?",
    category: "Culture",
    description:
      "YES if Billboard 200 reports #1 for a qualifying Nova X release.",
    resolutionSource: "Billboard 200 chart",
    isLive: false,
    state: 1,
    marketId: "80bd9a0e...2271",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3819900,
    currentHeight: 3634120,
    cptSats: 2500,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:0de1...441a",
    noAssetId: "NO:0de1...441b",
    yesReissuanceToken: "YRT:0de1...441c",
    noReissuanceToken: "NRT:0de1...441d",
    collateralUtxos: [{ txid: "888a...3c30", vout: 0, amountSats: 47200000 }],
    yesPrice: 0.58,
    change24h: -0.8,
    volumeBtc: 27.1,
    liquidityBtc: 3.8,
  },
  {
    id: "mkt-15",
    question: "Will Miami record a daily high above 100F in July 2026?",
    category: "Weather",
    description:
      "Resolves YES if NOAA station data records any 100F+ maximum in July.",
    resolutionSource: "NOAA Miami station daily highs",
    isLive: false,
    state: 1,
    marketId: "4137aa20...dd42",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3698400,
    currentHeight: 3634120,
    cptSats: 3000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:99ab...1101",
    noAssetId: "NO:99ab...1102",
    yesReissuanceToken: "YRT:99ab...1103",
    noReissuanceToken: "NRT:99ab...1104",
    collateralUtxos: [{ txid: "776f...6df0", vout: 0, amountSats: 53200000 }],
    yesPrice: 0.49,
    change24h: 0.6,
    volumeBtc: 24.3,
    liquidityBtc: 3.5,
  },
  {
    id: "mkt-16",
    question: "Will Hurricane Atlas make US landfall as Cat 3+ this season?",
    category: "Weather",
    description:
      "Resolved from NHC final advisories and post-storm official report.",
    resolutionSource: "National Hurricane Center",
    isLive: false,
    state: 0,
    marketId: "17ca621d...930e",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3743300,
    currentHeight: 3634120,
    cptSats: 4500,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:76a1...9ad1",
    noAssetId: "NO:76a1...9ad2",
    yesReissuanceToken: "YRT:76a1...9ad3",
    noReissuanceToken: "NRT:76a1...9ad4",
    collateralUtxos: [{ txid: "00f1...be09", vout: 0, amountSats: 0 }],
    yesPrice: 0.22,
    change24h: 0.0,
    volumeBtc: 0.0,
    liquidityBtc: 0.0,
  },
  {
    id: "mkt-17",
    question: "Will the Fed funds target be below 4.00% by year-end 2026?",
    category: "Macro",
    description:
      "Resolved from official FOMC target range upper bound at year-end meeting.",
    resolutionSource: "Federal Reserve target range",
    isLive: false,
    state: 1,
    marketId: "64de90ac...700c",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3814200,
    currentHeight: 3634120,
    cptSats: 4500,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:d1a9...fa21",
    noAssetId: "NO:d1a9...fa22",
    yesReissuanceToken: "YRT:d1a9...fa23",
    noReissuanceToken: "NRT:d1a9...fa24",
    collateralUtxos: [{ txid: "0ac4...bed3", vout: 0, amountSats: 144700000 }],
    yesPrice: 0.46,
    change24h: 1.4,
    volumeBtc: 88.5,
    liquidityBtc: 10.7,
  },
  {
    id: "mkt-18",
    question: "Will US core CPI print below 2.8% by Sep 2026?",
    category: "Macro",
    description:
      "YES if BLS release shows year-over-year core CPI below 2.8 by deadline.",
    resolutionSource: "BLS CPI release",
    isLive: false,
    state: 1,
    marketId: "91fc22ab...6a11",
    oraclePubkey: "8a2e4d9f...f102",
    expiryHeight: 3758800,
    currentHeight: 3634120,
    cptSats: 4000,
    collateralAssetId: "L-BTC:6f0279e9...",
    yesAssetId: "YES:c2d0...66f1",
    noAssetId: "NO:c2d0...66f2",
    yesReissuanceToken: "YRT:c2d0...66f3",
    noReissuanceToken: "NRT:c2d0...66f4",
    collateralUtxos: [{ txid: "61be...4e72", vout: 0, amountSats: 116400000 }],
    yesPrice: 0.34,
    change24h: -1.7,
    volumeBtc: 69.1,
    liquidityBtc: 8.9,
  },
];

const trendingIds = [
  "mkt-3",
  "mkt-1",
  "mkt-2",
  "mkt-11",
  "mkt-10",
  "mkt-17",
  "mkt-4",
];

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
  walletTransactions: { txid: string; balanceChange: number; fee: number; height: number | null; timestamp: number | null }[];
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
  logoutOpen: boolean;
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
  helpOpen: false,
  settingsOpen: false,
  logoutOpen: false,
};

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
const formatEstTime = (date: Date): string =>
  new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  })
    .format(date)
    .toLowerCase();
const formatSettlementDateTime = (date: Date): string =>
  `${new Intl.DateTimeFormat("en-US", {
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

type BaseCurrency = "BTC" | "USD" | "EUR" | "JPY" | "GBP" | "CNY" | "CHF" | "AUD" | "CAD";

const baseCurrencyOptions: BaseCurrency[] = ["BTC", "USD", "EUR", "JPY", "GBP", "CNY", "CHF", "AUD", "CAD"];

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
    case "USD": return new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(value);
    case "EUR": return new Intl.NumberFormat("de-DE", { style: "currency", currency: "EUR" }).format(value);
    case "GBP": return new Intl.NumberFormat("en-GB", { style: "currency", currency: "GBP" }).format(value);
    case "JPY": return new Intl.NumberFormat("ja-JP", { style: "currency", currency: "JPY", maximumFractionDigits: 0 }).format(value);
    case "CNY": return new Intl.NumberFormat("zh-CN", { style: "currency", currency: "CNY" }).format(value);
    case "CHF": return new Intl.NumberFormat("de-CH", { style: "currency", currency: "CHF" }).format(value);
    case "AUD": return new Intl.NumberFormat("en-AU", { style: "currency", currency: "AUD" }).format(value);
    case "CAD": return new Intl.NumberFormat("en-CA", { style: "currency", currency: "CAD" }).format(value);
    default: return "";
  }
}

function satsToFiatStr(sats: number): string {
  if (state.baseCurrency === "BTC") return "";
  return formatFiat(satsToFiat(sats, state.baseCurrency), state.baseCurrency);
}

function stateLabel(value: CovenantState): string {
  if (value === 0) return "UNINITIALIZED";
  if (value === 1) return "UNRESOLVED";
  if (value === 2) return "RESOLVED_YES";
  return "RESOLVED_NO";
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
  return trendingIds.map((id) => getMarketById(id));
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
  const seed = getMarketSeed(market);
  return {
    yes: 4 + (seed % 19),
    no: 3 + ((seed * 7) % 17),
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
        <span class="inline-flex items-center gap-1"><span class="h-2 w-2 rounded-full bg-emerald-300"></span>Yes ${yesPct}%</span>
        <span class="inline-flex items-center gap-1"><span class="h-2 w-2 rounded-full bg-rose-400"></span>No ${noPct}%</span>
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
          <circle cx="${yesEnd.x}" cy="${yesEnd.y}" r="1.8" fill="#5eead4" />
          <circle cx="${noEnd.x}" cy="${noEnd.y}" r="1.8" fill="#fb7185" />
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

function renderTopShell(): string {
  return `
    <header class="border-b border-slate-800 bg-slate-950/80 backdrop-blur">
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
              <button data-action="toggle-user-menu" class="flex h-9 w-9 items-center justify-center rounded-full border border-slate-700 text-slate-400 transition hover:border-slate-500 hover:text-slate-200">
                <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
              </button>
              ${state.userMenuOpen ? `<div class="absolute right-0 top-full z-50 mt-2 w-48 rounded-xl border border-slate-700 bg-slate-900 shadow-xl">
                <div class="px-3 pb-1 pt-3">
                  <div class="mb-1.5 text-[11px] text-slate-500">Display currency</div>
                  <div class="grid grid-cols-3 gap-1">
                    ${baseCurrencyOptions.map(c => `<button data-action="set-currency" data-currency="${c}" class="rounded-md px-2 py-1 text-xs transition ${c === state.baseCurrency ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:bg-slate-800 hover:text-slate-200"}">${c}</button>`).join("")}
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
              </div>` : ""}
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
            <button data-action="open-help" class="ml-auto shrink-0 rounded-full px-3 py-1.5 text-sm font-normal text-slate-500 transition hover:text-slate-300">Help</button>
          </div>
        </div>
      </div>
    </header>
    ${state.searchOpen ? `<div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm lg:hidden">
      <div class="flex items-center gap-3 border-b border-slate-800 bg-slate-950 px-4 py-3">
        <input id="global-search-mobile" value="${state.search}" class="h-10 flex-1 rounded-full border border-slate-700 bg-slate-900 px-4 text-sm text-slate-200 outline-none ring-emerald-300 transition focus:ring-2" placeholder="Trade on anything" autofocus />
        <button data-action="close-search" class="shrink-0 text-sm text-slate-400 hover:text-slate-200">Cancel</button>
      </div>
    </div>` : ""}
    ${state.helpOpen ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Help</h2>
          <button data-action="close-help" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <p class="mt-4 text-sm text-slate-400">Help content coming soon.</p>
      </div>
    </div>` : ""}
    ${state.settingsOpen ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-medium text-slate-100">Settings</h2>
          <button data-action="close-settings" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">
            <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          </button>
        </div>
        <p class="mt-4 text-sm text-slate-400">Settings content coming soon.</p>
      </div>
    </div>` : ""}
    ${state.logoutOpen ? `<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
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
    </div>` : ""}
    ${renderBackupModal(state.walletLoading)}
  `;
}

function renderHome(): string {
  if (state.activeCategory !== "Trending") {
    return renderCategoryPage();
  }

  const trending = getTrendingMarkets();
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
      <input id="limit-price" type="number" min="1" max="99" step="1" value="${Math.round(state.limitPrice * SATS_PER_FULL_CONTRACT)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
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
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.initialIssue ? "0 -> 1 Initial Issuance" : "1 -> 1 Subsequent Issuance"}</p>
        <label for="pairs-input" class="mb-1 block text-xs text-slate-400">Pairs to mint</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Required collateral</span><span>${formatSats(issueCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT (${state.pairsInput} * 2 * ${market.cptSats})</div>
        </div>
        <button data-action="submit-issue" ${paths.issue || paths.initialIssue ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.issue || paths.initialIssue ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit issuance tx</button>
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
        <p class="mb-2 text-sm text-slate-300">Path: 1 -> 1 Cancellation</p>
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
      <p class="mt-3 text-xs text-slate-500">NOSTR auth required to sign and broadcast covenant transactions.</p>
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
            <p class="mb-1 text-sm text-slate-400">${market.category} · Prediction contract</p>
            <h1 class="phi-title mb-2 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${market.question}</h1>
            <p class="mb-3 text-base text-slate-400">${market.description}</p>

            <div class="mb-4 grid gap-3 sm:grid-cols-3">
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Yes price<br/><span class="text-lg font-medium text-emerald-400">${formatProbabilityWithPercent(market.yesPrice)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">No price<br/><span class="text-lg font-medium text-rose-400">${formatProbabilityWithPercent(noPrice)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Settlement deadline<br/><span class="text-slate-100">Est. by ${formatSettlementDateTime(estimatedSettlementDate)}</span></div>
            </div>

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
              ${renderPathCard("0 -> 1 Initial issuance", paths.initialIssue, "pairs * 2 * CPT", "Outputs move to state-1 address")}
              ${renderPathCard("1 -> 1 Subsequent issuance", paths.issue, "pairs * 2 * CPT", "Collateral UTXO reconsolidated")}
              ${renderPathCard("1 -> 2/3 Oracle resolve", paths.resolve, "state commit via oracle signature", "All covenant outputs move atomically")}
              ${renderPathCard("2/3 Redemption", paths.redeem, "tokens * 2 * CPT", "Winning side burns tokens")}
              ${renderPathCard("1 Expiry redemption", paths.expiryRedeem, "tokens * CPT", "Unresolved + expiry only")}
              ${renderPathCard("1 -> 1 Cancellation", paths.cancel, "pairs * 2 * CPT", "Equal YES/NO burn")}
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
          <button data-action="submit-create-market" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950">Publish market draft</button>
          <p class="mt-2 text-xs text-slate-500">This creates a UI draft only. On-chain creation can be wired next.</p>
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
    state.walletNetwork = appState.networkStatus.network as "mainnet" | "testnet" | "regtest";
    state.walletPolicyAssetId = appState.networkStatus.policyAssetId;
  } catch (e) {
    console.warn("Failed to fetch app state:", e);
  }
}

async function refreshWallet(): Promise<void> {
  state.walletLoading = true;
  state.walletError = "";
  render();
  try {
    await invoke("sync_wallet");
    const balance = await invoke<{ assets: Record<string, number> }>("get_wallet_balance");
    state.walletBalance = balance.assets;
    const txs = await invoke<{ txid: string; balanceChange: number; fee: number; height: number | null; timestamp: number | null }[]>("get_wallet_transactions");
    state.walletTransactions = txs;
    const swaps = await invoke<PaymentSwap[]>("list_payment_swaps");
    state.walletSwaps = swaps;
  } catch (e) {
    state.walletError = String(e);
  }
  state.walletLoading = false;
  render();
}

const QR_LOGO_SVG = 'data:image/svg+xml;base64,' + btoa('<svg width="334" height="341" viewBox="0 0 334 341" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M0.19 11.59C0.19 1.58004 13.98 -4.04996 21.51 3.46004L110.88 91.89C128.28 87.15 147.01 84.56 166.53 84.56C186.05 84.56 204.79 87.14 222.19 91.89L311.54 3.47004C319.06 -4.02996 332.86 1.59004 332.86 11.6V206.56C332.98 208.58 333.05 210.61 333.05 212.65C333.05 283.39 258.5 340.74 166.53 340.74C74.56 340.74 0 283.4 0 212.65C0 210.61 0.06 208.58 0.19 206.56V11.59Z" fill="black"/><path d="M128.46 239.55L154.85 265.26V267.59C154.85 279.12 146.28 288.5 135.74 288.51H116.57C111.62 288.51 107.6 292.47 107.6 297.33C107.6 302.19 111.63 306.16 116.57 306.16H135.74C146.7 306.16 157 301.08 163.98 292.54C170.95 301.07 181.25 306.16 192.22 306.16C212.66 306.16 229.28 288.86 229.28 267.59C229.28 262.72 225.25 258.76 220.3 258.76C215.35 258.76 211.32 262.72 211.32 267.59C211.32 279.12 202.75 288.51 192.21 288.51C181.67 288.51 173.1 279.13 173.1 267.59V265.21L199.44 239.55H128.44H128.46ZM90.2699 179.49L67.1499 156.37L56.3599 167.16L79.4799 190.28L56.4399 213.32L67.2299 224.11L90.2699 201.07L113.39 224.19L124.18 213.4L101.06 190.28L124.26 167.09L113.47 156.3L90.2699 179.5V179.49ZM250.25 158.27C256.89 164.96 261.31 176.78 261.31 190.24C261.31 202.78 257.48 213.89 251.59 220.76C277 217.42 295.9 204.74 295.9 189.6C295.9 174.46 276.33 161.34 250.26 158.27H250.25ZM224.79 158.45C199.45 161.82 180.61 174.48 180.61 189.59C180.61 204.7 198.79 216.92 223.46 220.55C217.66 213.66 213.91 202.65 213.91 190.23C213.91 176.9 218.24 165.17 224.78 158.45H224.79Z" fill="#34D399"/></svg>');

async function generateQr(value: string): Promise<void> {
  try {
    const canvas = document.createElement("canvas");
    await QRCode.toCanvas(canvas, value, {
      errorCorrectionLevel: "H", margin: 4, scale: 8,
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
  } catch { state.modalQr = ""; }
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
    case "liquid_to_lightning": return "Lightning Send";
    case "lightning_to_liquid": return "Lightning Receive";
    case "bitcoin_to_liquid": return "Bitcoin Receive";
    case "liquid_to_bitcoin": return "Bitcoin Send";
    default: return flow;
  }
}

function formatSwapStatus(status: string): string {
  return status.replace(/[._]/g, " ").replace(/\b\w/g, c => c.toUpperCase());
}

function renderMnemonicGrid(mnemonic: string): string {
  const words = mnemonic.split(" ");
  return '<div class="grid grid-cols-3 gap-2">' + words.map((w, i) =>
    '<div class="flex items-baseline gap-2 rounded bg-slate-800 px-3 py-2">' +
    '<span class="text-xs text-slate-500 w-5 text-right shrink-0">' + (i + 1) + '.</span>' +
    '<span class="mono text-sm text-slate-100 whitespace-nowrap">' + w + '</span>' +
    '</div>'
  ).join("") + '</div>';
}

function renderBackupModal(loading: boolean): string {
  if (!state.walletShowBackup) return "";

  const closeBtn = '<button data-action="hide-backup" class="flex h-8 w-8 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200">' +
    '<svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>' +
    '</button>';

  let body: string;
  if (state.walletBackupMnemonic) {
    body =
      renderMnemonicGrid(state.walletBackupMnemonic) +
      '<div class="flex gap-3">' +
      '<button data-action="copy-backup-mnemonic" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Copy to clipboard</button>' +
      '<button data-action="hide-backup" class="flex-1 rounded-xl border border-slate-700 py-2.5 text-sm font-medium text-slate-300 transition hover:border-slate-500 hover:text-slate-100">Done</button>' +
      '</div>';
  } else {
    body =
      '<p class="text-sm text-slate-400">Enter your wallet password to reveal your recovery phrase.</p>' +
      '<input id="wallet-backup-password" type="password" value="' + state.walletBackupPassword + '" placeholder="Wallet password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
      '<button data-action="export-backup" class="w-full rounded-xl bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' + (loading ? " disabled" : "") + '>Show Recovery Phrase</button>';
  }

  return '<div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">' +
    '<div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">' +
    '<div class="flex items-center justify-between">' +
    '<h2 class="text-lg font-medium text-slate-100">Backup Recovery Phrase</h2>' +
    closeBtn +
    '</div>' +
    '<div class="mt-5 space-y-4">' +
    body +
    '<p class="text-xs text-slate-500"><strong class="text-slate-300">Deadcat.live does not hold user funds.</strong> If you lose your recovery phrase and password, your funds cannot be recovered.</p>' +
    '</div>' +
    '</div></div>';
}

function renderCopyable(value: string, label: string, copyAction: string): string {
  return '<div class="flex items-center gap-2">' +
    '<div class="flex-1 overflow-hidden rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">' +
    '<div class="text-xs text-slate-500">' + label + '</div>' +
    '<div class="mono text-xs text-slate-300 truncate">' + value + '</div>' +
    '</div>' +
    '<button data-action="' + copyAction + '" data-copy-value="' + value + '" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800">Copy</button>' +
    '</div>';
}

function renderModalTabs(): string {
  const tabs: Array<"lightning" | "liquid" | "bitcoin"> = ["lightning", "liquid", "bitcoin"];
  return '<div class="flex rounded-lg border border-slate-700 bg-slate-900/50 p-1 gap-1">' +
    tabs.map(t => {
      const active = state.walletModalTab === t;
      const label = t === "lightning" ? "Lightning" : t === "liquid" ? "Liquid" : "Bitcoin";
      return '<button data-action="modal-tab" data-tab-value="' + t + '" class="flex-1 rounded-md px-3 py-2 text-sm font-semibold transition ' +
        (active ? 'bg-slate-700 text-slate-100' : 'text-slate-400 hover:text-slate-200') + '">' + label + '</button>';
    }).join("") +
    '</div>';
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
        '<p class="text-xs text-slate-400">Swap ' + s.id.slice(0, 8) + '... | ' + s.expectedOnchainAmountSat.toLocaleString() + ' sats expected on Liquid</p>' +
        '<p class="text-xs text-slate-500">Expires: ' + new Date(s.invoiceExpiresAt).toLocaleString() + '</p>' +
        (state.modalQr ? '<div class="flex justify-center"><img src="' + state.modalQr + '" alt="QR" class="w-56 h-56 rounded-lg" /></div>' : '') +
        renderCopyable(s.invoice, "BOLT11 Invoice", "copy-modal-value") +
        '</div>';
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create a Lightning invoice via Boltz swap. Funds settle as L-BTC.</p>' +
        '<div class="flex gap-2">' +
        '<button data-action="receive-preset" data-preset="1000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">1k</button>' +
        '<button data-action="receive-preset" data-preset="10000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">10k</button>' +
        '<button data-action="receive-preset" data-preset="100000" class="flex-1 rounded-lg border border-slate-700 py-2 text-sm text-slate-300 hover:bg-slate-800">100k</button>' +
        '</div>' +
        '<div class="flex gap-2">' +
        '<input id="receive-amount" type="number" value="' + state.receiveAmount + '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-lightning-receive" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' + (creating ? ' disabled' : '') + '>' + (creating ? 'Creating...' : 'Create Invoice') + '</button>' +
        '</div>' +
        '</div>';
    }
  } else if (state.walletModalTab === "liquid") {
    if (state.receiveLiquidAddress) {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Send L-BTC to this address to fund your wallet.</p>' +
        (state.modalQr ? '<div class="flex justify-center"><img src="' + state.modalQr + '" alt="QR" class="w-56 h-56 rounded-lg" /></div>' : '') +
        renderCopyable(state.receiveLiquidAddress, "Liquid Address", "copy-modal-value") +
        '<button data-action="generate-liquid-address" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">New Address</button>' +
        '</div>';
    } else {
      content =
        '<div class="flex flex-col items-center gap-4 py-4">' +
        '<p class="text-sm text-slate-400">Generate a Liquid address to receive L-BTC.</p>' +
        '<button data-action="generate-liquid-address" class="rounded-lg bg-emerald-400 px-6 py-3 font-medium text-slate-950 hover:bg-emerald-300">' + (creating ? 'Generating...' : 'Generate Address') + '</button>' +
        '</div>';
    }
  } else {
    // Bitcoin tab
    if (state.receiveBitcoinSwap) {
      const s = state.receiveBitcoinSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Bitcoin Deposit Address Ready</p>' +
        '<p class="text-xs text-slate-400">Swap ' + s.id.slice(0, 8) + '... | ' + s.expectedAmountSat.toLocaleString() + ' sats expected on Liquid</p>' +
        '<p class="text-xs text-slate-500">Timeout block: ' + s.timeoutBlockHeight + '</p>' +
        (state.modalQr ? '<div class="flex justify-center"><img src="' + state.modalQr + '" alt="QR" class="w-56 h-56 rounded-lg" /></div>' : '') +
        renderCopyable(s.lockupAddress, "Bitcoin Lockup Address", "copy-modal-value") +
        (s.bip21 ? renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value") : '') +
        '</div>';
    } else {
      const pair = state.receiveBtcPairInfo;
      const pairInfo = pair
        ? '<div class="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">' +
          '<div>Min: ' + pair.minAmountSat.toLocaleString() + ' sats</div>' +
          '<div>Max: ' + pair.maxAmountSat.toLocaleString() + ' sats</div>' +
          '<div>Fee: ' + pair.feePercentage + '% + ' + pair.fixedMinerFeeTotalSat.toLocaleString() + ' sats fixed</div>' +
          '</div>'
        : '';
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create a Boltz chain swap. Send BTC on-chain to receive L-BTC.</p>' +
        pairInfo +
        '<div class="flex gap-2">' +
        '<input id="receive-amount" type="number" value="' + state.receiveAmount + '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-bitcoin-receive" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' + (creating ? ' disabled' : '') + '>' + (creating ? 'Creating...' : 'Create Address') + '</button>' +
        '</div>' +
        '</div>';
    }
  }

  if (err) {
    content += '<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">' + err + '</div>';
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
        '<p class="text-xs text-slate-400">Swap ' + s.id.slice(0, 8) + '... | ' + s.invoiceAmountSat.toLocaleString() + ' sats</p>' +
        '<p class="text-xs text-slate-500">Waiting for lockup confirmation. Expires: ' + new Date(s.invoiceExpiresAt).toLocaleString() + '</p>' +
        renderCopyable(s.lockupAddress, "Lockup Address", "copy-modal-value") +
        renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value") +
        '</div>';
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Paste a BOLT11 Lightning invoice to pay via Boltz submarine swap.</p>' +
        '<input id="send-invoice" value="' + state.sendInvoice + '" placeholder="BOLT11 invoice" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="pay-lightning-invoice" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' + (creating ? ' disabled' : '') + '>' + (creating ? 'Creating Swap...' : 'Pay via Lightning') + '</button>' +
        '</div>';
    }
  } else if (state.walletModalTab === "liquid") {
    if (state.sentLiquidResult) {
      const r = state.sentLiquidResult;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-emerald-300">Transaction Sent</p>' +
        '<p class="text-xs text-slate-400">Fee: ' + r.feeSat.toLocaleString() + ' sats</p>' +
        renderCopyable(r.txid, "Transaction ID", "copy-modal-value") +
        '</div>';
    } else {
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Send L-BTC directly to a Liquid address.</p>' +
        '<input id="send-liquid-address" value="' + state.sendLiquidAddress + '" placeholder="Liquid address" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<input id="send-liquid-amount" type="number" value="' + state.sendLiquidAmount + '" placeholder="Amount (sats)" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="send-liquid" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300"' + (creating ? ' disabled' : '') + '>' + (creating ? 'Sending...' : 'Send L-BTC') + '</button>' +
        '</div>';
    }
  } else {
    // Bitcoin tab
    if (state.sentBitcoinSwap) {
      const s = state.sentBitcoinSwap;
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm font-semibold text-slate-100">Chain Swap Created</p>' +
        '<p class="text-xs text-slate-400">Swap ' + s.id.slice(0, 8) + '... | ' + s.expectedAmountSat.toLocaleString() + ' sats expected on Bitcoin</p>' +
        '<p class="text-xs text-slate-500">Timeout block: ' + s.timeoutBlockHeight + '</p>' +
        (state.modalQr ? '<div class="flex justify-center"><img src="' + state.modalQr + '" alt="QR" class="w-56 h-56 rounded-lg" /></div>' : '') +
        renderCopyable(s.lockupAddress, "Liquid Lockup Address", "copy-modal-value") +
        (s.bip21 ? renderCopyable(s.bip21, "BIP21 URI", "copy-modal-value") : '') +
        '</div>';
    } else {
      const pair = state.sendBtcPairInfo;
      const pairInfo = pair
        ? '<div class="rounded-lg border border-slate-700 bg-slate-900 p-3 text-xs text-slate-400 space-y-1">' +
          '<div>Min: ' + pair.minAmountSat.toLocaleString() + ' sats</div>' +
          '<div>Max: ' + pair.maxAmountSat.toLocaleString() + ' sats</div>' +
          '<div>Fee: ' + pair.feePercentage + '% + ' + pair.fixedMinerFeeTotalSat.toLocaleString() + ' sats fixed</div>' +
          '</div>'
        : '';
      content =
        '<div class="space-y-3">' +
        '<p class="text-sm text-slate-400">Create an L-BTC to BTC chain swap via Boltz.</p>' +
        pairInfo +
        '<div class="flex gap-2">' +
        '<input id="send-btc-amount" type="number" value="' + state.sendBtcAmount + '" placeholder="Amount (sats)" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 focus:ring-2" />' +
        '<button data-action="create-bitcoin-send" class="shrink-0 rounded-lg bg-emerald-400 px-4 py-2 text-sm font-medium text-slate-950 hover:bg-emerald-300"' + (creating ? ' disabled' : '') + '>' + (creating ? 'Creating...' : 'Create Swap') + '</button>' +
        '</div>' +
        '</div>';
    }
  }

  if (err) {
    content += '<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">' + err + '</div>';
  }

  return content;
}

function renderWalletModal(): string {
  if (state.walletModal === "none") return "";

  const title = state.walletModal === "receive" ? "Receive Funds" : "Send Funds";
  const subtitle = state.walletModal === "receive"
    ? "Choose a method to receive funds into your Liquid wallet."
    : "Send funds from your wallet via Lightning, Liquid, or Bitcoin.";
  const body = state.walletModal === "receive" ? renderReceiveModal() : renderSendModal();

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

  const networkBadge = state.walletNetwork !== "mainnet"
    ? `<span class="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">${state.walletNetwork}</span>`
    : "";

  const errorHtml = error
    ? `<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">${error}</div>`
    : "";

  const loadingHtml = loading
    ? `<div class="text-sm text-slate-400">Loading...</div>`
    : "";

  if (state.walletStatus === "not_created") {
    if (state.walletMnemonic) {
      return `
        <div class="phi-container py-8">
          <div class="mx-auto max-w-lg space-y-6">
            <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet Created ${networkBadge}</h2>
            <div class="rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
              <p class="text-sm font-medium text-slate-200">Back up your recovery phrase! You will not see this again.</p>
              ${renderMnemonicGrid(state.walletMnemonic)}
              <button data-action="copy-mnemonic" class="mt-2 w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Copy to clipboard</button>
            </div>
            ${errorHtml}
            <button data-action="dismiss-mnemonic" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">I've saved my recovery phrase</button>
          </div>
        </div>
      `;
    }

    return `
      <div class="phi-container py-8">
        <div class="mx-auto max-w-lg space-y-6">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
          <p class="text-sm text-slate-400">No wallet found. Create a new Liquid (L-BTC) wallet or restore from a recovery phrase.</p>
          ${errorHtml}
          ${loadingHtml}

          ${!state.walletShowRestore ? `
            <div class="space-y-4 rounded-lg border border-slate-700 bg-slate-900/50 p-6">
              <h3 class="font-semibold text-slate-100">Create New Wallet</h3>
              <input id="wallet-password" type="password" value="${state.walletPassword}" placeholder="Set a password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2" />
              <button data-action="create-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>Create Wallet</button>
            </div>
            <button data-action="toggle-restore" class="text-sm text-slate-400 hover:text-slate-200 underline">Restore from recovery phrase instead</button>
          ` : `
            <div class="space-y-4 rounded-lg border border-slate-700 bg-slate-900/50 p-6">
              <h3 class="font-semibold text-slate-100">Restore Wallet</h3>
              <textarea id="wallet-restore-mnemonic" placeholder="Enter your 12-word recovery phrase" rows="3" class="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 focus:ring-2">${state.walletRestoreMnemonic}</textarea>
              <input id="wallet-password" type="password" value="${state.walletPassword}" placeholder="Set a password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2" />
              <button data-action="restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>Restore Wallet</button>
            </div>
            <button data-action="toggle-restore" class="text-sm text-slate-400 hover:text-slate-200 underline">Create new wallet instead</button>
          `}
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
            <input id="wallet-password" type="password" value="${state.walletPassword}" placeholder="Password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2" />
            <button data-action="unlock-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>Unlock</button>
          </div>
        </div>
      </div>
    `;
  }

  // Unlocked — clean dashboard
  const policyBalance = state.walletBalance && state.walletPolicyAssetId
    ? (state.walletBalance[state.walletPolicyAssetId] ?? 0)
    : 0;

  const txRows = state.walletTransactions.map((tx) => {
    const sign = tx.balanceChange >= 0 ? "+" : "";
    const color = tx.balanceChange >= 0 ? "text-emerald-300" : "text-red-300";
    const icon = tx.balanceChange >= 0 ? "&#8595;" : "&#8593;";
    const date = tx.timestamp ? new Date(tx.timestamp * 1000).toLocaleString() : "unconfirmed";
    const shortTxid = tx.txid.slice(0, 10) + "..." + tx.txid.slice(-6);
    return '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm select-none">' +
      '<div class="flex items-center gap-2">' +
      '<span class="' + color + '">' + icon + '</span>' +
      '<button data-action="open-explorer-tx" data-txid="' + tx.txid + '" class="mono text-slate-400 hover:text-slate-200 transition cursor-pointer">' + shortTxid + '</button>' +
      '<span class="text-slate-500">' + date + '</span>' +
      '</div>' +
      '<div class="text-right">' +
      '<span class="' + color + '">' + sign + formatLbtc(tx.balanceChange) + '</span>' +
      (state.baseCurrency !== "BTC" ? '<div class="text-xs text-slate-500">' + satsToFiatStr(Math.abs(tx.balanceChange)) + '</div>' : '') +
      '</div>' +
      '</div>';
  }).join("");

  const swapRows = state.walletSwaps.map((sw) => {
    return '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
      '<div>' +
      '<span class="text-slate-300">' + flowLabel(sw.flow) + '</span>' +
      '<span class="ml-2 text-slate-500">' + sw.invoiceAmountSat.toLocaleString() + ' sats</span>' +
      '</div>' +
      '<div class="flex items-center gap-2">' +
      '<span class="text-xs text-slate-500">' + formatSwapStatus(sw.status) + '</span>' +
      '<button data-action="refresh-swap" data-swap-id="' + sw.id + '" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-400 hover:bg-slate-800">Refresh</button>' +
      '</div>' +
      '</div>';
  }).join("");

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
              ${state.walletBalanceHidden
                ? `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg>`
                : `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>`}
            </button>
          </div>
          <div class="mt-1 text-3xl font-medium tracking-tight text-slate-100">${state.walletBalanceHidden ? "********" : formatLbtc(policyBalance)}</div>
          ${!state.walletBalanceHidden && state.baseCurrency !== "BTC" ? `<div class="mt-1 text-sm text-slate-400">${satsToFiatStr(policyBalance)}</div>` : ""}
          <div class="mt-3 flex items-center justify-center gap-1 rounded-full border border-slate-700 mx-auto w-fit text-xs">
            <button data-action="set-wallet-unit" data-unit="sats" class="rounded-full px-3 py-1 transition ${state.walletUnit === "sats" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-sats</button>
            <button data-action="set-wallet-unit" data-unit="btc" class="rounded-full px-3 py-1 transition ${state.walletUnit === "btc" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-BTC</button>
          </div>
        </div>

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
          ${state.walletTransactions.length === 0
            ? `<p class="text-sm text-slate-500">No transactions yet.</p>`
            : txRows}
        </div>

        <!-- Swaps -->
        ${state.walletSwaps.length > 0 ? `
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Swaps</h3>
          ${swapRows}
        </div>
        ` : ""}

        <!-- Backup modal rendered in renderTopShell -->
      </div>
    </div>
    ${renderWalletModal()}
  `;
}

function render(): void {
  app.innerHTML = `
    <div class="min-h-screen text-slate-100">
      ${renderTopShell()}
      <main>${state.view === "wallet" ? renderWallet() : state.view === "home" ? renderHome() : state.view === "detail" ? renderDetail() : renderCreateMarket()}</main>
    </div>
  `;
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
  state.limitPrice =
    getBasePriceSats(market, nextSide) / SATS_PER_FULL_CONTRACT;
  render();
}

function ticketActionAllowed(market: Market, tab: ActionTab): boolean {
  const paths = getPathAvailability(market);
  if (tab === "trade") return true;
  if (tab === "issue") return paths.initialIssue || paths.issue;
  if (tab === "redeem") return paths.redeem || paths.expiryRedeem;
  return paths.cancel;
}

render();
void fetchWalletStatus().then(() => {
  render();
  if (state.walletStatus === "unlocked") {
    void refreshWallet();
  }
});
updateEstClockLabels();
setInterval(updateEstClockLabels, 1_000);
void syncCurrentHeightFromLwk("liquid");
setInterval(() => {
  void syncCurrentHeightFromLwk("liquid");
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
  const orderType = orderTypeEl?.getAttribute(
    "data-order-type",
  ) as OrderType | null;
  const tab = tabEl?.getAttribute("data-tab") as ActionTab | null;

  // Close user menu on any click that isn't inside the menu
  if (state.userMenuOpen && action !== "toggle-user-menu" && action !== "user-settings" && action !== "user-logout" && action !== "set-currency") {
    // Check if click is inside the dropdown
    const inMenu = target.closest("[data-action='toggle-user-menu']")?.parentElement?.contains(target);
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

  if (action === "set-currency") {
    const currency = actionEl?.getAttribute("data-currency") as BaseCurrency | null;
    if (currency) { state.baseCurrency = currency; render(); }
    return;
  }

  if (action === "user-settings") {
    state.userMenuOpen = false;
    state.settingsOpen = true;
    render();
    return;
  }

  if (action === "close-settings") {
    state.settingsOpen = false;
    render();
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
    // TODO: actual logout logic
    render();
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
    state.walletBalance = null;
    state.walletTransactions = [];
    state.walletSwaps = [];
    void fetchWalletStatus().then(() => {
      state.view = "wallet";
      render();
      if (state.walletStatus === "unlocked") {
        void refreshWallet();
      }
    });
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
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("create_wallet", { password: state.walletPassword });
        state.walletMnemonic = mnemonic;
        state.walletPassword = "";
        await fetchWalletStatus();
        // Stay on not_created so mnemonic screen shows
        state.walletStatus = "not_created";
      } catch (e) {
        state.walletError = String(e);
      }
      state.walletLoading = false;
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
    render();
    (async () => {
      try {
        await invoke("restore_wallet", {
          mnemonic: state.walletRestoreMnemonic.trim(),
          password: state.walletPassword,
        });
        state.walletRestoreMnemonic = "";
        state.walletPassword = "";
        await fetchWalletStatus();
        await refreshWallet();
      } catch (e) {
        state.walletError = String(e);
        state.walletLoading = false;
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
    render();
    (async () => {
      try {
        await invoke("unlock_wallet", { password: state.walletPassword });
        state.walletPassword = "";
        await fetchWalletStatus();
        await refreshWallet();
      } catch (e) {
        state.walletError = String(e);
        state.walletLoading = false;
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

  if (action === "toggle-balance-hidden") {
    state.walletBalanceHidden = !state.walletBalanceHidden;
    render();
    return;
  }

  if (action === "set-wallet-unit") {
    const unit = actionEl?.getAttribute("data-unit") as "sats" | "btc" | null;
    if (unit) { state.walletUnit = unit; render(); }
    return;
  }

  if (action === "sync-wallet") {
    void refreshWallet();
    return;
  }

  if (action === "open-explorer-tx") {
    const txid = actionEl?.getAttribute("data-txid");
    if (txid) {
      const base = state.walletNetwork === "testnet" ? "https://blockstream.info/liquidtestnet" : "https://blockstream.info/liquid";
      void openUrl(base + "/tx/" + txid);
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
        const pairs = await invoke<BoltzChainSwapPairsInfo>("get_chain_swap_pairs");
        state.receiveBtcPairInfo = pairs.bitcoinToLiquid;
      } catch { /* ignore */ }
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
        const pairs = await invoke<BoltzChainSwapPairsInfo>("get_chain_swap_pairs");
        state.sendBtcPairInfo = pairs.liquidToBitcoin;
      } catch { /* ignore */ }
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
    const tab = actionEl?.getAttribute("data-tab-value") as "lightning" | "liquid" | "bitcoin" | null;
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
    if (amt <= 0) { state.receiveError = "Enter a valid amount."; render(); return; }
    state.receiveCreating = true;
    state.receiveError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzLightningReceiveCreated>("create_lightning_receive", { amountSat: amt });
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
        const addr = await invoke<{ address: string }>("get_wallet_address", { index: state.receiveLiquidAddressIndex });
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
    if (amt <= 0) { state.receiveError = "Enter a valid amount."; render(); return; }
    state.receiveCreating = true;
    state.receiveError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzChainSwapCreated>("create_bitcoin_receive", { amountSat: amt });
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
    if (!invoice) { state.sendError = "Paste a BOLT11 invoice."; render(); return; }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzSubmarineSwapCreated>("pay_lightning_invoice", { invoice });
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
    if (!address || amountSat <= 0) { state.sendError = "Enter address and amount."; render(); return; }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const result = await invoke<{ txid: string; feeSat: number }>("send_lbtc", {
          address,
          amountSat,
          feeRate: null,
        });
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
    if (amt <= 0) { state.sendError = "Enter a valid amount."; render(); return; }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzChainSwapCreated>("create_bitcoin_send", { amountSat: amt });
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
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("get_wallet_mnemonic", { password: state.walletBackupPassword });
        state.walletBackupMnemonic = mnemonic;
        state.walletBackupPassword = "";
        state.walletBackedUp = true;
      } catch (e) {
        state.walletError = String(e);
      }
      state.walletLoading = false;
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
    state.limitPrice =
      getBasePriceSats(market, closeSide) / SATS_PER_FULL_CONTRACT;
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
    state.limitPrice = getBasePriceSats(market, side) / SATS_PER_FULL_CONTRACT;
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
      state.limitPrice =
        getBasePriceSats(market, pickedSide) / SATS_PER_FULL_CONTRACT;
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

  if (orderType) {
    state.orderType = orderType;
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
          "Complete question, settlement rule, source, and settlement deadline before publishing.",
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
      const noSats = SATS_PER_FULL_CONTRACT - yesSats;
      window.alert(
        `Prepared new market draft.\nQuestion: ${question}\nCategory: ${state.createCategory}\nSettlement deadline: ${new Date(state.createSettlementInput).toLocaleString()}\nResolution source: ${source}\nStart prices: YES ${yesSats} sats / NO ${noSats} sats`,
      );
      return;
    }

    const market = getSelectedMarket();
    if (action === "submit-trade") {
      const preview = getTradePreview(market);
      if (state.orderType === "market" && preview.fill.filledContracts <= 0) {
        window.alert("Order is currently not fillable. Adjust price or size.");
        return;
      }
      if (
        state.tradeIntent === "close" &&
        preview.requestedContracts > preview.positionContracts + 0.0001
      ) {
        window.alert("Close size exceeds your available position.");
        return;
      }

      const needsGuardrailConfirm =
        state.orderType === "market" &&
        (preview.slippagePct >= 5 ||
          preview.executedSats >= LARGE_ORDER_SATS_GUARDRAIL);

      if (needsGuardrailConfirm) {
        const confirmed = window.confirm(
          `Large market ${state.tradeIntent} detected.\nEstimated slippage: ${preview.slippagePct.toFixed(1)}%\nEstimated notional: ${formatSats(preview.executedSats)}\nYes + No = ${SATS_PER_FULL_CONTRACT} sats.\nProceed?`,
        );
        if (!confirmed) return;
      }

      window.alert(
        `Prepared trade tx for ${market.marketId}.\nIntent: ${state.tradeIntent.toUpperCase()} ${state.selectedSide.toUpperCase()}\nOrder: ${state.orderType.toUpperCase()}\nEstimated fill now: ${preview.fill.filledContracts.toFixed(2)} contracts\nEstimated notional: ${formatSats(preview.executedSats)}\nYes + No = ${SATS_PER_FULL_CONTRACT} sats.`,
      );
      return;
    }

    window.alert(
      `Prepared ${action.replace("submit-", "")} transaction for ${market.marketId}.`,
    );
  }
});

app.addEventListener("input", (event) => {
  const target = event.target as HTMLInputElement;

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
    const parsedSats = Math.floor(
      Number(target.value) || SATS_PER_FULL_CONTRACT / 2,
    );
    const clampedSats = Math.max(
      1,
      Math.min(SATS_PER_FULL_CONTRACT - 1, parsedSats),
    );
    state.limitPrice = Math.max(
      0.01,
      Math.min(0.99, clampedSats / SATS_PER_FULL_CONTRACT),
    );
    render();
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
    state.walletRestoreMnemonic = (target as unknown as HTMLTextAreaElement).value;
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
  if (event.key !== "Enter") return;

  if (target.id === "wallet-backup-password") {
    event.preventDefault();
    const btn = document.querySelector("[data-action='export-backup']") as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "wallet-password") {
    event.preventDefault();
    if (state.walletStatus === "not_created") {
      if (state.walletShowRestore) {
        target.closest("[data-action='restore-wallet']")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        const btn = document.querySelector("[data-action='restore-wallet']") as HTMLElement | null;
        btn?.click();
      } else {
        const btn = document.querySelector("[data-action='create-wallet']") as HTMLElement | null;
        btn?.click();
      }
    } else if (state.walletStatus === "locked") {
      const btn = document.querySelector("[data-action='unlock-wallet']") as HTMLElement | null;
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
