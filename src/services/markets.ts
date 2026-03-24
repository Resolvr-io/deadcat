import { invoke } from "@tauri-apps/api/core";
import { markets, setMarkets } from "../state.ts";
import type {
  CancelLimitOrderResponse,
  CancellationResult,
  CreateContractOnchainResponse,
  CreateLimitOrderResponse,
  DiscoveredMarket,
  DiscoveredOrder,
  ExecuteTradeExpectedQuote,
  ExecuteTradeResponse,
  IssuanceResult,
  Market,
  MarketCategory,
  OwnOrderSummary,
  RedemptionResult,
  ResolutionResult,
  Side,
  TradeDirection,
  TradeQuoteResponse,
  TxOptions,
} from "../types.ts";
import { hexToBytes } from "../utils/crypto.ts";
import { DEFAULT_TX_OPTIONS } from "./tx.ts";

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
    anchor: d.anchor,
    limitOrders: [],
    creationTxid: d.anchor?.creation_txid ?? null,
    collateralUtxos: [],
    nostrEventJson: d.nostr_event_json ?? null,
    yesPrice: d.yes_price_bps != null ? d.yes_price_bps / 10000 : null,
    change24h: 0,
    volumeBtc: 0,
    liquidityBtc: 0,
    dormantTxid: d.dormant_txid ?? null,
    unresolvedTxid: d.unresolved_txid ?? null,
    resolvedYesTxid: d.resolved_yes_txid ?? null,
    resolvedNoTxid: d.resolved_no_txid ?? null,
    expiredTxid: d.expired_txid ?? null,
  };
}

export async function loadMarkets(): Promise<void> {
  try {
    const stored = await invoke<DiscoveredMarket[]>("discover_contracts");
    setMarkets(stored.map(discoveredToMarket));
  } catch (error) {
    console.warn("Failed to load markets:", error);
    setMarkets([]);
  }
}

export async function refreshMarketsFromStore(): Promise<void> {
  try {
    const stored = await invoke<DiscoveredMarket[]>("list_contracts");
    const oldByMarketId = new Map(markets.map((m) => [m.marketId, m]));
    setMarkets(
      stored.map((d) => {
        const m = discoveredToMarket(d);
        const prev = oldByMarketId.get(m.marketId);
        if (prev) {
          m.limitOrders = prev.limitOrders;
          m.collateralUtxos = prev.collateralUtxos;
        }
        return m;
      }),
    );
  } catch (error) {
    console.warn("Failed to refresh markets from store:", error);
  }
}

export function marketToContractParamsJson(market: Market): string {
  return JSON.stringify({
    oracle_public_key: hexToBytes(market.oraclePubkey),
    collateral_asset_id: hexToBytes(market.collateralAssetId),
    yes_token_asset: hexToBytes(market.yesAssetId),
    no_token_asset: hexToBytes(market.noAssetId),
    yes_reissuance_token: hexToBytes(market.yesReissuanceToken),
    no_reissuance_token: hexToBytes(market.noReissuanceToken),
    collateral_per_token: market.cptSats,
    expiry_time: market.expiryHeight,
  });
}

export async function createContractOnchain(request: {
  question: string;
  description: string;
  category: MarketCategory;
  resolution_source: string;
  settlement_deadline_unix: number;
  collateral_per_token: number;
  tx_options: TxOptions;
}): Promise<CreateContractOnchainResponse> {
  return invoke<CreateContractOnchainResponse>("create_contract_onchain", {
    request,
  });
}

export async function issueTokens(
  market: Market,
  pairs: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<IssuanceResult> {
  if (!market.anchor) {
    throw new Error("Market has no canonical anchor — cannot issue tokens");
  }
  return invoke<IssuanceResult>("issue_tokens", {
    contractParamsJson: marketToContractParamsJson(market),
    anchor: market.anchor,
    pairs,
    txOptions,
  });
}

export async function resolveMarket(
  market: Market,
  anchor: NonNullable<Market["anchor"]>,
  outcomeYes: boolean,
  oracleSignatureHex: string,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<ResolutionResult> {
  return invoke<ResolutionResult>("resolve_market", {
    contractParamsJson: marketToContractParamsJson(market),
    anchor,
    outcomeYes,
    oracleSignatureHex,
    txOptions,
  });
}

export async function cancelTokens(
  market: Market,
  anchor: NonNullable<Market["anchor"]>,
  pairs: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<CancellationResult> {
  return invoke<CancellationResult>("cancel_tokens", {
    contractParamsJson: marketToContractParamsJson(market),
    anchor,
    pairs,
    txOptions,
  });
}

export async function redeemTokens(
  market: Market,
  anchor: NonNullable<Market["anchor"]>,
  tokens: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<RedemptionResult> {
  return invoke<RedemptionResult>("redeem_tokens", {
    contractParamsJson: marketToContractParamsJson(market),
    anchor,
    tokens,
    txOptions,
  });
}

export async function redeemExpiredTokens(
  market: Market,
  anchor: NonNullable<Market["anchor"]>,
  tokenAssetHex: string,
  tokens: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<RedemptionResult> {
  return invoke<RedemptionResult>("redeem_expired", {
    contractParamsJson: marketToContractParamsJson(market),
    anchor,
    tokenAssetHex,
    tokens,
    txOptions,
  });
}

export async function quoteTrade(
  market: Market,
  side: Side,
  direction: TradeDirection,
  exactInput: number,
): Promise<TradeQuoteResponse> {
  return invoke<TradeQuoteResponse>("quote_trade", {
    request: {
      contract_params_json: marketToContractParamsJson(market),
      market_id: market.marketId,
      side,
      direction,
      exact_input: Math.max(1, Math.floor(exactInput)),
    },
  });
}

export async function executeTrade(
  market: Market,
  side: Side,
  direction: TradeDirection,
  exactInput: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
  expectedQuote?: ExecuteTradeExpectedQuote,
): Promise<ExecuteTradeResponse> {
  return invoke<ExecuteTradeResponse>("execute_trade", {
    request: {
      contract_params_json: marketToContractParamsJson(market),
      market_id: market.marketId,
      side,
      direction,
      exact_input: Math.max(1, Math.floor(exactInput)),
      tx_options: txOptions,
      expected_quote: expectedQuote,
    },
  });
}

export async function fetchOrders(
  marketId?: string,
): Promise<DiscoveredOrder[]> {
  return invoke<DiscoveredOrder[]>("fetch_orders", {
    marketId: marketId ?? null,
  });
}

export async function createLimitOrder(
  market: Market,
  side: Side,
  direction: TradeDirection,
  price: number,
  amount: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<CreateLimitOrderResponse> {
  return invoke<CreateLimitOrderResponse>("create_limit_order", {
    request: {
      contract_params_json: marketToContractParamsJson(market),
      market_id: market.marketId,
      side,
      direction,
      price: Math.floor(price),
      amount: Math.floor(amount),
      tx_options: txOptions,
    },
  });
}

export async function cancelLimitOrder(
  order: DiscoveredOrder,
  orderIndex?: number,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<CancelLimitOrderResponse> {
  return invoke<CancelLimitOrderResponse>("cancel_limit_order", {
    request: {
      market_id: order.market_id,
      base_asset_id: order.base_asset_id,
      quote_asset_id: order.quote_asset_id,
      price: order.price,
      min_fill_lots: order.min_fill_lots,
      min_remainder_lots: order.min_remainder_lots,
      direction: order.direction,
      maker_base_pubkey: order.maker_base_pubkey,
      order_nonce: order.order_nonce,
      cosigner_pubkey: order.cosigner_pubkey,
      maker_receive_spk_hash: order.maker_receive_spk_hash,
      tx_options: txOptions,
      order_index: orderIndex ?? null,
    },
  });
}

export async function fetchOwnOrders(): Promise<OwnOrderSummary[]> {
  return invoke<OwnOrderSummary[]>("list_own_orders");
}

export function mergeOrdersIntoMarket(
  marketId: string,
  orders: DiscoveredOrder[],
): void {
  const market = markets.find((m) => m.marketId === marketId);
  if (market) {
    market.limitOrders = orders.filter((o) => o.market_id === marketId);
  }
}
