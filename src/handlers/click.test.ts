import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Market } from "../types.ts";

const invokeMock = vi.fn();
const openUrlMock = vi.fn();
const showToastMock = vi.fn();
const refreshWalletMock = vi.fn();
const issueTokensMock = vi.fn();
const quoteTradeMock = vi.fn();
const executeTradeMock = vi.fn();
const confirmMock = vi.fn(() => true);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: openUrlMock,
}));

vi.mock("../ui/toast.ts", () => ({
  showToast: showToastMock,
}));

vi.mock("../services/wallet.ts", () => ({
  fetchWalletStatus: vi.fn(),
  generateQr: vi.fn(),
  refreshWallet: refreshWalletMock,
  resetReceiveState: vi.fn(),
  resetSendState: vi.fn(),
}));

vi.mock("../services/markets.ts", async () => {
  const actual = await vi.importActual<typeof import("../services/markets.ts")>(
    "../services/markets.ts",
  );
  return {
    ...actual,
    issueTokens: issueTokensMock,
    quoteTrade: quoteTradeMock,
    executeTrade: executeTradeMock,
  };
});

type ClickModule = typeof import("./click.ts");
type StateModule = typeof import("../state.ts");

let clickModule: ClickModule;
let stateModule: StateModule;

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

function mockMarket(): Market {
  return {
    id: "mkt-click-test",
    nevent: "nevent1clicktest",
    question: "Will click guards short-circuit?",
    category: "Bitcoin",
    description: "Click handler tests",
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

function actionEvent(action: string): MouseEvent {
  const actionEl = {
    getAttribute(name: string) {
      return name === "data-action" ? action : null;
    },
  } as unknown as HTMLElement;
  const target = {
    closest(selector: string) {
      return selector === "[data-action]" ? actionEl : null;
    },
  } as unknown as HTMLElement;
  return { target } as MouseEvent;
}

const deps = {
  render: vi.fn(),
  openMarket: vi.fn(),
  finishOnboarding: vi.fn(),
};

beforeAll(async () => {
  (globalThis as { document?: unknown }).document = {
    querySelector: () => ({}),
  };
  (globalThis as { window?: unknown }).window = {
    confirm: confirmMock,
    alert: vi.fn(),
  };
  stateModule = await import("../state.ts");
  clickModule = await import("./click.ts");
});

describe("handleClick anchor guards", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const market = mockMarket();
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.pairsInput = 2;
    stateModule.state.tokensInput = 3;
    stateModule.state.lastAttestationSig = null;
    stateModule.state.lastAttestationOutcome = null;
    stateModule.state.lastAttestationMarketId = null;
    stateModule.state.walletData = null;
  });

  it("blocks execute-resolution when anchor is missing", async () => {
    stateModule.state.lastAttestationSig = "cc".repeat(32);
    stateModule.state.lastAttestationOutcome = true;

    await clickModule.handleClick(actionEvent("execute-resolution"), deps);

    expect(showToastMock).toHaveBeenCalledWith(
      "Market has no canonical anchor — cannot resolve market",
      "error",
    );
    expect(confirmMock).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("blocks refresh-market-state when anchor is missing even if creation txid exists", async () => {
    await clickModule.handleClick(actionEvent("refresh-market-state"), deps);

    expect(showToastMock).toHaveBeenCalledWith(
      "Market has no canonical anchor — cannot query market state",
      "error",
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("blocks submit-cancel when anchor is missing", async () => {
    await clickModule.handleClick(actionEvent("submit-cancel"), deps);

    expect(showToastMock).toHaveBeenCalledWith(
      "Market has no canonical anchor — cannot cancel tokens",
      "error",
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("blocks submit-redeem redemption when anchor is missing", async () => {
    stateModule.state.selectedMarketId = "mkt-click-test";
    stateModule.setMarkets([{ ...mockMarket(), state: 2 }]);

    await clickModule.handleClick(actionEvent("submit-redeem"), deps);

    expect(showToastMock).toHaveBeenCalledWith(
      "Market has no canonical anchor — cannot redeem tokens",
      "error",
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("blocks submit-redeem expiry redemption when anchor is missing", async () => {
    stateModule.state.selectedMarketId = "mkt-click-test";
    stateModule.setMarkets([{ ...mockMarket(), state: 4 }]);

    await clickModule.handleClick(actionEvent("submit-redeem"), deps);

    expect(showToastMock).toHaveBeenCalledWith(
      "Market has no canonical anchor — cannot redeem expired tokens",
      "error",
    );
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("allows refresh-market-state when an anchor is present", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor() };
    stateModule.setMarkets([market]);
    invokeMock.mockResolvedValue({ state: 1 });

    await clickModule.handleClick(actionEvent("refresh-market-state"), deps);

    expect(invokeMock).toHaveBeenCalledWith("get_market_state", {
      contractParamsJson: expect.any(String),
      anchor: market.anchor,
    });
  });
});
