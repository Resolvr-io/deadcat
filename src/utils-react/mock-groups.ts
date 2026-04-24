import type { FullOrderbook, MarketGroup } from "../types";

// ── Seeded RNG (same as mock-orderbook.ts) ───────────────────────────
function hashCode(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}
function seededRand(seed: number): number {
  const x = Math.sin(seed + 1) * 10000;
  return x - Math.floor(x);
}

/**
 * Generate a deterministic mock orderbook for a multi-outcome outcome.
 * Works without a full Market object — just needs the outcome id, current
 * probability, and the collateral-per-token value.
 */
export function generateMockOutcomeOrderbook(
  outcomeId: string,
  yesPrice: number,
  cptSats: number,
): FullOrderbook {
  const seed = hashCode(outcomeId);
  const fc = cptSats * 2; // full contract sats
  const midPriceSats = Math.round(yesPrice * fc);

  const spreadFraction = 0.01 + seededRand(seed) * 0.03;
  const halfSpread = Math.max(1, Math.round((spreadFraction * fc) / 2));

  const bestAsk = Math.min(fc - 1, midPriceSats + halfSpread);
  const bestBid = Math.max(1, midPriceSats - halfSpread);

  const askLevelCount = 4 + Math.floor(seededRand(seed + 1) * 4);
  const bidLevelCount = 4 + Math.floor(seededRand(seed + 2) * 4);
  const askTick = 1 + Math.floor(seededRand(seed + 3) * 3);
  const bidTick = 1 + Math.floor(seededRand(seed + 4) * 3);
  const maxDepth = 50 + Math.floor(seededRand(seed + 5) * 250);

  const asks = Array.from({ length: askLevelCount }, (_, i) => ({
    priceSats: Math.min(fc - 1, bestAsk + i * askTick),
    contracts: Math.round(
      (0.3 + (i / (askLevelCount - 1)) * 0.7) *
        maxDepth *
        (0.7 + seededRand(seed + 10 + i) * 0.6),
    ),
  })).filter((l) => l.contracts > 0);

  const bids = Array.from({ length: bidLevelCount }, (_, i) => ({
    priceSats: Math.max(1, bestBid - i * bidTick),
    contracts: Math.round(
      (0.3 + (i / (bidLevelCount - 1)) * 0.7) *
        maxDepth *
        (0.7 + seededRand(seed + 20 + i) * 0.6),
    ),
  })).filter((l) => l.contracts > 0);

  asks.sort((a, b) => a.priceSats - b.priceSats);
  bids.sort((a, b) => b.priceSats - a.priceSats);

  return {
    asks,
    bids,
    spread:
      asks.length > 0 && bids.length > 0
        ? asks[0].priceSats - bids[0].priceSats
        : null,
  };
}

// Mock current block height ~852,000 (late April 2026).
// US Open starts June 12 → ~7,200 blocks away → expiry 859,200
// NBA Finals start ~June 5 → ~5,760 blocks away → expiry 857,760

export function getMockMarketGroups(): MarketGroup[] {
  return [F1_CHAMPIONSHIP_2026, US_OPEN_2026, NBA_FINALS_2026];
}

const F1_CHAMPIONSHIP_2026: MarketGroup = {
  id: "group-f1-wdc-2026",
  title: "2026 Formula 1 Drivers' Championship",
  description:
    "Which driver will win the 2026 FIA Formula 1 World Drivers' Championship? The season runs March–November 2026 across 24 rounds. Each outcome is a binary contract: YES pays 2× collateral if the named driver wins the title, NO pays 2× if they don't.",
  category: "Sports",
  resolutionSource: "FIA official results · formula1.com",
  expiryHeight: 907_200,
  currentHeight: 852_000,
  totalVolumeBtc: 11.43,
  traderCount: 5_872,
  createdAt: Date.now() - 45 * 24 * 60 * 60 * 1000,
  state: "active",
  cptSats: 10_000,
  nevent: "nevent1mock-f1-wdc-2026",
  nostrEventJson: '{"kind":30050,"id":"mock-f1-wdc-2026"}',
  creationTxid: "mocktxid-f1-wdc-2026",
  creatorPubkey:
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
  outcomes: [
    {
      id: "ou-verstappen",
      name: "Max Verstappen",
      yesPrice: 0.32,
      change24h: -0.021,
      volumeBtc: 3.41,
      traderCount: 1_482,
    },
    {
      id: "ou-norris",
      name: "Lando Norris",
      yesPrice: 0.2,
      change24h: 0.018,
      volumeBtc: 2.17,
      traderCount: 943,
    },
    {
      id: "ou-leclerc",
      name: "Charles Leclerc",
      yesPrice: 0.13,
      change24h: 0.007,
      volumeBtc: 1.38,
      traderCount: 612,
    },
    {
      id: "ou-hamilton-f1",
      name: "Lewis Hamilton",
      yesPrice: 0.09,
      change24h: -0.004,
      volumeBtc: 0.97,
      traderCount: 441,
    },
    {
      id: "ou-russell",
      name: "George Russell",
      yesPrice: 0.07,
      change24h: 0.003,
      volumeBtc: 0.74,
      traderCount: 327,
    },
    {
      id: "ou-piastri",
      name: "Oscar Piastri",
      yesPrice: 0.06,
      change24h: 0.011,
      volumeBtc: 0.63,
      traderCount: 281,
    },
    {
      id: "ou-sainz",
      name: "Carlos Sainz",
      yesPrice: 0.04,
      change24h: -0.002,
      volumeBtc: 0.41,
      traderCount: 187,
    },
    {
      id: "ou-alonso",
      name: "Fernando Alonso",
      yesPrice: 0.02,
      change24h: 0.001,
      volumeBtc: 0.22,
      traderCount: 98,
    },
    {
      id: "ou-tsunoda",
      name: "Yuki Tsunoda",
      yesPrice: 0.01,
      change24h: 0.004,
      volumeBtc: 0.11,
      traderCount: 54,
    },
    {
      id: "ou-albon",
      name: "Alex Albon",
      yesPrice: 0.01,
      change24h: 0.0,
      volumeBtc: 0.09,
      traderCount: 43,
    },
    {
      id: "ou-hulkenberg",
      name: "Nico Hülkenberg",
      yesPrice: 0.01,
      change24h: 0.001,
      volumeBtc: 0.08,
      traderCount: 39,
    },
    {
      id: "ou-bearman",
      name: "Oliver Bearman",
      yesPrice: 0.01,
      change24h: -0.001,
      volumeBtc: 0.07,
      traderCount: 35,
    },
    {
      id: "ou-stroll",
      name: "Lance Stroll",
      yesPrice: 0.01,
      change24h: 0.0,
      volumeBtc: 0.06,
      traderCount: 29,
    },
    {
      id: "ou-gasly",
      name: "Pierre Gasly",
      yesPrice: 0.01,
      change24h: 0.002,
      volumeBtc: 0.06,
      traderCount: 27,
    },
    {
      id: "ou-hadjar",
      name: "Isack Hadjar",
      yesPrice: 0.01,
      change24h: 0.003,
      volumeBtc: 0.05,
      traderCount: 24,
    },
    {
      id: "ou-doohan",
      name: "Jack Doohan",
      yesPrice: 0.01,
      change24h: 0.0,
      volumeBtc: 0.04,
      traderCount: 21,
    },
    {
      id: "ou-ocon",
      name: "Esteban Ocon",
      yesPrice: 0.01,
      change24h: -0.001,
      volumeBtc: 0.04,
      traderCount: 19,
    },
    {
      id: "ou-bottas",
      name: "Valtteri Bottas",
      yesPrice: 0.01,
      change24h: 0.0,
      volumeBtc: 0.03,
      traderCount: 15,
    },
    {
      id: "ou-colapinto",
      name: "Franco Colapinto",
      yesPrice: 0.01,
      change24h: 0.005,
      volumeBtc: 0.03,
      traderCount: 14,
    },
    {
      id: "ou-zhou",
      name: "Guanyu Zhou",
      yesPrice: 0.01,
      change24h: 0.0,
      volumeBtc: 0.02,
      traderCount: 11,
    },
  ],
};

const US_OPEN_2026: MarketGroup = {
  id: "group-us-open-2026",
  title: "2026 US Open Golf Winner",
  description:
    "Which golfer will win the 2026 US Open Championship at Oakmont Country Club (June 12–15)?  Each outcome is a binary contract: YES pays 2× collateral if the named player wins, NO pays 2× if they don't.",
  category: "Sports",
  resolutionSource: "PGA TOUR official results · usopen.com",
  expiryHeight: 859_200,
  currentHeight: 852_000,
  totalVolumeBtc: 4.812,
  traderCount: 2_341,
  createdAt: Date.now() - 12 * 24 * 60 * 60 * 1000,
  state: "active",
  cptSats: 10_000,
  nevent: "nevent1mock-us-open-2026",
  nostrEventJson: '{"kind":30050,"id":"mock-us-open-2026"}',
  creationTxid: "mocktxid-us-open-2026",
  creatorPubkey:
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
  outcomes: [
    {
      id: "ou-scottie",
      name: "Scottie Scheffler",
      yesPrice: 0.21,
      change24h: 0.012,
      volumeBtc: 0.94,
      traderCount: 412,
    },
    {
      id: "ou-rory",
      name: "Rory McIlroy",
      yesPrice: 0.15,
      change24h: 0.031,
      volumeBtc: 0.81,
      traderCount: 387,
    },
    {
      id: "ou-xander",
      name: "Xander Schauffele",
      yesPrice: 0.11,
      change24h: -0.008,
      volumeBtc: 0.52,
      traderCount: 261,
    },
    {
      id: "ou-collin",
      name: "Collin Morikawa",
      yesPrice: 0.09,
      change24h: 0.005,
      volumeBtc: 0.43,
      traderCount: 198,
    },
    {
      id: "ou-ludvig",
      name: "Ludvig Åberg",
      yesPrice: 0.08,
      change24h: -0.014,
      volumeBtc: 0.38,
      traderCount: 174,
    },
    {
      id: "ou-viktor",
      name: "Viktor Hovland",
      yesPrice: 0.07,
      change24h: 0.002,
      volumeBtc: 0.31,
      traderCount: 143,
    },
    {
      id: "ou-bryson",
      name: "Bryson DeChambeau",
      yesPrice: 0.06,
      change24h: -0.003,
      volumeBtc: 0.24,
      traderCount: 118,
    },
    {
      id: "ou-cantlay",
      name: "Patrick Cantlay",
      yesPrice: 0.05,
      change24h: 0.001,
      volumeBtc: 0.19,
      traderCount: 92,
    },
    {
      id: "ou-tommy",
      name: "Tommy Fleetwood",
      yesPrice: 0.04,
      change24h: 0.009,
      volumeBtc: 0.15,
      traderCount: 76,
    },
    {
      id: "ou-lowry",
      name: "Shane Lowry",
      yesPrice: 0.04,
      change24h: -0.002,
      volumeBtc: 0.13,
      traderCount: 64,
    },
    {
      id: "ou-homa",
      name: "Max Homa",
      yesPrice: 0.03,
      change24h: 0.004,
      volumeBtc: 0.09,
      traderCount: 47,
    },
    {
      id: "ou-clark",
      name: "Wyndham Clark",
      yesPrice: 0.03,
      change24h: -0.001,
      volumeBtc: 0.08,
      traderCount: 43,
    },
    {
      id: "ou-tony",
      name: "Tony Finau",
      yesPrice: 0.02,
      change24h: 0.0,
      volumeBtc: 0.06,
      traderCount: 31,
    },
    {
      id: "ou-field-golf",
      name: "Field (any other player)",
      yesPrice: 0.02,
      change24h: -0.005,
      volumeBtc: 0.05,
      traderCount: 28,
    },
  ],
};

const NBA_FINALS_2026: MarketGroup = {
  id: "group-nba-finals-2026",
  title: "2026 NBA Championship Winner",
  description:
    "Which team will win the 2026 NBA Championship? The Finals are expected to begin in early June 2026. Each outcome is a binary contract: YES pays 2× collateral if the named team wins, NO pays 2× if they don't.",
  category: "Sports",
  resolutionSource: "NBA.com official results",
  expiryHeight: 857_760,
  currentHeight: 852_000,
  totalVolumeBtc: 6.237,
  traderCount: 3_108,
  createdAt: Date.now() - 28 * 24 * 60 * 60 * 1000,
  state: "active",
  cptSats: 10_000,
  creatorPubkey:
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
  outcomes: [
    {
      id: "ou-celtics",
      name: "Boston Celtics",
      yesPrice: 0.24,
      change24h: -0.018,
      volumeBtc: 1.42,
      traderCount: 681,
    },
    {
      id: "ou-thunder",
      name: "Oklahoma City Thunder",
      yesPrice: 0.21,
      change24h: 0.024,
      volumeBtc: 1.28,
      traderCount: 592,
    },
    {
      id: "ou-cavs",
      name: "Cleveland Cavaliers",
      yesPrice: 0.16,
      change24h: 0.011,
      volumeBtc: 0.87,
      traderCount: 401,
    },
    {
      id: "ou-warriors",
      name: "Golden State Warriors",
      yesPrice: 0.1,
      change24h: -0.007,
      volumeBtc: 0.61,
      traderCount: 298,
    },
    {
      id: "ou-nuggets",
      name: "Denver Nuggets",
      yesPrice: 0.08,
      change24h: 0.003,
      volumeBtc: 0.47,
      traderCount: 221,
    },
    {
      id: "ou-lakers",
      name: "Los Angeles Lakers",
      yesPrice: 0.07,
      change24h: -0.012,
      volumeBtc: 0.43,
      traderCount: 209,
    },
    {
      id: "ou-pacers",
      name: "Indiana Pacers",
      yesPrice: 0.05,
      change24h: 0.006,
      volumeBtc: 0.31,
      traderCount: 148,
    },
    {
      id: "ou-heat",
      name: "Miami Heat",
      yesPrice: 0.04,
      change24h: 0.002,
      volumeBtc: 0.24,
      traderCount: 117,
    },
    {
      id: "ou-knicks",
      name: "New York Knicks",
      yesPrice: 0.03,
      change24h: -0.004,
      volumeBtc: 0.19,
      traderCount: 94,
    },
    {
      id: "ou-bucks",
      name: "Milwaukee Bucks",
      yesPrice: 0.02,
      change24h: 0.001,
      volumeBtc: 0.12,
      traderCount: 58,
    },
  ],
};
