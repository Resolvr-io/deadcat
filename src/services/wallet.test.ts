import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Market, PaymentSwap, PriceHistoryEntry } from "../types.ts";

const syncWalletMock = vi.fn();
const listPaymentSwapsMock = vi.fn();
const getPriceHistoryMock = vi.fn();
const showOverlayLoaderMock = vi.fn();
const hideOverlayLoaderMock = vi.fn();

vi.mock("../api/tauri.ts", () => ({
  tauriApi: {
    getAppState: vi.fn(),
    getWalletBalance: vi.fn(),
    getWalletTransactions: vi.fn(),
    listPaymentSwaps: listPaymentSwapsMock,
    restoreWallet: vi.fn(),
    unlockWallet: vi.fn(),
    syncWallet: syncWalletMock,
    fetchChainTip: vi.fn(),
  },
}));

vi.mock("./pools.ts", () => ({
  getPriceHistory: getPriceHistoryMock,
}));

vi.mock("../ui/loader", () => ({
  showOverlayLoader: showOverlayLoaderMock,
  hideOverlayLoader: hideOverlayLoaderMock,
}));

type StateModule = typeof import("../state.ts");
type WalletModule = typeof import("./wallet.ts");

let stateModule: StateModule;
let walletModule: WalletModule;

function mockMarket(): Market {
  return {
    id: "mkt-wallet-history-test",
    nevent: "nevent1wallethistorytest",
    question: "Will wallet sync refresh chart history?",
    category: "Bitcoin",
    description: "Wallet service tests",
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

function historyEntry(
  blockHeight: number,
  impliedYesPriceBps: number,
): PriceHistoryEntry {
  return {
    pool_id: "pool-wallet-test",
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

function sampleSwap(): PaymentSwap {
  return {
    id: "swap-1",
    flow: "lightning_to_liquid",
    network: "liquid-testnet",
    status: "pending",
    invoiceAmountSat: 1_000,
    expectedAmountSat: 900,
    lockupAddress: null,
    invoice: null,
    invoiceExpiresAt: null,
    lockupTxid: null,
    createdAt: "2026-03-21T00:00:00Z",
    updatedAt: "2026-03-21T00:00:00Z",
  };
}

beforeAll(async () => {
  (globalThis as { btoa?: (value: string) => string }).btoa = () =>
    "mock-base64";
  (globalThis as { document?: unknown }).document = {
    querySelector: () => ({}),
  };
  stateModule = await import("../state.ts");
  walletModule = await import("./wallet.ts");
});

describe("refreshWallet", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stateModule.setMarkets([]);
    stateModule.state.selectedMarketId = "";
    stateModule.state.priceHistory = new Map();
    stateModule.state.walletData = stateModule.createWalletData();
    stateModule.state.walletError = "";
    stateModule.state.walletLoading = false;
    syncWalletMock.mockResolvedValue(undefined);
    listPaymentSwapsMock.mockResolvedValue([sampleSwap()]);
  });

  it("refreshes the selected market history after a successful sync", async () => {
    const market = mockMarket();
    const updatedHistory = [historyEntry(1_000, 5_900)];
    const render = vi.fn();

    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.priceHistory.set(market.marketId, [
      historyEntry(995, 4_800),
    ]);
    getPriceHistoryMock.mockResolvedValue(updatedHistory);

    await walletModule.refreshWallet(render);

    expect(syncWalletMock).toHaveBeenCalledTimes(1);
    expect(listPaymentSwapsMock).toHaveBeenCalledTimes(1);
    expect(getPriceHistoryMock).toHaveBeenCalledWith(market.marketId, 500);
    expect(stateModule.state.walletData?.swaps).toEqual([sampleSwap()]);
    expect(stateModule.state.priceHistory.get(market.marketId)).toEqual(
      updatedHistory,
    );
    expect(stateModule.state.walletError).toBe("");
  });

  it("keeps cached history when the post-sync refresh fails", async () => {
    const market = mockMarket();
    const cachedHistory = [historyEntry(995, 4_800)];
    const render = vi.fn();
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.priceHistory.set(market.marketId, cachedHistory);
    getPriceHistoryMock.mockRejectedValue(new Error("history refresh failed"));

    await walletModule.refreshWallet(render);

    expect(syncWalletMock).toHaveBeenCalledTimes(1);
    expect(getPriceHistoryMock).toHaveBeenCalledWith(market.marketId, 500);
    expect(stateModule.state.priceHistory.get(market.marketId)).toEqual(
      cachedHistory,
    );
    expect(stateModule.state.walletError).toBe("");
    warnSpy.mockRestore();
  });
});
