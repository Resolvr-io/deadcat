import { describe, expect, it } from "vitest";

import type { DiscoveredOrder } from "../../types.ts";
import {
  canRenderFillButton,
  getFillBlockedReason,
} from "./limit-order-guards.ts";

const HEX_A = "aa".repeat(32);
const HEX_B = "bb".repeat(32);
const HEX_C = "cc".repeat(32);

function baseOrder(overrides: Partial<DiscoveredOrder> = {}): DiscoveredOrder {
  return {
    id: "order-1",
    order_uid: "uid-1",
    market_id: HEX_A,
    base_asset_id: HEX_B,
    quote_asset_id: HEX_C,
    price: 10,
    min_fill_lots: 1,
    min_remainder_lots: 1,
    direction: "sell-base",
    direction_label: "sell-yes",
    maker_base_pubkey: HEX_A,
    order_nonce: HEX_B,
    covenant_address: "el1qqtestaddress",
    offered_amount: 5,
    cosigner_pubkey: HEX_C,
    maker_receive_spk_hash: HEX_A,
    creator_pubkey: HEX_B,
    created_at: 1,
    ...overrides,
  };
}

describe("limit-order fill guards", () => {
  it("blocks recovered-local orders", () => {
    const order = baseOrder({ source: "recovered-local" });
    expect(canRenderFillButton(order)).toBe(false);
    expect(getFillBlockedReason(order)).toContain("Local-only recovered");
  });

  it("blocks spent_or_filled own orders", () => {
    const order = baseOrder({ own_order_recovery_status: "spent_or_filled" });
    expect(canRenderFillButton(order)).toBe(false);
    expect(getFillBlockedReason(order)).toContain("no longer active");
  });

  it("blocks ambiguous own orders", () => {
    const order = baseOrder({ own_order_recovery_status: "ambiguous" });
    expect(canRenderFillButton(order)).toBe(false);
    expect(getFillBlockedReason(order)).toContain("ambiguous");
  });

  it("allows active_confirmed own orders", () => {
    const order = baseOrder({ own_order_recovery_status: "active_confirmed" });
    expect(canRenderFillButton(order)).toBe(true);
    expect(getFillBlockedReason(order)).toBeNull();
  });

  it("allows active_mempool own orders", () => {
    const order = baseOrder({ own_order_recovery_status: "active_mempool" });
    expect(canRenderFillButton(order)).toBe(true);
    expect(getFillBlockedReason(order)).toBeNull();
  });

  it("allows orders without own recovery status", () => {
    const order = baseOrder();
    expect(canRenderFillButton(order)).toBe(true);
    expect(getFillBlockedReason(order)).toBeNull();
  });
});
