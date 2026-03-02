import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { Market } from "../types.ts";
import { reverseHex } from "../utils/crypto.ts";

type StateModule = typeof import("../state.ts");
type DetailModule = typeof import("./detail.ts");

let stateModule: StateModule;
let detailModule: DetailModule;

beforeAll(async () => {
  (globalThis as { document?: unknown }).document = {
    querySelector: () => ({}),
  };
  stateModule = await import("../state.ts");
  detailModule = await import("./detail.ts");
});

function mockMarket(): Market {
  return {
    id: "mkt-detail-test",
    nevent: "nevent1detailtest",
    question: "Will UI helper text be correct?",
    category: "Bitcoin",
    description: "Detail render tests",
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

function setWalletLotsForMarket(
  market: Market,
  yesLots: number,
  noLots = 0,
): void {
  stateModule.state.walletData = {
    balance: {
      [reverseHex(market.yesAssetId)]: yesLots,
      [reverseHex(market.noAssetId)]: noLots,
    },
    transactions: [],
    utxos: [],
    swaps: [],
    backupWords: [],
    backedUp: false,
    showBackup: false,
    backupPassword: "",
  };
}

describe("detail sell helper text", () => {
  beforeEach(() => {
    const market = mockMarket();
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.selectedSide = "yes";
    stateModule.state.tradeIntent = "close";
    stateModule.state.orderType = "market";
    stateModule.state.sizeMode = "contracts";
    stateModule.state.tradeContracts = 0;
    stateModule.state.tradeContractsDraft = "0";
    stateModule.state.walletData = null;
  });

  it("shows no-holdings helper when balance is zero and size is zero", () => {
    const market = mockMarket();
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain("No contracts available on this side.");
    expect(html).not.toContain("Enter contracts to sell.");
  });

  it("shows enter-size helper when balance exists and size is zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain("Enter contracts to sell.");
    expect(html).not.toContain("No contracts available on this side.");
  });

  it("shows no helper when balance exists and size is non-zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 2;
    stateModule.state.tradeContractsDraft = "2";
    const html = detailModule.renderActionTicket(market);

    expect(html).not.toContain("Enter contracts to sell.");
    expect(html).not.toContain("No contracts available on this side.");
  });

  it("uses draft input (empty) over committed contracts for helper + CTA state", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 3;
    stateModule.state.tradeContractsDraft = "";
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain("Enter contracts to sell.");
    expect(html).toMatch(/data-action="request-trade-quote"[^>]*disabled/);
  });

  it("uses draft input (zero) over committed contracts for helper + CTA state", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 4;
    stateModule.state.tradeContractsDraft = "0";
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain("Enter contracts to sell.");
    expect(html).toMatch(/data-action="request-trade-quote"[^>]*disabled/);
  });

  it("enables sell CTA when draft has lots even if committed value is zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 0;
    stateModule.state.tradeContractsDraft = "2";
    const html = detailModule.renderActionTicket(market);

    expect(html).not.toContain("Enter contracts to sell.");
    expect(html).not.toMatch(/data-action="request-trade-quote"[^>]*disabled/);
  });
});
