import { tauriApi } from "../api/tauri.ts";
import { setMarkets } from "../state.ts";
import type {
  ContractParamsPayload,
  DiscoveredMarket,
  DiscoveredOrder,
  IssuanceResult,
  Market,
  MarketCategory,
  PricePoint,
  RecoveredOwnLimitOrder,
} from "../types.ts";
import { hexToBytes } from "../utils/crypto.ts";
import { attachOrdersToMarkets } from "./market-order-merge.ts";

export function discoveredToMarket(d: DiscoveredMarket): Market {
  return {
    id: d.id,
    nevent: d.nevent,
    question: d.question,
    category: ([
      "Bitcoin",
      "Politics",
      "Sports",
      "Culture",
      "Weather",
      "Macro",
    ].includes(d.category)
      ? d.category
      : "Bitcoin") as MarketCategory,
    description: d.description,
    resolutionSource: d.resolution_source,
    isLive: d.state === 1,
    state: d.state,
    marketId: d.market_id,
    oraclePubkey: d.oracle_pubkey,
    expiryHeight: d.expiry_height,
    currentHeight: 0,
    cptSats: d.cpt_sats,
    collateralAssetId: d.collateral_asset_id,
    yesAssetId: d.yes_asset_id,
    noAssetId: d.no_asset_id,
    yesReissuanceToken: d.yes_reissuance_token,
    noReissuanceToken: d.no_reissuance_token,
    creationTxid: d.creation_txid,
    collateralUtxos: [],
    nostrEventJson: d.nostr_event_json ?? null,
    yesPrice: d.yes_price_bps != null ? d.yes_price_bps / 10000 : null,
    change24h: 0,
    volumeBtc: 0,
    liquidityBtc: 0,
    limitOrders: [],
  };
}

async function loadOrdersAndRecovery(mode: "load" | "refresh"): Promise<{
  orders: DiscoveredOrder[];
  recoveredOwnOrders: RecoveredOwnLimitOrder[];
}> {
  const ordersError =
    mode === "load"
      ? "Failed to load limit orders:"
      : "Failed to refresh limit orders:";

  let orders: DiscoveredOrder[] = [];
  let recoveredOwnOrders: RecoveredOwnLimitOrder[] = [];
  try {
    orders = await tauriApi.discoverLimitOrders();
  } catch (error) {
    console.warn(ordersError, error);
  }
  try {
    recoveredOwnOrders = await tauriApi.recoverOwnLimitOrders();
  } catch (error) {
    console.warn("Failed to recover own limit orders:", error);
  }
  return { orders, recoveredOwnOrders };
}

async function refreshMarkets(
  fetchMarkets: () => Promise<DiscoveredMarket[]>,
  mode: "load" | "refresh",
): Promise<void> {
  const stored = await fetchMarkets();
  const { orders, recoveredOwnOrders } = await loadOrdersAndRecovery(mode);
  setMarkets(
    attachOrdersToMarkets(
      stored.map(discoveredToMarket),
      orders,
      recoveredOwnOrders,
    ),
  );
}

export async function loadMarkets(): Promise<void> {
  try {
    await refreshMarkets(() => tauriApi.discoverContracts(), "load");
  } catch (error) {
    console.warn("Failed to load markets:", error);
    setMarkets([]);
  }
}

export async function refreshMarketsFromStore(): Promise<void> {
  try {
    await refreshMarkets(() => tauriApi.listContracts(), "refresh");
  } catch (error) {
    console.warn("Failed to refresh markets from store:", error);
  }
}

export function marketToContractParams(market: Market): ContractParamsPayload {
  return {
    oracle_public_key: hexToBytes(market.oraclePubkey),
    collateral_asset_id: hexToBytes(market.collateralAssetId),
    yes_token_asset: hexToBytes(market.yesAssetId),
    no_token_asset: hexToBytes(market.noAssetId),
    yes_reissuance_token: hexToBytes(market.yesReissuanceToken),
    no_reissuance_token: hexToBytes(market.noReissuanceToken),
    collateral_per_token: market.cptSats,
    expiry_time: market.expiryHeight,
  };
}

export async function issueTokens(
  market: Market,
  pairs: number,
): Promise<IssuanceResult> {
  if (!market.creationTxid) {
    throw new Error("Market has no creation txid — cannot issue tokens");
  }
  return tauriApi.issueTokens(
    marketToContractParams(market),
    market.creationTxid,
    pairs,
  );
}

export async function syncPool(poolId: string): Promise<void> {
  await tauriApi.syncPool(poolId);
}

export async function loadPriceHistory(
  marketId: string,
): Promise<PricePoint[]> {
  return tauriApi.getPoolPriceHistory(marketId);
}
