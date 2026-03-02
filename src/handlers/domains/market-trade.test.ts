import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { reverseHex } from "../../utils/crypto.ts";
import type { ClickDomainContext } from "./context.ts";

type StateModule = typeof import("../../state.ts");
type MarketModule = typeof import("./market.ts");
type MarketUtilsModule = typeof import("../../utils/market.ts");
type TauriApiModule = typeof import("../../api/tauri.ts");
type KeydownModule = typeof import("../keydown.ts");
type ToastModule = typeof import("../../ui/toast.ts");

let stateModule: StateModule;
let marketModule: MarketModule;
let marketUtilsModule: MarketUtilsModule;
let tauriApiModule: TauriApiModule;
let keydownModule: KeydownModule;
let toastModule: ToastModule;

beforeAll(async () => {
  (globalThis as { document?: unknown }).document = {
    querySelector: () => ({}),
    createElement: () => ({
      className: "",
      style: {},
      textContent: "",
      remove: () => {},
    }),
    body: {
      appendChild: () => {},
    },
  };
  (globalThis as { requestAnimationFrame?: unknown }).requestAnimationFrame = (
    cb: FrameRequestCallback,
  ) => {
    cb(0);
    return 0;
  };
  stateModule = await import("../../state.ts");
  marketModule = await import("./market.ts");
  marketUtilsModule = await import("../../utils/market.ts");
  tauriApiModule = await import("../../api/tauri.ts");
  keydownModule = await import("../keydown.ts");
  toastModule = await import("../../ui/toast.ts");
});

function mockMarket() {
  return {
    id: "mkt-test",
    nevent: "nevent1test",
    question: "Will test pass?",
    category: "Bitcoin",
    description: "Test market",
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
  } satisfies import("../../types.ts").Market;
}

function context(
  overrides: Partial<ClickDomainContext> = {},
): ClickDomainContext {
  const target = {
    closest: () => null,
  } as unknown as HTMLElement;
  return {
    target,
    actionEl: null,
    action: null,
    actionDomain: "market",
    side: null,
    tradeChoiceRaw: null,
    tradeIntent: null,
    sizeMode: null,
    tradeSizePreset: Number.NaN,
    tradeSizeDelta: Number.NaN,
    limitPriceDelta: Number.NaN,
    contractsStepDelta: Number.NaN,
    orderType: null,
    tab: null,
    render: () => {},
    finishOnboarding: async () => {},
    ...overrides,
  };
}

function setWalletLotsForMarket(
  market: import("../../types.ts").Market,
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

describe("market trade helpers", () => {
  beforeEach(() => {
    const market = mockMarket();
    stateModule.setMarkets([market]);
    stateModule.state.selectedMarketId = market.id;
    stateModule.state.selectedSide = "yes";
    stateModule.state.limitSellOverrideAccepted = false;
    stateModule.state.limitSellWarning = null;
    stateModule.state.limitSellWarningInfo = "";
    stateModule.state.limitSellGuardVersion = 0;
    stateModule.state.limitSellGuardChecking = false;
    stateModule.state.tradeQuoteData = null;
    stateModule.state.tradeQuoteModalOpen = false;
    stateModule.state.tradeQuoteLoading = false;
    stateModule.state.tradeQuoteExecuting = false;
    stateModule.state.tradeQuoteError = "";
    stateModule.state.tradeQuoteNowUnix = 0;
    stateModule.state.walletData = null;
  });

  it("builds quote payload from buy/sell inputs", () => {
    const contractParams = {
      oracle_public_key: Array.from({ length: 32 }, () => 1),
      collateral_asset_id: Array.from({ length: 32 }, () => 2),
      yes_token_asset: Array.from({ length: 32 }, () => 3),
      no_token_asset: Array.from({ length: 32 }, () => 4),
      yes_reissuance_token: Array.from({ length: 32 }, () => 5),
      no_reissuance_token: Array.from({ length: 32 }, () => 6),
      collateral_per_token: 100,
      expiry_time: 1234,
    };

    const buy = marketModule.buildMarketQuoteRequestPayload(
      contractParams,
      "market-1",
      "yes",
      "open",
      1200.9,
      9.9,
    );
    expect(buy.direction).toBe("buy");
    expect(buy.exact_input).toBe(1200);

    const sell = marketModule.buildMarketQuoteRequestPayload(
      contractParams,
      "market-1",
      "no",
      "close",
      1200.9,
      9.9,
    );
    expect(sell.direction).toBe("sell");
    expect(sell.exact_input).toBe(9);
  });

  it("normalizes contract drafts to integer lots", () => {
    const market = mockMarket();
    stateModule.state.tradeIntent = "open";
    stateModule.state.tradeContractsDraft = "12.8";

    marketUtilsModule.commitTradeContractsDraft(market);

    expect(stateModule.state.tradeContracts).toBe(12);
    expect(stateModule.state.tradeContractsDraft).toBe("12");
  });

  it("allows zero-lot normalization for close intent", () => {
    const market = mockMarket();
    stateModule.state.tradeIntent = "close";
    stateModule.state.tradeContractsDraft = "";

    marketUtilsModule.commitTradeContractsDraft(market);

    expect(stateModule.state.tradeContracts).toBe(0);
    expect(stateModule.state.tradeContractsDraft).toBe("0");
  });

  it("computes limit sell warning from full-size SDK sell quote", () => {
    const warning = marketUtilsModule.getLimitSellWarningFromSellQuote(40, {
      direction: "sell",
      total_input: 25,
      total_output: 1250,
    });
    expect(warning).not.toBeNull();
    expect(warning?.discountSats).toBe(10);
    expect(warning?.discountPct).toBeCloseTo(20, 5);
    expect(warning?.referencePriceSats).toBe(50);
  });

  it("computes direction-aware effective quote price", () => {
    const buyPrice = marketUtilsModule.getQuoteEffectivePriceSatsPerContract({
      direction: "buy",
      total_input: 2400,
      total_output: 48,
    });
    const sellPrice = marketUtilsModule.getQuoteEffectivePriceSatsPerContract({
      direction: "sell",
      total_input: 48,
      total_output: 2400,
    });
    const inverse = marketUtilsModule.getQuoteEffectivePriceContractsPerSat({
      direction: "sell",
      total_input: 48,
      total_output: 2400,
    });

    expect(buyPrice).toBe(50);
    expect(sellPrice).toBe(50);
    expect(inverse).toBeCloseTo(0.02, 6);
  });

  it("computes remaining quote seconds from ticking now value", () => {
    expect(marketUtilsModule.getQuoteRemainingSeconds(100, 95)).toBe(5);
    expect(marketUtilsModule.getQuoteRemainingSeconds(100, 96)).toBe(4);
    expect(marketUtilsModule.getQuoteRemainingSeconds(100, 102)).toBe(0);
  });

  it("resets override, warning, and quote state when side changes", async () => {
    stateModule.state.selectedSide = "no";
    stateModule.state.limitSellOverrideAccepted = true;
    stateModule.state.limitSellWarning = {
      referencePriceSats: 50,
      discountSats: 10,
      discountPct: 20,
    };
    stateModule.state.limitSellWarningInfo = "warn";
    stateModule.state.tradeQuoteData = {
      quote_id: "q1",
      market_id: "m1",
      side: "no",
      direction: "sell",
      exact_input: 1,
      total_input: 1,
      total_output: 1,
      effective_price: 1,
      expires_at_unix: 1,
      legs: [],
    };
    stateModule.state.tradeQuoteModalOpen = true;

    await marketModule.handleMarketDomain(context({ side: "yes" }));

    expect(stateModule.state.limitSellOverrideAccepted).toBe(false);
    expect(stateModule.state.limitSellWarning).toBeNull();
    expect(stateModule.state.limitSellWarningInfo).toBe("");
    expect(stateModule.state.tradeQuoteData).toBeNull();
    expect(stateModule.state.tradeQuoteModalOpen).toBe(false);
  });

  it("resets override on limit-price and size edits", async () => {
    stateModule.state.limitSellOverrideAccepted = true;
    stateModule.state.limitSellWarning = {
      referencePriceSats: 45,
      discountSats: 5,
      discountPct: 11.1,
    };
    stateModule.state.limitPriceDraft = "50";
    stateModule.state.tradeContractsDraft = "2";

    await marketModule.handleMarketDomain(
      context({
        action: "step-limit-price",
        limitPriceDelta: 1,
      }),
    );
    expect(stateModule.state.limitSellOverrideAccepted).toBe(false);
    expect(stateModule.state.limitSellWarning).toBeNull();

    stateModule.state.limitSellOverrideAccepted = true;
    stateModule.state.limitSellWarning = {
      referencePriceSats: 45,
      discountSats: 5,
      discountPct: 11.1,
    };
    await marketModule.handleMarketDomain(
      context({
        action: "step-trade-contracts",
        contractsStepDelta: 1,
      }),
    );
    expect(stateModule.state.limitSellOverrideAccepted).toBe(false);
    expect(stateModule.state.limitSellWarning).toBeNull();
  });

  it("resets override on keyboard limit-price arrow edits", () => {
    stateModule.state.limitSellOverrideAccepted = true;
    stateModule.state.limitSellWarning = {
      referencePriceSats: 45,
      discountSats: 5,
      discountPct: 11.1,
    };
    stateModule.state.limitPriceDraft = "50";
    const target = { id: "limit-price" } as HTMLInputElement;
    const event = {
      key: "ArrowUp",
      preventDefault: vi.fn(),
      target,
    } as unknown as KeyboardEvent;

    keydownModule.handleKeydown(event, { render: () => {} });

    expect(stateModule.state.limitSellOverrideAccepted).toBe(false);
    expect(stateModule.state.limitSellWarning).toBeNull();
  });

  it("sell-max keeps zero when no position is available", async () => {
    stateModule.state.tradeIntent = "close";
    stateModule.state.tradeContracts = 2;
    stateModule.state.tradeContractsDraft = "2";

    await marketModule.handleMarketDomain(context({ action: "sell-max" }));

    expect(stateModule.state.tradeContracts).toBe(0);
    expect(stateModule.state.tradeContractsDraft).toBe("0");
  });

  it("sell quote request rejects zero lots", async () => {
    stateModule.state.tradeIntent = "close";
    stateModule.state.orderType = "market";
    stateModule.state.sizeMode = "contracts";
    stateModule.state.tradeContracts = 0;
    stateModule.state.tradeContractsDraft = "0";
    const quoteSpy = vi
      .spyOn(tauriApiModule.tauriApi, "quoteMarketTrade")
      .mockResolvedValue({
        quote_id: "q1",
        market_id: "m1",
        side: "yes",
        direction: "sell",
        exact_input: 1,
        total_input: 1,
        total_output: 1,
        effective_price: 1,
        expires_at_unix: 1,
        legs: [],
      });

    await marketModule.handleMarketDomain(
      context({ action: "request-trade-quote" }),
    );

    expect(quoteSpy).not.toHaveBeenCalled();
    expect(stateModule.state.tradeQuoteModalOpen).toBe(false);
    quoteSpy.mockRestore();
  });

  it("ignores repeated limit-sell submits while guard quote is in flight", async () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 10, 0);
    stateModule.state.tradeIntent = "close";
    stateModule.state.orderType = "limit";
    stateModule.state.sizeMode = "contracts";
    stateModule.state.tradeContracts = 2;
    stateModule.state.tradeContractsDraft = "2";
    stateModule.state.limitPrice = 0.4;
    stateModule.state.limitPriceDraft = "40";

    let resolvePreview!: (
      value: import("../../types.ts").PreviewMarketTradeResult,
    ) => void;
    const previewPromise = new Promise<
      import("../../types.ts").PreviewMarketTradeResult
    >((resolve) => {
      resolvePreview = resolve;
    });
    const previewSpy = vi
      .spyOn(tauriApiModule.tauriApi, "previewMarketTrade")
      .mockReturnValue(previewPromise);
    const createSpy = vi
      .spyOn(tauriApiModule.tauriApi, "createLimitOrder")
      .mockResolvedValue({
        txid: "aa".repeat(32),
        order_event_id: "evt",
        order_uid: "uid",
        order_params: {
          base_asset_id_hex: "11".repeat(32),
          quote_asset_id_hex: "22".repeat(32),
          price: 40,
          min_fill_lots: 1,
          min_remainder_lots: 1,
          direction: "sell-base",
          maker_receive_spk_hash_hex: "33".repeat(32),
          cosigner_pubkey_hex: "44".repeat(32),
          maker_pubkey_hex: "55".repeat(32),
        },
        maker_base_pubkey_hex: "66".repeat(32),
        order_nonce_hex: "77".repeat(32),
        covenant_address:
          "el1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        order_amount: 2,
      });

    await marketModule.handleMarketDomain(
      context({ action: "submit-limit-sell" }),
    );
    expect(stateModule.state.limitSellGuardChecking).toBe(true);
    await marketModule.handleMarketDomain(
      context({ action: "submit-limit-sell" }),
    );
    expect(previewSpy).toHaveBeenCalledTimes(1);

    resolvePreview({
      market_id: "m1",
      side: "yes",
      direction: "sell",
      exact_input: 2,
      total_input: 2,
      total_output: 100,
      effective_price: 0.02,
      legs: [],
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(stateModule.state.limitSellGuardChecking).toBe(false);
    expect(createSpy).not.toHaveBeenCalled();

    previewSpy.mockRestore();
    createSpy.mockRestore();
  });

  it("uses enter-size message when holdings exist but sell size is zero", async () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 5, 0);
    stateModule.state.tradeIntent = "close";
    stateModule.state.sizeMode = "contracts";
    stateModule.state.orderType = "market";
    stateModule.state.tradeContracts = 0;
    stateModule.state.tradeContractsDraft = "0";

    const toastSpy = vi.spyOn(toastModule, "showToast");
    await marketModule.handleMarketDomain(
      context({ action: "request-trade-quote" }),
    );

    expect(toastSpy).toHaveBeenCalledWith("Enter contracts to sell.", "error");
    toastSpy.mockRestore();
  });

  it("uses no-holdings message when sell size is zero and balance is zero", async () => {
    stateModule.state.tradeIntent = "close";
    stateModule.state.sizeMode = "contracts";
    stateModule.state.orderType = "market";
    stateModule.state.tradeContracts = 0;
    stateModule.state.tradeContractsDraft = "0";

    const toastSpy = vi.spyOn(toastModule, "showToast");
    await marketModule.handleMarketDomain(
      context({ action: "request-trade-quote" }),
    );

    expect(toastSpy).toHaveBeenCalledWith(
      "No contracts available on this side.",
      "error",
    );
    toastSpy.mockRestore();
  });

  it("ignores stale limit-sell preview responses", async () => {
    const market = mockMarket();
    setWalletLotsForMarket(market, 10, 0);
    stateModule.state.tradeIntent = "close";
    stateModule.state.orderType = "limit";
    stateModule.state.sizeMode = "contracts";
    stateModule.state.tradeContracts = 2;
    stateModule.state.tradeContractsDraft = "2";
    stateModule.state.limitPrice = 0.4;
    stateModule.state.limitPriceDraft = "40";

    let resolvePreview!: (
      value: import("../../types.ts").PreviewMarketTradeResult,
    ) => void;
    const previewPromise = new Promise<
      import("../../types.ts").PreviewMarketTradeResult
    >((resolve) => {
      resolvePreview = resolve;
    });
    const previewSpy = vi
      .spyOn(tauriApiModule.tauriApi, "previewMarketTrade")
      .mockReturnValue(previewPromise);
    const createSpy = vi
      .spyOn(tauriApiModule.tauriApi, "createLimitOrder")
      .mockResolvedValue({
        txid: "aa".repeat(32),
        order_event_id: "evt",
        order_uid: "uid",
        order_params: {
          base_asset_id_hex: "11".repeat(32),
          quote_asset_id_hex: "22".repeat(32),
          price: 40,
          min_fill_lots: 1,
          min_remainder_lots: 1,
          direction: "sell-base",
          maker_receive_spk_hash_hex: "33".repeat(32),
          cosigner_pubkey_hex: "44".repeat(32),
          maker_pubkey_hex: "55".repeat(32),
        },
        maker_base_pubkey_hex: "66".repeat(32),
        order_nonce_hex: "77".repeat(32),
        covenant_address:
          "el1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        order_amount: 2,
      });

    await marketModule.handleMarketDomain(
      context({ action: "submit-limit-sell" }),
    );
    expect(stateModule.state.limitSellGuardChecking).toBe(true);

    await marketModule.handleMarketDomain(
      context({ action: "step-limit-price", limitPriceDelta: 1 }),
    );
    expect(stateModule.state.limitSellGuardChecking).toBe(false);

    resolvePreview({
      market_id: "m1",
      side: "yes",
      direction: "sell",
      exact_input: 2,
      total_input: 2,
      total_output: 100,
      effective_price: 0.02,
      legs: [],
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(stateModule.state.limitSellWarning).toBeNull();
    expect(stateModule.state.limitSellGuardChecking).toBe(false);
    expect(createSpy).not.toHaveBeenCalled();

    previewSpy.mockRestore();
    createSpy.mockRestore();
  });
});
