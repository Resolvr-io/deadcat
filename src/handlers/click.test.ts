import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Market } from "../types.ts";

const invokeMock = vi.fn();
const openUrlMock = vi.fn();
const showToastMock = vi.fn();
const refreshWalletMock = vi.fn();
const issueTokensMock = vi.fn();
const quoteTradeMock = vi.fn();
const executeTradeMock = vi.fn();
const createLimitOrderMock = vi.fn();
const cancelLimitOrderMock = vi.fn();
const fetchOrdersMock = vi.fn();
const fetchOwnOrdersMock = vi.fn();
const createContractOnchainMock = vi.fn();
const resolveMarketMock = vi.fn();
const cancelTokensMock = vi.fn();
const redeemTokensMock = vi.fn();
const redeemExpiredTokensMock = vi.fn();
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
    createLimitOrder: createLimitOrderMock,
    cancelLimitOrder: cancelLimitOrderMock,
    fetchOrders: fetchOrdersMock,
    fetchOwnOrders: fetchOwnOrdersMock,
    createContractOnchain: createContractOnchainMock,
    issueTokens: issueTokensMock,
    quoteTrade: quoteTradeMock,
    executeTrade: executeTradeMock,
    resolveMarket: resolveMarketMock,
    cancelTokens: cancelTokensMock,
    redeemTokens: redeemTokensMock,
    redeemExpiredTokens: redeemExpiredTokensMock,
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

function actionEventWithOrder(action: string, orderId: string): MouseEvent {
  const actionEl = {
    getAttribute(name: string) {
      if (name === "data-action") return action;
      if (name === "data-order-id") return orderId;
      return null;
    },
    dataset: {
      action,
      orderId,
    },
  } as unknown as HTMLElement;
  const target = {
    closest(selector: string) {
      return selector === "[data-action]" ? actionEl : null;
    },
  } as unknown as HTMLElement;
  return { target } as MouseEvent;
}

function reverseHexBytes(hex: string): string {
  return (
    hex
      .match(/.{1,2}/g)
      ?.reverse()
      .join("") ?? hex
  );
}

async function flushAsyncWork(): Promise<void> {
  for (let i = 0; i < 4; i += 1) {
    await Promise.resolve();
  }
}

function sampleFee() {
  return {
    policy: {
      kind: "confirmation_target_blocks" as const,
      blocks: 6,
    },
    amountSat: 321,
    rateSatPerVb: 3.21,
    discountVsize: 100,
  };
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
    refreshWalletMock.mockResolvedValue(undefined);
    fetchOrdersMock.mockResolvedValue([]);
    fetchOwnOrdersMock.mockResolvedValue([]);
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

  it("shows resolution fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor() };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.lastAttestationSig = "cc".repeat(32);
    stateModule.state.lastAttestationOutcome = true;
    resolveMarketMock.mockResolvedValue({
      txid: "aa".repeat(32),
      previous_state: 1,
      new_state: 2,
      outcome_yes: true,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("execute-resolution"), deps);
    await flushAsyncWork();

    expect(resolveMarketMock).toHaveBeenCalledWith(
      market,
      market.anchor,
      true,
      "cc".repeat(32),
      expect.any(Object),
    );
    expect(showToastMock).toHaveBeenCalledWith(
      "Resolution executed! txid: aaaaaaaaaaaaaaaa... State: 2 · fee: 321 sats",
      "success",
    );
  });

  it("shows cancellation fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor() };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.pairsInput = 2;
    cancelTokensMock.mockResolvedValue({
      txid: "bb".repeat(32),
      previous_state: 1,
      new_state: 1,
      pairs_burned: 2,
      is_full_cancellation: false,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-cancel"), deps);
    await flushAsyncWork();

    expect(cancelTokensMock).toHaveBeenCalledWith(
      market,
      market.anchor,
      2,
      expect.any(Object),
    );
    expect(showToastMock).toHaveBeenCalledWith(
      "Tokens cancelled! txid: bbbbbbbbbbbbbbbb... (partial) · fee: 321 sats",
      "success",
    );
  });

  it("shows redemption fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor(), state: 2 };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.tokensInput = 3;
    redeemTokensMock.mockResolvedValue({
      txid: "cc".repeat(32),
      previous_state: 2,
      tokens_redeemed: 3,
      payout_sats: 12_345,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-redeem"), deps);
    await flushAsyncWork();

    expect(redeemTokensMock).toHaveBeenCalledWith(
      market,
      market.anchor,
      3,
      expect.any(Object),
    );
    expect(showToastMock).toHaveBeenCalledWith(
      "Redeemed! txid: cccccccccccccccc... payout: 12,345 sats · fee: 321 sats",
      "success",
    );
  });

  it("shows expiry redemption fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor(), state: 4 };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.tokensInput = 3;
    stateModule.state.walletData = {
      balance: { [reverseHexBytes(market.yesAssetId)]: 9 },
      transactions: [],
      utxos: [],
      swaps: [],
      backupWords: [],
      backedUp: false,
      showBackup: false,
      backupPassword: "",
    };
    redeemExpiredTokensMock.mockResolvedValue({
      txid: "dd".repeat(32),
      previous_state: 4,
      tokens_redeemed: 3,
      payout_sats: 7_500,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-redeem"), deps);
    await flushAsyncWork();

    expect(redeemExpiredTokensMock).toHaveBeenCalledWith(
      market,
      market.anchor,
      market.yesAssetId,
      3,
      expect.any(Object),
    );
    expect(showToastMock).toHaveBeenCalledWith(
      "Expired tokens redeemed! txid: dddddddddddddddd... payout: 7,500 sats · fee: 321 sats",
      "success",
    );
  });

  it("shows issuance fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor() };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.pairsInput = 2;
    issueTokensMock.mockResolvedValue({
      txid: "ee".repeat(32),
      previous_state: 0,
      new_state: 1,
      pairs_issued: 2,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-issue"), deps);
    await flushAsyncWork();

    expect(issueTokensMock).toHaveBeenCalledWith(market, 2);
    expect(showToastMock).toHaveBeenCalledWith(
      "Tokens issued! txid: eeeeeeeeeeeeeeee... · fee: 321 sats",
      "success",
    );
  });

  it("shows trade execution fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor() };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.tradeIntent = "open";
    stateModule.state.orderType = "market";
    stateModule.state.selectedSide = "yes";
    stateModule.state.tradeSizeSats = 10_000;
    stateModule.state.tradeSizeSatsDraft = "10,000";
    quoteTradeMock.mockResolvedValue({
      total_input: 10_000,
      total_output: 95,
      effective_price: 105.26,
      legs: [],
    });
    executeTradeMock.mockResolvedValue({
      txid: "ff".repeat(32),
      total_input: 10_000,
      total_output: 95,
      num_orders_filled: 1,
      pool_used: true,
      new_reserves: null,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-trade"), deps);
    await flushAsyncWork();

    expect(executeTradeMock).toHaveBeenCalledWith(
      market,
      "yes",
      "buy",
      10_000,
      expect.any(Object),
      expect.any(Object),
    );
    expect(showToastMock).toHaveBeenCalledWith(
      "Trade executed! txid: ffffffffffffffff... · fee: 321 sats",
      "success",
    );
  });

  it("shows limit-order placement fee in the success toast", async () => {
    const market = { ...mockMarket(), anchor: sampleAnchor() };
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.orderType = "limit";
    stateModule.state.tradeIntent = "open";
    stateModule.state.selectedSide = "yes";
    stateModule.state.tradeSizeSats = 10_000;
    stateModule.state.tradeSizeSatsDraft = "10,000";
    stateModule.state.limitPrice = 0.55;
    createLimitOrderMock.mockResolvedValue({
      txid: "ab".repeat(32),
      nostr_event_id: "event-1",
      covenant_address: "ert1qexample",
      order_amount: 10_000,
      order_index: 0,
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-trade"), deps);
    await flushAsyncWork();

    expect(createLimitOrderMock).toHaveBeenCalledWith(
      market,
      "yes",
      "buy",
      55,
      10_000,
    );
    expect(showToastMock).toHaveBeenCalledWith(
      "Limit order placed! txid: abababababababab... · fee: 321 sats",
      "success",
    );
  });

  it("shows limit-order cancellation fee in the success toast", async () => {
    const order = {
      id: "order-1",
      market_id: "market-for-order",
      base_asset_id: "01".repeat(32),
      quote_asset_id: "02".repeat(32),
      price: 100,
      min_fill_lots: 1,
      min_remainder_lots: 1,
      direction: "sell-base" as const,
      direction_label: "buy",
      maker_base_pubkey: "03".repeat(32),
      order_nonce: "04".repeat(32),
      covenant_address: "ert1qexample",
      offered_amount: 5,
      cosigner_pubkey: "05".repeat(32),
      maker_receive_spk_hash: "06".repeat(32),
      creator_pubkey: "07".repeat(32),
      created_at: 0,
    };
    const market = {
      ...mockMarket(),
      marketId: "market-for-order",
      limitOrders: [order],
    };
    stateModule.setMarkets([market]);
    cancelLimitOrderMock.mockResolvedValue({
      txid: "cd".repeat(32),
      refunded_amount: 2_500,
      fee: sampleFee(),
    });

    await clickModule.handleClick(
      actionEventWithOrder("cancel-limit-order", "order-1"),
      deps,
    );
    await flushAsyncWork();

    expect(cancelLimitOrderMock).toHaveBeenCalledWith(order);
    expect(showToastMock).toHaveBeenCalledWith(
      "Order cancelled! Refunded 2500 sats. txid: cdcdcdcdcdcdcdcd... · fee: 321 sats",
      "success",
    );
  });

  it("shows contract creation fee in the success toast", async () => {
    const market = {
      ...mockMarket(),
      anchor: sampleAnchor(),
      question: "New market",
    };
    stateModule.state.createQuestion = market.question;
    stateModule.state.createDescription = market.description;
    stateModule.state.createResolutionSource = market.resolutionSource;
    stateModule.state.createSettlementInput = "2030-01-01T12:00";
    createContractOnchainMock.mockResolvedValue({
      market: {
        id: market.id,
        nevent: market.nevent,
        market_id: market.marketId,
        question: market.question,
        category: market.category,
        description: market.description,
        resolution_source: market.resolutionSource,
        oracle_pubkey: market.oraclePubkey,
        expiry_height: market.expiryHeight,
        cpt_sats: market.cptSats,
        collateral_asset_id: market.collateralAssetId,
        yes_asset_id: market.yesAssetId,
        no_asset_id: market.noAssetId,
        yes_reissuance_token: market.yesReissuanceToken,
        no_reissuance_token: market.noReissuanceToken,
        creator_pubkey: "12".repeat(32),
        created_at: 1,
        anchor: market.anchor,
        state: market.state,
      },
      fee: sampleFee(),
    });

    await clickModule.handleClick(actionEvent("submit-create-market"), deps);
    await flushAsyncWork();

    expect(createContractOnchainMock).toHaveBeenCalledWith(
      expect.objectContaining({
        question: market.question,
        description: market.description,
        resolution_source: market.resolutionSource,
        collateral_per_token: 5000,
      }),
    );
    expect(showToastMock).toHaveBeenCalledWith(
      `Market created! txid: ${market.anchor?.creation_txid ?? "unknown"} · fee: 321 sats`,
      "success",
    );
    expect(
      stateModule.markets.some((m) => m.marketId === market.marketId),
    ).toBe(true);
  });
});
