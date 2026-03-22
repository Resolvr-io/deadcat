import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Market } from "../types.ts";

const listenerMap = new Map<
  string,
  Array<(event: { payload: unknown }) => void>
>();
const refreshMarketsFromStoreMock = vi.fn();
const fetchOrdersMock = vi.fn();
const mergeOrdersIntoMarketMock = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    (
      eventName: string,
      callback: (event: { payload: unknown }) => void,
    ): Promise<() => void> => {
      const callbacks = listenerMap.get(eventName) ?? [];
      callbacks.push(callback);
      listenerMap.set(eventName, callbacks);
      return Promise.resolve(() => {
        const current = listenerMap.get(eventName) ?? [];
        listenerMap.set(
          eventName,
          current.filter((candidate) => candidate !== callback),
        );
      });
    },
  ),
}));

vi.mock("../services/markets.ts", () => ({
  fetchOrders: fetchOrdersMock,
  mergeOrdersIntoMarket: mergeOrdersIntoMarketMock,
  refreshMarketsFromStore: refreshMarketsFromStoreMock,
}));

type SubscriptionsModule = typeof import("./subscriptions.ts");
type StateModule = typeof import("../state.ts");

let subscriptionsModule: SubscriptionsModule;
let stateModule: StateModule;

function mockMarket(): Market {
  return {
    id: "mkt-subscriptions-test",
    nevent: "nevent1subscriptions",
    question: "Should invalidations refresh orders?",
    category: "Bitcoin",
    description: "Subscription tests",
    resolutionSource: "Unit tests",
    isLive: true,
    state: 1,
    marketId: "ab".repeat(32),
    oraclePubkey: "11".repeat(32),
    expiryHeight: 999999,
    currentHeight: 1,
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

async function flushPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

function emit(eventName: string, payload: unknown = null): void {
  for (const callback of listenerMap.get(eventName) ?? []) {
    callback({ payload });
  }
}

beforeAll(async () => {
  (globalThis as { document?: unknown }).document = {
    querySelector: () => ({}),
  };
  stateModule = await import("../state.ts");
  subscriptionsModule = await import("./subscriptions.ts");
});

describe("setupTauriSubscriptions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenerMap.clear();
    const market = mockMarket();
    stateModule.setMarkets([market]);
    stateModule.state.view = "detail";
    stateModule.state.selectedMarketId = market.id;
    refreshMarketsFromStoreMock.mockResolvedValue(undefined);
    fetchOrdersMock.mockResolvedValue([{ id: "order-1" }]);
  });

  it("refreshes markets and selected-market orders on discovery invalidation", async () => {
    const render = vi.fn();
    const dispose = subscriptionsModule.setupTauriSubscriptions(render);
    await flushPromises();

    emit("discovery:orders-invalidated", { market_id: mockMarket().marketId });
    await flushPromises();

    expect(refreshMarketsFromStoreMock).toHaveBeenCalledTimes(1);
    expect(fetchOrdersMock).toHaveBeenCalledWith(mockMarket().marketId);
    expect(mergeOrdersIntoMarketMock).toHaveBeenCalledWith(
      mockMarket().marketId,
      [{ id: "order-1" }],
    );
    expect(render).toHaveBeenCalledTimes(2);

    dispose();
  });
});
