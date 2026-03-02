import { describe, expect, it } from "vitest";
import type {
  DiscoveredOrder,
  MakerOrderParamsPayload,
  Market,
  RecoveredOwnLimitOrder,
} from "../types.ts";
import { attachOrdersToMarkets } from "./market-order-merge.ts";

const MARKET_ID = "aa".repeat(32);
const COLLATERAL_ASSET = "bb".repeat(32);
const YES_ASSET = "11".repeat(32);
const NO_ASSET = "22".repeat(32);
const MAKER_PUBKEY = "33".repeat(32);
const ORDER_NONCE = "44".repeat(32);
const COSIGNER_PUBKEY = "55".repeat(32);
const MAKER_RECEIVE_SPK_HASH = "66".repeat(32);

function baseMarket(): Market {
  return {
    id: "market-event-id",
    nevent: "nevent1",
    question: "Will this test pass?",
    category: "Bitcoin",
    description: "test market",
    resolutionSource: "test source",
    isLive: true,
    state: 1,
    marketId: MARKET_ID,
    oraclePubkey: "77".repeat(32),
    expiryHeight: 1000,
    currentHeight: 900,
    cptSats: 10_000,
    collateralAssetId: COLLATERAL_ASSET,
    yesAssetId: YES_ASSET,
    noAssetId: NO_ASSET,
    yesReissuanceToken: "88".repeat(32),
    noReissuanceToken: "99".repeat(32),
    creationTxid: "txid",
    collateralUtxos: [],
    nostrEventJson: null,
    yesPrice: null,
    change24h: 0,
    volumeBtc: 0,
    liquidityBtc: 0,
    limitOrders: [],
  };
}

function baseOrderParams(
  overrides: Partial<MakerOrderParamsPayload> = {},
): MakerOrderParamsPayload {
  return {
    base_asset_id_hex: YES_ASSET,
    quote_asset_id_hex: COLLATERAL_ASSET,
    price: 10,
    min_fill_lots: 1,
    min_remainder_lots: 1,
    direction: "sell-base",
    maker_receive_spk_hash_hex: MAKER_RECEIVE_SPK_HASH,
    cosigner_pubkey_hex: COSIGNER_PUBKEY,
    maker_pubkey_hex: MAKER_PUBKEY,
    ...overrides,
  };
}

function baseDiscoveredOrder(
  overrides: Partial<DiscoveredOrder> = {},
): DiscoveredOrder {
  return {
    id: "order-1",
    order_uid: "uid-1",
    market_id: MARKET_ID,
    base_asset_id: YES_ASSET,
    quote_asset_id: COLLATERAL_ASSET,
    price: 10,
    min_fill_lots: 1,
    min_remainder_lots: 1,
    direction: "sell-base",
    direction_label: "sell-yes",
    maker_base_pubkey: MAKER_PUBKEY,
    order_nonce: ORDER_NONCE,
    covenant_address: "addr",
    offered_amount: 5,
    cosigner_pubkey: COSIGNER_PUBKEY,
    maker_receive_spk_hash: MAKER_RECEIVE_SPK_HASH,
    creator_pubkey: MAKER_PUBKEY,
    created_at: 1,
    nostr_event_json: null,
    ...overrides,
  };
}

function baseRecoveredOrder(
  overrides: Partial<RecoveredOwnLimitOrder> = {},
): RecoveredOwnLimitOrder {
  return {
    txid: "ab".repeat(32),
    vout: 0,
    outpoint: `${"ab".repeat(32)}:0`,
    offered_asset_id_hex: YES_ASSET,
    offered_amount: 5,
    order_index: 1,
    maker_base_pubkey_hex: MAKER_PUBKEY,
    order_nonce_hex: ORDER_NONCE,
    order_params: baseOrderParams(),
    status: "active_confirmed",
    ambiguity_count: 1,
    is_cancelable: true,
    ...overrides,
  };
}

describe("attachOrdersToMarkets", () => {
  it("keeps discovered orders recoverable when same key has active+spent recovery records", () => {
    const market = baseMarket();
    const discoveredOrder = baseDiscoveredOrder();
    const recovered = [
      baseRecoveredOrder({
        status: "spent_or_filled",
        is_cancelable: false,
      }),
      baseRecoveredOrder({
        status: "active_confirmed",
        is_cancelable: true,
      }),
    ];

    const result = attachOrdersToMarkets(
      [market],
      [discoveredOrder],
      recovered,
    );
    expect(result[0].limitOrders).toHaveLength(1);
    expect(result[0].limitOrders[0].is_recoverable_by_current_wallet).toBe(
      true,
    );
    expect(result[0].limitOrders[0].own_order_recovery_status).toBe(
      "active_confirmed",
    );
  });

  it("is order-independent when selecting best recovered status for discovered orders", () => {
    const market = baseMarket();
    const discoveredOrder = baseDiscoveredOrder();
    const recovered = [
      baseRecoveredOrder({
        status: "active_mempool",
        is_cancelable: true,
      }),
      baseRecoveredOrder({
        status: "active_confirmed",
        is_cancelable: true,
      }),
    ];

    const reversed = [...recovered].reverse();
    const firstPass = attachOrdersToMarkets(
      [market],
      [discoveredOrder],
      recovered,
    );
    const secondPass = attachOrdersToMarkets(
      [market],
      [discoveredOrder],
      reversed,
    );

    expect(firstPass[0].limitOrders[0].is_recoverable_by_current_wallet).toBe(
      true,
    );
    expect(secondPass[0].limitOrders[0].is_recoverable_by_current_wallet).toBe(
      true,
    );
    expect(firstPass[0].limitOrders[0].own_order_recovery_status).toBe(
      "active_confirmed",
    );
    expect(secondPass[0].limitOrders[0].own_order_recovery_status).toBe(
      "active_confirmed",
    );
  });

  it("marks discovered orders non-recoverable when only spent records exist", () => {
    const market = baseMarket();
    const discoveredOrder = baseDiscoveredOrder();
    const recovered = [
      baseRecoveredOrder({
        status: "spent_or_filled",
        is_cancelable: false,
      }),
    ];

    const result = attachOrdersToMarkets(
      [market],
      [discoveredOrder],
      recovered,
    );
    expect(result[0].limitOrders).toHaveLength(1);
    expect(result[0].limitOrders[0].is_recoverable_by_current_wallet).toBe(
      false,
    );
    expect(result[0].limitOrders[0].own_order_recovery_status).toBe(
      "spent_or_filled",
    );
  });

  it("creates one synthetic row for cancelable recovered-only orders", () => {
    const market = baseMarket();
    const recovered = [baseRecoveredOrder()];

    const result = attachOrdersToMarkets([market], [], recovered);
    expect(result[0].limitOrders).toHaveLength(1);
    expect(result[0].limitOrders[0].source).toBe("recovered-local");
    expect(result[0].limitOrders[0].is_recoverable_by_current_wallet).toBe(
      true,
    );
  });

  it("does not create synthetic rows for non-cancelable recovered-only orders", () => {
    const market = baseMarket();
    const recovered = [
      baseRecoveredOrder({
        status: "spent_or_filled",
        is_cancelable: false,
      }),
    ];

    const result = attachOrdersToMarkets([market], [], recovered);
    expect(result[0].limitOrders).toHaveLength(0);
  });

  it("does not create synthetic rows when maker pubkey is missing", () => {
    const market = baseMarket();
    const recovered = [
      baseRecoveredOrder({
        maker_base_pubkey_hex: null,
        is_cancelable: true,
      }),
    ];

    const result = attachOrdersToMarkets([market], [], recovered);
    expect(result[0].limitOrders).toHaveLength(0);
  });

  it("dedupes synthetic rows by maker+nonce key", () => {
    const market = baseMarket();
    const recovered = [
      baseRecoveredOrder({
        status: "active_mempool",
        outpoint: `${"cd".repeat(32)}:1`,
        txid: "cd".repeat(32),
        vout: 1,
      }),
      baseRecoveredOrder({
        status: "active_confirmed",
        outpoint: `${"ef".repeat(32)}:2`,
        txid: "ef".repeat(32),
        vout: 2,
      }),
    ];

    const result = attachOrdersToMarkets([market], [], recovered);
    expect(result[0].limitOrders).toHaveLength(1);
    expect(result[0].limitOrders[0].own_order_recovery_status).toBe(
      "active_confirmed",
    );
  });
});
