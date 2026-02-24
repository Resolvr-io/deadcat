import "./style.css";
import { invoke } from "@tauri-apps/api/core";

const app = document.querySelector<HTMLDivElement>("#app")!;

function dismissSplash(): void {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("fade-out");
  splash.addEventListener("transitionend", () => splash.remove(), {
    once: true,
  });
  setTimeout(() => splash.remove(), 900);
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
type ViewMode = "home" | "detail" | "create";
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
    expiryHeight: 380,
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
  limitPriceDraft: string;
  pairsInput: number;
  tokensInput: number;
  createQuestion: string;
  createDescription: string;
  createCategory: MarketCategory;
  createResolutionSource: string;
  createSettlementInput: string;
  createStartingYesSats: number;
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
};

const SATS_PER_FULL_CONTRACT = 100;
const formatProbabilitySats = (price: number): string =>
  `${Math.round(price * SATS_PER_FULL_CONTRACT)} sats`;
const formatProbabilityWithPercent = (price: number): string =>
  `${Math.round(price * 100)}% (${formatProbabilitySats(price)})`;
const formatPercent = (value: number): string =>
  `${value >= 0 ? "+" : ""}${value.toFixed(1)}%`;
const formatBtc = (value: number): string => `${value.toFixed(4)} BTC`;
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

function setLimitPriceSats(limitPriceSats: number): void {
  const clamped = clampContractPriceSats(limitPriceSats);
  state.limitPrice = clamped / SATS_PER_FULL_CONTRACT;
  state.limitPriceDraft = String(clamped);
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
        <span class="inline-flex items-center gap-1"><span class="h-2 w-2 rounded-full bg-blue-300"></span>No ${noPct}%</span>
        <span class="text-slate-500">Yes + No = ${SATS_PER_FULL_CONTRACT} sats</span>
        ${
          market.isLive
            ? '<span class="inline-flex items-center gap-1 rounded-full border border-rose-500/50 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-rose-300"><span class="liveIndicatorDot"></span>Live · Round 1</span>'
            : ""
        }
      </div>
      <div class="pointer-events-none absolute inset-x-3 top-10 bottom-8">
        <svg viewBox="0 0 100 100" class="h-full w-full">
          <polyline fill="none" stroke="#5eead4" stroke-width="${market.isLive ? "1.6" : "1.3"}" stroke-opacity="${market.isLive ? "1" : "0.7"}" points="${yesPath}" />
          <polyline fill="none" stroke="#60a5fa" stroke-width="${market.isLive ? "1.6" : "1.3"}" stroke-opacity="${market.isLive ? "1" : "0.7"}" points="${noPath}" />
          ${
            market.isLive
              ? `<circle class="chartLivePulse chartLivePulseYes" cx="${yesEnd.x}" cy="${yesEnd.y}" r="1.8" />
          <circle class="chartLivePulse chartLivePulseNo" cx="${noEnd.x}" cy="${noEnd.y}" r="1.8" />`
              : ""
          }
          <circle cx="${yesEnd.x}" cy="${yesEnd.y}" r="1.8" fill="#5eead4" />
          <circle cx="${noEnd.x}" cy="${noEnd.y}" r="1.8" fill="#60a5fa" />
        </svg>
        <div class="absolute text-[12px] font-semibold text-emerald-300" style="left: calc(${yesEnd.x}% - 56px); top: calc(${yesEnd.y}% - 8px)">Yes ${yesPct}%</div>
        <div class="absolute text-[12px] font-semibold text-blue-300" style="left: calc(${noEnd.x}% - 50px); top: calc(${noEnd.y}% - 8px)">No ${noPct}%</div>
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
      <div class="phi-container py-3 lg:py-5">
        <div class="flex flex-wrap items-center gap-3">
          <button data-action="go-home" class="text-3xl font-bold leading-none tracking-tight text-emerald-300">deadcat.live</button>
          <nav class="flex items-center gap-4 text-sm font-semibold uppercase tracking-[0.12em] text-slate-300">
            <button class="rounded-full bg-slate-800 px-4 py-2 text-slate-100">Markets</button>
            <button class="hover:text-rose-300">Live</button>
            <button class="hover:text-slate-100">Social</button>
            <button data-action="open-create-market" class="rounded-full border border-emerald-400/40 px-4 py-2 text-emerald-300 hover:bg-emerald-400/10">New Market</button>
          </nav>
          <div class="ml-auto flex w-full items-center gap-2 md:w-auto">
            <input id="global-search" value="${state.search}" class="h-11 w-full rounded-full border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-300 transition focus:ring-2 md:w-[420px]" placeholder="Trade on anything" />
          </div>
        </div>
      </div>
      <div class="border-t border-slate-800">
        <div class="phi-container py-2">
          <div id="category-row" class="flex items-center gap-1 overflow-x-auto whitespace-nowrap">
            ${categories
              .map((category) => {
                const active = state.activeCategory === category;
                return `<button data-category="${category}" class="rounded-full px-3 py-1.5 text-sm font-semibold transition ${
                  active
                    ? "text-slate-100"
                    : "text-slate-400 hover:text-slate-200"
                }">${category}</button>`;
              })
              .join("")}
          </div>
        </div>
      </div>
    </header>
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
              <h1 class="phi-title text-2xl font-semibold leading-tight text-slate-100 lg:text-[34px]">${featured.question}</h1>
              <div class="flex items-center gap-2">
                <button data-action="trending-prev" class="h-11 w-11 rounded-full border border-slate-700 text-xl text-slate-200">&#8249;</button>
                <p class="w-20 text-center text-base font-semibold text-slate-200">${state.trendingIndex + 1} of ${trending.length}</p>
                <button data-action="trending-next" class="h-11 w-11 rounded-full border border-slate-700 text-xl text-slate-200">&#8250;</button>
              </div>
            </div>

            <div class="grid gap-[21px] lg:grid-cols-[1fr_1.618fr]">
              <div>
                <p class="mb-3 text-base font-semibold uppercase tracking-wide ${featured.isLive ? "text-rose-300" : "text-slate-400"}">${featured.isLive ? "Live" : "Scheduled"}</p>
                <div class="mb-3 grid grid-cols-2 gap-2 text-xs text-slate-400">
                  <div class="rounded-lg border border-slate-800 bg-slate-900/50 p-2">State<br/><span class="text-slate-200">${stateLabel(featured.state)}</span></div>
                  <div class="rounded-lg border border-slate-800 bg-slate-900/50 p-2">Volume<br/><span class="text-slate-200">${formatVolumeBtc(featured.volumeBtc)}</span></div>
                </div>
                <div class="space-y-3 text-lg text-slate-200">
                  <div class="flex items-center justify-between"><span>Yes contract</span><button data-open-market="${featured.id}" data-open-side="yes" data-open-intent="buy" class="rounded-full border border-emerald-600 px-4 py-1 text-emerald-300 transition hover:bg-emerald-500/10">${formatProbabilityWithPercent(featured.yesPrice)}</button></div>
                  <div class="flex items-center justify-between"><span>No contract</span><button data-open-market="${featured.id}" data-open-side="no" data-open-intent="buy" class="rounded-full border border-rose-600 px-4 py-1 text-rose-300 transition hover:bg-rose-500/10">${formatProbabilityWithPercent(featuredNo)}</button></div>
                </div>
                <p class="mt-3 text-[15px] text-slate-400">${featured.description}</p>
                <button data-open-market="${featured.id}" class="mt-5 rounded-xl bg-emerald-300 px-5 py-2.5 text-base font-semibold text-slate-950">Open contract</button>
              </div>
              <div>${chartSkeleton(featured)}</div>
            </div>
          </div>

          <section>
            <div class="mb-3 flex items-center justify-between">
              <h2 class="text-xl font-semibold text-slate-100">Top Markets</h2>
              <p class="text-sm text-slate-400">${topMarkets.length} shown</p>
            </div>
            <div class="grid gap-3 md:grid-cols-2">
              ${topMarkets
                .map((market) => {
                  const no = 1 - market.yesPrice;
                  return `
                    <button data-open-market="${market.id}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
                      <p class="mb-2 text-sm uppercase tracking-wide text-slate-400">${market.category} ${market.isLive ? "· LIVE" : ""}</p>
                      <p class="mb-3 max-h-14 overflow-hidden text-lg font-semibold text-slate-100">${market.question}</p>
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
            <h3 class="mb-3 text-xl font-semibold text-slate-100">Trending</h3>
            <div class="space-y-4">
              ${trending
                .slice(0, 3)
                .map((market, idx) => {
                  return `
                    <button data-open-market="${market.id}" class="w-full text-left">
                      <div class="flex items-start justify-between gap-2">
                        <p class="w-full text-base font-semibold text-slate-200">${idx + 1}. ${market.question}</p>
                        <p class="text-base font-semibold text-slate-100">${Math.round(market.yesPrice * 100)}%</p>
                      </div>
                      <p class="mt-1 text-xs text-slate-500">${market.category}</p>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 text-xl font-semibold text-slate-100">Top movers</h3>
            <div class="space-y-4">
              ${topMovers
                .map((market, idx) => {
                  return `
                    <button data-open-market="${market.id}" class="w-full text-left">
                      <div class="flex items-start justify-between gap-2">
                        <p class="w-full text-base font-semibold text-slate-200">${idx + 1}. ${market.question}</p>
                        <p class="text-base font-semibold ${market.change24h >= 0 ? "text-emerald-300" : "text-rose-300"}">${formatPercent(market.change24h)}</p>
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
            <h1 class="text-2xl font-semibold text-slate-100">${category}</h1>
            <div class="flex items-center gap-2 text-sm text-slate-400">
              <button class="rounded-full border border-slate-700 px-3 py-1.5">Trending</button>
              <button class="rounded-full border border-slate-700 px-3 py-1.5">Frequency</button>
            </div>
          </div>
          <div class="mb-4 grid gap-2 sm:grid-cols-3">
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs uppercase tracking-wide text-slate-500">Contracts</p>
              <p class="text-lg font-semibold text-slate-100">${categoryMarkets.length}</p>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs uppercase tracking-wide text-slate-500">Live now</p>
              <p class="text-lg font-semibold text-rose-300">${liveContracts.length}</p>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs uppercase tracking-wide text-slate-500">24h volume</p>
              <p class="text-lg font-semibold text-slate-100">${formatVolumeBtc(
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
                      <span class="uppercase tracking-wide text-slate-400">${market.category}</span>
                      <span class="${market.isLive ? "text-rose-300" : "text-slate-500"}">${market.isLive ? "LIVE" : "SCHEDULED"}</span>
                    </div>
                    <p class="mb-3 text-lg font-semibold text-slate-100">${market.question}</p>
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
            <h3 class="mb-3 text-lg font-semibold text-slate-100">Live contracts</h3>
            <div class="space-y-3">
              ${
                liveContracts.length
                  ? liveContracts
                      .map(
                        (market) => `
                      <button data-open-market="${market.id}" class="w-full text-left">
                        <p class="text-sm font-semibold text-slate-200">${market.question}</p>
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
            <h3 class="mb-3 text-lg font-semibold text-slate-100">Highest liquidity</h3>
            <div class="space-y-3">
              ${highestLiquidity
                .map(
                  (market, idx) => `
                <button data-open-market="${market.id}" class="flex w-full items-start justify-between gap-2 text-left">
                  <p class="text-sm text-slate-300">${idx + 1}. ${market.question}</p>
                  <p class="text-sm font-semibold text-emerald-300">${formatVolumeBtc(market.liquidityBtc)}</p>
                </button>`,
                )
                .join("")}
            </div>
          </section>
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-lg font-semibold text-slate-100">State mix</h3>
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
      ? "border-emerald-800 bg-emerald-950/20 text-emerald-200"
      : "border-slate-800 bg-slate-950/60 text-slate-400"
  }">
    <div class="mb-1 flex items-center justify-between gap-2">
      <p class="text-sm font-semibold">${label}</p>
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
          <button data-trade-intent="open" class="border-b-2 pb-1 text-2xl font-semibold ${state.tradeIntent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Buy</button>
          <button data-trade-intent="close" class="border-b-2 pb-1 text-2xl font-semibold ${state.tradeIntent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}">Sell</button>
        </div>
        <div class="flex items-center gap-2 rounded-lg border border-slate-700 p-1">
          <button data-order-type="market" class="rounded px-3 py-1 text-sm ${state.orderType === "market" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Market</button>
          <button data-order-type="limit" class="rounded px-3 py-1 text-sm ${state.orderType === "limit" ? "bg-slate-200 text-slate-950" : "text-slate-300"}">Limit</button>
        </div>
      </div>
      <div class="mb-3 grid grid-cols-2 gap-2">
        <button data-side="yes" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "yes" ? (state.tradeIntent === "open" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-amber-400 bg-amber-400/20 text-amber-200") : "border-slate-700 text-slate-300"}">Yes ${yesDisplaySats} sats</button>
        <button data-side="no" class="rounded-xl border px-3 py-3 text-lg font-semibold ${state.selectedSide === "no" ? (state.tradeIntent === "open" ? "border-rose-400 bg-rose-400/20 text-rose-200" : "border-blue-400 bg-blue-400/20 text-blue-200") : "border-slate-700 text-slate-300"}">No ${noDisplaySats} sats</button>
      </div>
      <div class="mb-3 flex items-center justify-between gap-2">
        <label class="text-xs uppercase tracking-wide text-slate-400">Amount</label>
        <div class="grid grid-cols-2 gap-2">
          <button data-size-mode="sats" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "sats" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">sats</button>
          <button data-size-mode="contracts" class="rounded border px-2 py-1 text-xs ${state.sizeMode === "contracts" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">contracts</button>
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
      <label for="limit-price" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Limit price (sats)</label>
      <div class="mb-3 grid grid-cols-[40px_1fr_40px] gap-2">
        <button data-action="step-limit-price" data-limit-price-delta="-1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-base font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Decrease limit price">-</button>
        <input id="limit-price" type="text" inputmode="numeric" pattern="[0-9]*" maxlength="2" value="${state.limitPriceDraft}" class="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2" />
        <button data-action="step-limit-price" data-limit-price-delta="1" class="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-base font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800" aria-label="Increase limit price">+</button>
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
          <p class="text-xs uppercase tracking-wide text-slate-500">Advanced actions</p>
          <button data-action="toggle-advanced-actions" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-300">${state.showAdvancedActions ? "Hide" : "Show"}</button>
        </div>
      </section>
      ${
        state.showAdvancedActions
          ? `
      <div class="mt-3 grid grid-cols-3 gap-2">
        <button data-tab="issue" class="rounded border px-3 py-2 text-sm ${state.actionTab === "issue" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Issue</button>
        <button data-tab="redeem" class="rounded border px-3 py-2 text-sm ${state.actionTab === "redeem" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Redeem</button>
        <button data-tab="cancel" class="rounded border px-3 py-2 text-sm ${state.actionTab === "cancel" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Cancel</button>
      </div>
      ${
        state.actionTab === "issue"
          ? `
      <div class="mt-3">
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.initialIssue ? "0 -> 1 Initial Issuance" : "1 -> 1 Subsequent Issuance"}</p>
        <label for="pairs-input" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Pairs to mint</label>
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
        <label for="tokens-input" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Tokens to burn</label>
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
        <label for="pairs-input" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Matched YES/NO pairs to burn</label>
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
          ? `<div class="mb-4 rounded-xl border border-amber-700 bg-amber-950/20 px-4 py-3 text-sm text-amber-200">Market expired unresolved at height ${market.expiryHeight}. Expiry redemption path is active. Issuance and oracle resolve are disabled.</div>`
          : ""
      }
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] lg:p-[34px]">
            <button data-action="go-home" class="mb-4 rounded-lg border border-slate-700 px-3 py-1 text-sm text-slate-300">Back to markets</button>
            <p class="mb-1 text-sm text-slate-400">${market.category} · Prediction contract</p>
            <h1 class="phi-title mb-2 text-2xl font-semibold leading-tight text-slate-100 lg:text-[34px]">${market.question}</h1>
            <p class="mb-3 text-base text-slate-400">${market.description}</p>

            <div class="mb-4 grid gap-3 sm:grid-cols-3">
              <div class="rounded-xl border border-emerald-800/60 bg-emerald-950/20 p-3 text-sm text-emerald-200">Yes price<br/><span class="text-lg font-semibold text-emerald-100">${formatProbabilityWithPercent(market.yesPrice)}</span></div>
              <div class="rounded-xl border border-rose-800/60 bg-rose-950/20 p-3 text-sm text-rose-200">No price<br/><span class="text-lg font-semibold text-rose-100">${formatProbabilityWithPercent(noPrice)}</span></div>
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
              <h1 class="phi-title text-2xl font-semibold text-slate-100 lg:text-[34px]">Create New Market</h1>
            </div>
            <button data-action="cancel-create-market" class="rounded-lg border border-slate-700 px-3 py-2 text-sm text-slate-300">Back</button>
          </div>

          <div class="space-y-4">
            <div>
              <label for="create-question" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Question</label>
              <input id="create-question" value="${state.createQuestion}" maxlength="140" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Will X happen by Y?" />
            </div>

            <div>
              <label for="create-description" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Settlement rule</label>
              <textarea id="create-description" rows="3" maxlength="280" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Define exactly how YES/NO resolves.">${state.createDescription}</textarea>
            </div>

            <div class="grid gap-4 md:grid-cols-2">
              <div>
                <label for="create-category" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Category</label>
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
                <label for="create-settlement" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Settlement deadline</label>
                <input id="create-settlement" type="datetime-local" value="${state.createSettlementInput}" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
              </div>
            </div>

            <div>
              <label for="create-resolution-source" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Resolution source</label>
              <input id="create-resolution-source" value="${state.createResolutionSource}" maxlength="120" class="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" placeholder="Official source (e.g., NHC advisory, FEC filing, exchange index)" />
            </div>

            <div>
              <label for="create-yes-sats" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Starting Yes price (sats out of 100)</label>
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
              <div class="rounded-lg border border-emerald-700/60 bg-emerald-950/20 p-2 text-center text-emerald-200">Yes ${yesSats} sats</div>
              <div class="rounded-lg border border-rose-700/60 bg-rose-950/20 p-2 text-center text-rose-200">No ${noSats} sats</div>
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

function render(): void {
  app.innerHTML = `
    <div class="min-h-screen text-slate-100">
      ${renderTopShell()}
      <main>${state.view === "home" ? renderHome() : state.view === "detail" ? renderDetail() : renderCreateMarket()}</main>
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

render();
updateEstClockLabels();
dismissSplash();
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
  const limitPriceDelta = Number(
    actionEl?.getAttribute("data-limit-price-delta") ?? "",
  );
  const orderType = orderTypeEl?.getAttribute(
    "data-order-type",
  ) as OrderType | null;
  const tab = tabEl?.getAttribute("data-tab") as ActionTab | null;

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

  if (action === "open-create-market") {
    state.view = "create";
    render();
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

  if (target.id === "global-search") {
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
