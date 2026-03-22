import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { Market, PriceHistoryEntry } from "../types.ts";

type StateModule = typeof import("../state.ts");
type ChartSeriesModule = typeof import("./chart-series.ts");

let stateModule: StateModule;
let chartSeriesModule: ChartSeriesModule;

function mockMarket(): Market {
  return {
    id: "mkt-chart-series-test",
    nevent: "nevent1chartseriestest",
    question: "Will the latest chart window render?",
    category: "Bitcoin",
    description: "Chart series tests",
    resolutionSource: "Unit tests",
    isLive: true,
    state: 1,
    marketId: "ab".repeat(32),
    oraclePubkey: "11".repeat(32),
    expiryHeight: 999999,
    currentHeight: 1_000,
    cptSats: 100,
    collateralAssetId: "22".repeat(32),
    yesAssetId: "33".repeat(32),
    noAssetId: "44".repeat(32),
    yesReissuanceToken: "55".repeat(32),
    noReissuanceToken: "66".repeat(32),
    anchor: null,
    creationTxid: "00".repeat(32),
    collateralUtxos: [],
    nostrEventJson: null,
    yesPrice: 0.5,
    change24h: 0,
    volumeBtc: 0,
    liquidityBtc: 0,
    limitOrders: [],
  };
}

function entry(
  blockHeight: number,
  impliedYesPriceBps: number,
): PriceHistoryEntry {
  return {
    pool_id: "pool-1",
    market_id: "ab".repeat(32),
    transition_txid: `tx-${blockHeight}`,
    old_s_index: 1,
    new_s_index: 2,
    reserve_yes: 100,
    reserve_no: 100,
    reserve_collateral: 100,
    implied_yes_price_bps: impliedYesPriceBps,
    block_height: blockHeight,
  };
}

describe("buildChartFromHistory", () => {
  beforeAll(async () => {
    (globalThis as { document?: unknown }).document = {
      querySelector: () => ({}),
    };
    stateModule = await import("../state.ts");
    chartSeriesModule = await import("./chart-series.ts");
  });

  beforeEach(() => {
    stateModule.state.chartTimescale = "10B";
  });

  it("renders the latest limited block window from ascending history", () => {
    const market = mockMarket();
    const history = [
      entry(995, 4_800),
      entry(996, 5_100),
      entry(998, 5_400),
      entry(1_000, 5_900),
    ];

    const series = chartSeriesModule.buildChartFromHistory(market, history);

    expect(series.startBlockHeight).toBe(990);
    expect(series.endBlockHeight).toBe(1_000);
    expect(series.xLabels).toEqual([990, 993, 997, 1_000]);
    expect(series.yesSeries.at(-1)).toBeCloseTo(0.59, 6);
    expect(
      chartSeriesModule.sampleChartProbabilityAtFraction(series, 0.7),
    ).toBeCloseTo(0.525, 6);
  });

  it("leaves the pre-history prefix empty and disables hover sampling there", () => {
    const market = mockMarket();
    const history = [entry(995, 4_800), entry(998, 5_400), entry(1_000, 5_900)];

    const series = chartSeriesModule.buildChartFromHistory(market, history);

    expect(series.yesSeries[0]).toBeNull();
    expect(
      chartSeriesModule.sampleChartProbabilityAtFraction(series, 0.2),
    ).toBeNull();
    expect(
      chartSeriesModule.sampleChartProbabilityAtFraction(series, 0.5),
    ).toBeCloseTo(0.48, 6);
  });

  it("renders a single confirmed history point as a real flat series", () => {
    const market = mockMarket();
    const history = [entry(998, 6_100)];

    const series = chartSeriesModule.buildChartFromHistory(market, history);

    expect(series.historyPoints).toHaveLength(1);
    expect(series.yesSeries.some((value) => value === null)).toBe(true);
    expect(
      chartSeriesModule.sampleChartProbabilityAtFraction(series, 0.7),
    ).toBeNull();
    expect(
      chartSeriesModule.sampleChartProbabilityAtFraction(series, 0.9),
    ).toBeCloseTo(0.61, 6);
    expect(
      chartSeriesModule.lastDefinedChartProbability(series, 0.5),
    ).toBeCloseTo(0.61, 6);
  });
});
