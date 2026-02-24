import { invoke } from "@tauri-apps/api/core";
import { setMarkets } from "../state.ts";
import type {
  DiscoveredMarket,
  IssuanceResult,
  Market,
  MarketCategory,
} from "../types.ts";
import { hexToBytes } from "../utils/crypto.ts";

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
    yesPrice: d.starting_yes_price / 100,
    change24h: 0,
    volumeBtc: 0,
    liquidityBtc: 0,
  };
}

export async function loadMarkets(): Promise<void> {
  try {
    // 1. Fetch from Nostr
    const discovered = await invoke<DiscoveredMarket[]>("discover_contracts");
    // 2. Ingest into store (incompatible contracts silently dropped)
    await invoke("ingest_discovered_markets", { markets: discovered });
    // 3. Load from store (only compatible, compiled contracts with on-chain state)
    const stored = await invoke<DiscoveredMarket[]>("list_contracts");
    setMarkets(stored.map(discoveredToMarket));
  } catch (error) {
    console.warn("Failed to discover contracts:", error);
    setMarkets([]);
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

export async function issueTokens(
  market: Market,
  pairs: number,
): Promise<IssuanceResult> {
  if (!market.creationTxid) {
    throw new Error("Market has no creation txid — cannot issue tokens");
  }
  return invoke<IssuanceResult>("issue_tokens", {
    contractParamsJson: marketToContractParamsJson(market),
    creationTxid: market.creationTxid,
    pairs,
  });
}
