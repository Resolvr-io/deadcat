import "./style.css";

const app = document.querySelector<HTMLDivElement>("#app")!;

type NavCategory =
  | "Trending"
  | "Politics"
  | "Sports"
  | "Culture"
  | "Crypto"
  | "Weather"
  | "Macro";
type MarketCategory = Exclude<NavCategory, "Trending">;
type ViewMode = "home" | "detail";
type Side = "yes" | "no";
type OrderType = "market" | "limit";
type ActionTab = "trade" | "issue" | "redeem" | "cancel";
type CovenantState = 0 | 1 | 2 | 3;

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

const categories: NavCategory[] = [
  "Trending",
  "Politics",
  "Sports",
  "Culture",
  "Crypto",
  "Weather",
  "Macro",
];

const markets: Market[] = [
  {
    id: "mkt-1",
    question: "Will BTC close above $120,000 by Dec 31, 2026?",
    category: "Crypto",
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
    category: "Crypto",
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
    category: "Crypto",
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

const state: {
  view: ViewMode;
  activeCategory: NavCategory;
  search: string;
  trendingIndex: number;
  selectedMarketId: string;
  selectedSide: Side;
  orderType: OrderType;
  actionTab: ActionTab;
  tradeSizeBtc: number;
  limitPrice: number;
  pairsInput: number;
  tokensInput: number;
} = {
  view: "home",
  activeCategory: "Trending",
  search: "",
  trendingIndex: 0,
  selectedMarketId: "mkt-3",
  selectedSide: "yes",
  orderType: "limit",
  actionTab: "trade",
  tradeSizeBtc: 0.05,
  limitPrice: 0.5,
  pairsInput: 10,
  tokensInput: 25,
};

const SATS_PER_FULL_CONTRACT = 1000;
const formatProbabilitySats = (price: number): string =>
  `${Math.round(price * SATS_PER_FULL_CONTRACT)} sats`;
const formatProbabilityWithPercent = (price: number): string =>
  `${formatProbabilitySats(price)} (${Math.round(price * 100)}%)`;
const formatPercent = (value: number): string =>
  `${value >= 0 ? "+" : ""}${value.toFixed(1)}%`;
const formatBtc = (value: number): string => `${value.toFixed(4)} BTC`;
const formatSats = (value: number): string => `${value.toLocaleString()} sats`;
const formatVolumeBtc = (value: number): string =>
  value >= 1000
    ? `${(value / 1000).toFixed(1)}K BTC`
    : `${value.toFixed(1)} BTC`;
const formatEstTime = (date: Date): string =>
  new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  })
    .format(date)
    .toLowerCase();

function stateLabel(value: CovenantState): string {
  if (value === 0) return "UNINITIALIZED";
  if (value === 1) return "UNRESOLVED";
  if (value === 2) return "RESOLVED_YES";
  return "RESOLVED_NO";
}

function isExpired(market: Market): boolean {
  return market.currentHeight >= market.expiryHeight;
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

function getExecutionPrice(market: Market): number {
  if (state.orderType === "market") {
    return state.selectedSide === "yes" ? market.yesPrice : 1 - market.yesPrice;
  }
  return Math.max(0.01, Math.min(0.99, state.limitPrice));
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
          </nav>
          <div class="ml-auto flex w-full items-center gap-2 md:w-auto">
            <input id="global-search" value="${state.search}" class="h-11 w-full rounded-full border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-300 transition focus:ring-2 md:w-[420px]" placeholder="Trade on anything" />
            <button class="h-11 rounded-full border border-slate-700 px-4 text-sm font-semibold text-slate-200">Log in</button>
            <button class="h-11 rounded-full bg-emerald-300 px-4 text-sm font-semibold text-slate-950">Sign up</button>
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
                  <div class="flex items-center justify-between"><span>Yes contract</span><span class="rounded-full border border-emerald-600 px-4 py-1 text-emerald-300">${formatProbabilityWithPercent(featured.yesPrice)}</span></div>
                  <div class="flex items-center justify-between"><span>No contract</span><span class="rounded-full border border-rose-600 px-4 py-1 text-rose-300">${formatProbabilityWithPercent(featuredNo)}</span></div>
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
  const executionPrice = getExecutionPrice(market);
  const tradeValue = Math.max(0.001, state.tradeSizeBtc);
  const executionFee = tradeValue * EXECUTION_FEE_RATE;
  const shares = tradeValue / executionPrice;
  const grossPayout = shares;
  const grossProfit = Math.max(0, grossPayout - tradeValue);
  const winFee = grossProfit * WIN_FEE_RATE;
  const netIfCorrect = grossPayout - executionFee - winFee;

  const issueCollateral = state.pairsInput * 2 * market.cptSats;
  const cancelCollateral = state.pairsInput * 2 * market.cptSats;
  const redeemRate = paths.redeem
    ? 2 * market.cptSats
    : paths.expiryRedeem
      ? market.cptSats
      : 0;
  const redeemCollateral = state.tokensInput * redeemRate;

  const tabClass = (tab: ActionTab): string =>
    `rounded-lg border px-3 py-2 text-sm ${state.actionTab === tab ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}`;

  return `
    <aside class="rounded-[21px] border border-slate-800 bg-slate-900/80 p-[21px]">
      <p class="panel-subtitle">Contract Action Ticket</p>
      <p class="mb-3 mt-1 text-sm text-slate-300">Path-gated controls based on covenant state and expiry rules.</p>
      <div class="mb-3 grid grid-cols-2 gap-2">
        <button data-tab="trade" class="${tabClass("trade")}">Trade</button>
        <button data-tab="issue" class="${tabClass("issue")}">Issue</button>
        <button data-tab="redeem" class="${tabClass("redeem")}">Redeem</button>
        <button data-tab="cancel" class="${tabClass("cancel")}">Cancel</button>
      </div>

      ${
        state.actionTab === "trade"
          ? `
        <div class="mb-3 grid grid-cols-2 gap-2">
          <button data-side="yes" class="rounded-lg border px-3 py-2 text-sm ${state.selectedSide === "yes" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Yes</button>
          <button data-side="no" class="rounded-lg border px-3 py-2 text-sm ${state.selectedSide === "no" ? "border-rose-400 bg-rose-400/20 text-rose-200" : "border-slate-700 text-slate-300"}">No</button>
        </div>
        <div class="mb-3 grid grid-cols-2 gap-2">
          <button data-order-type="market" class="rounded-lg border px-3 py-2 text-sm ${state.orderType === "market" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Market</button>
          <button data-order-type="limit" class="rounded-lg border px-3 py-2 text-sm ${state.orderType === "limit" ? "border-emerald-400 bg-emerald-400/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Limit</button>
        </div>
        <div class="${state.orderType === "market" ? "hidden" : "block"}">
          <label for="limit-price" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Limit price</label>
          <input id="limit-price" type="number" min="0.01" max="0.99" step="0.01" value="${state.limitPrice.toFixed(2)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        </div>
        <label for="trade-size" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Trade size (BTC)</label>
        <input id="trade-size" type="number" min="0.001" step="0.001" value="${state.tradeSizeBtc.toFixed(3)}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between py-1"><span>Execution price</span><span>${formatProbabilityWithPercent(executionPrice)} (${state.orderType})</span></div>
          <div class="flex items-center justify-between py-1"><span>Order value</span><span>${formatBtc(tradeValue)}</span></div>
          <div class="flex items-center justify-between py-1"><span>Execution fee (1%)</span><span>${formatBtc(executionFee)}</span></div>
          <div class="flex items-center justify-between py-1"><span>Potential gross payout</span><span>${formatBtc(grossPayout)}</span></div>
          <div class="flex items-center justify-between py-1"><span>Winning PnL fee (2%)</span><span>${formatBtc(winFee)}</span></div>
          <div class="mt-2 border-t border-slate-800 pt-2 font-semibold"><div class="flex items-center justify-between"><span>Net if correct</span><span>${formatBtc(netIfCorrect)}</span></div></div>
        </div>
        <button data-action="submit-trade" class="mt-4 w-full rounded-lg bg-emerald-300 px-4 py-2 font-semibold text-slate-950">Submit prediction contract</button>
      `
          : ""
      }

      ${
        state.actionTab === "issue"
          ? `
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.initialIssue ? "0 -> 1 Initial Issuance" : "1 -> 1 Subsequent Issuance"}</p>
        <label for="pairs-input" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Pairs to mint</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Required collateral</span><span>${formatSats(issueCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT (${state.pairsInput} * 2 * ${market.cptSats})</div>
        </div>
        <button data-action="submit-issue" ${paths.issue || paths.initialIssue ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.issue || paths.initialIssue ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit issuance tx</button>
      `
          : ""
      }

      ${
        state.actionTab === "redeem"
          ? `
        <p class="mb-2 text-sm text-slate-300">Path: ${paths.redeem ? "Post-resolution redemption" : paths.expiryRedeem ? "Expiry redemption" : "Unavailable"}</p>
        <label for="tokens-input" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Tokens to burn</label>
        <input id="tokens-input" type="number" min="1" step="1" value="${state.tokensInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral withdrawn</span><span>${formatSats(redeemCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: tokens * ${paths.redeem ? "2*CPT" : paths.expiryRedeem ? "CPT" : "N/A"}</div>
        </div>
        <button data-action="submit-redeem" ${paths.redeem || paths.expiryRedeem ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.redeem || paths.expiryRedeem ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit redemption tx</button>
      `
          : ""
      }

      ${
        state.actionTab === "cancel"
          ? `
        <p class="mb-2 text-sm text-slate-300">Path: 1 -> 1 Cancellation</p>
        <label for="pairs-input" class="mb-1 block text-xs uppercase tracking-wide text-slate-400">Matched YES/NO pairs to burn</label>
        <input id="pairs-input" type="number" min="1" step="1" value="${state.pairsInput}" class="mb-3 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm" />
        <div class="rounded-xl border border-slate-800 bg-slate-950/70 p-3 text-sm">
          <div class="flex items-center justify-between"><span>Collateral refund</span><span>${formatSats(cancelCollateral)}</span></div>
          <div class="mt-1 text-xs text-slate-400">Formula: pairs * 2 * CPT</div>
        </div>
        <button data-action="submit-cancel" ${paths.cancel ? "" : "disabled"} class="mt-4 w-full rounded-lg ${paths.cancel ? "bg-emerald-300 text-slate-950" : "bg-slate-700 text-slate-400"} px-4 py-2 font-semibold">Submit cancellation tx</button>
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

            <div class="mb-4 grid gap-3 sm:grid-cols-4">
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">State<br/><span class="text-slate-100">${market.state} · ${stateLabel(market.state)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Expiry<br/><span class="text-slate-100">${market.expiryHeight}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">CPT<br/><span class="text-slate-100">${formatSats(market.cptSats)}</span></div>
              <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-3 text-sm text-slate-300">Collateral pool<br/><span class="text-slate-100">${formatSats(collateralPoolSats)}</span></div>
            </div>

            ${chartSkeleton(market)}

            <div class="mt-4 flex flex-wrap gap-2 text-sm">
              <button data-side="yes" class="rounded-full border px-4 py-2 ${state.selectedSide === "yes" ? "border-emerald-300 bg-emerald-300/20 text-emerald-200" : "border-slate-700 text-slate-300"}">Yes ${formatProbabilityWithPercent(market.yesPrice)}</button>
              <button data-side="no" class="rounded-full border px-4 py-2 ${state.selectedSide === "no" ? "border-rose-300 bg-rose-300/20 text-rose-200" : "border-slate-700 text-slate-300"}">No ${formatProbabilityWithPercent(noPrice)}</button>
            </div>
          </div>

          <div class="grid gap-3 lg:grid-cols-2">
            <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
              <p class="panel-subtitle">Oracle</p>
              <h3 class="panel-title mb-2 text-lg">Oracle Attestation</h3>
              <div class="space-y-1 text-xs text-slate-300">
                <div class="kv-row"><span>ORACLE_PUBLIC_KEY</span><span class="mono">${market.oraclePubkey}</span></div>
                <div class="kv-row"><span>MARKET_ID</span><span class="mono">${market.marketId}</span></div>
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
        </section>

        ${renderActionTicket(market)}
      </div>
    </div>
  `;
}

function render(): void {
  app.innerHTML = `
    <div class="min-h-screen text-slate-100">
      ${renderTopShell()}
      <main>${state.view === "home" ? renderHome() : renderDetail()}</main>
    </div>
  `;
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

function openMarket(marketId: string): void {
  const market = getMarketById(marketId);
  state.selectedMarketId = market.id;
  state.view = "detail";
  state.selectedSide = "yes";
  state.orderType = "limit";
  state.limitPrice = market.yesPrice;
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
setInterval(updateEstClockLabels, 1_000);

app.addEventListener("click", (event) => {
  const target = event.target as HTMLElement;
  const categoryEl = target.closest("[data-category]") as HTMLElement | null;
  const openMarketEl = target.closest(
    "[data-open-market]",
  ) as HTMLElement | null;
  const actionEl = target.closest("[data-action]") as HTMLElement | null;
  const sideEl = target.closest("[data-side]") as HTMLElement | null;
  const orderTypeEl = target.closest("[data-order-type]") as HTMLElement | null;
  const tabEl = target.closest("[data-tab]") as HTMLElement | null;

  const category = categoryEl?.getAttribute(
    "data-category",
  ) as NavCategory | null;
  const openMarketId = openMarketEl?.getAttribute("data-open-market") ?? null;
  const action = actionEl?.getAttribute("data-action") ?? null;
  const side = sideEl?.getAttribute("data-side") as Side | null;
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
    openMarket(openMarketId);
    return;
  }

  if (action === "go-home") {
    state.view = "home";
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
    state.limitPrice = side === "yes" ? market.yesPrice : 1 - market.yesPrice;
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
    action === "submit-cancel"
  ) {
    const market = getSelectedMarket();
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

  if (target.id === "trade-size") {
    state.tradeSizeBtc = Math.max(0.001, Number(target.value) || 0.001);
    render();
    return;
  }

  if (target.id === "limit-price") {
    state.limitPrice = Math.max(
      0.01,
      Math.min(0.99, Number(target.value) || 0.5),
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
  }
});
