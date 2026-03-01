import type { DiscoveredOrder } from "../../types.ts";

export function getFillBlockedReason(order: DiscoveredOrder): string | null {
  if (order.source === "recovered-local") {
    return "Local-only recovered orders are not fillable from the order book";
  }
  if (order.own_order_recovery_status === "spent_or_filled") {
    return "This order is no longer active and cannot be filled";
  }
  if (order.own_order_recovery_status === "ambiguous") {
    return "This order has ambiguous recovery state and cannot be safely filled";
  }
  return null;
}

export function canRenderFillButton(order: DiscoveredOrder): boolean {
  return getFillBlockedReason(order) === null;
}
