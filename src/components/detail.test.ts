import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { Market } from "../types.ts";
import { reverseHex } from "../utils/crypto.ts";

type StateModule = typeof import("../state.ts");
type DetailModule = typeof import("./detail.ts");

let stateModule: StateModule;
let detailModule: DetailModule;

function sampleAnchor() {
  return {
    creation_txid: "77".repeat(32),
    yes_dormant_opening: {
      asset_blinding_factor: "88".repeat(32),
      value_blinding_factor: "99".repeat(32),
    },
    no_dormant_opening: {
      asset_blinding_factor: "aa".repeat(32),
      value_blinding_factor: "bb".repeat(32),
    },
  };
}

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
    anchor: sampleAnchor(),
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

describe("detail sell ticket rendering", () => {
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

  it("shows zero holdings when balance is empty", () => {
    const market = mockMarket();
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain("You hold: YES 0.00 · NO 0.00");
    expect(html).toContain('value="0"');
  });

  it("shows the full sellable position when holdings exist and size is zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain("You hold: YES 5.00 · NO 0.00");
    expect(html).toContain(
      "Position remaining (if filled)</span><span>5.00 contracts",
    );
  });

  it("shows the remaining position when balance exists and size is non-zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 2;
    stateModule.state.tradeContractsDraft = "2";
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain(
      "Position remaining (if filled)</span><span>3.00 contracts",
    );
  });

  it("renders an empty draft input even when committed contracts are non-zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 3;
    stateModule.state.tradeContractsDraft = "";
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain('value=""');
    expect(html).toContain('data-action="submit-trade"');
  });

  it("renders a zero draft input even when committed contracts are non-zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 4;
    stateModule.state.tradeContractsDraft = "0";
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain('value="0"');
    expect(html).toContain('data-action="submit-trade"');
  });

  it("renders the draft contract value when committed size is zero", () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeContracts = 0;
    stateModule.state.tradeContractsDraft = "2";
    const html = detailModule.renderActionTicket(market);

    expect(html).toContain('value="2"');
    expect(html).toContain('data-action="submit-trade"');
  });

  it("shows refresh-state only when a canonical anchor is present", () => {
    const market = mockMarket();
    const htmlWithAnchor = detailModule.renderDetail();

    expect(htmlWithAnchor).toContain('data-action="refresh-market-state"');
    expect(htmlWithAnchor).toContain("Creation TX");

    market.anchor = null;
    stateModule.setMarkets([market]);
    const htmlWithoutAnchor = detailModule.renderDetail();

    expect(htmlWithoutAnchor).not.toContain(
      'data-action="refresh-market-state"',
    );
    expect(htmlWithoutAnchor).toContain("Creation TX");
  });
});
