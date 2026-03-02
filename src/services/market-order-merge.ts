import type {
  DiscoveredOrder,
  Market,
  OwnOrderRecoveryStatus,
  RecoveredOwnLimitOrder,
} from "../types.ts";

const RECOVERY_STATUS_RANK: Record<OwnOrderRecoveryStatus, number> = {
  active_confirmed: 4,
  active_mempool: 3,
  ambiguous: 2,
  spent_or_filled: 1,
};

type RecoveryAggregate = {
  isRecoverableByCurrentWallet: boolean;
  bestStatus: OwnOrderRecoveryStatus | null;
  preferredRecordForSynthetic: RecoveredOwnLimitOrder | null;
};

function recoveryKey(makerBasePubkey: string, orderNonce: string): string {
  return `${makerBasePubkey.toLowerCase()}:${orderNonce.toLowerCase()}`;
}

function recoveredOrderKey(order: RecoveredOwnLimitOrder): string | null {
  if (!order.maker_base_pubkey_hex || !order.order_nonce_hex) {
    return null;
  }
  return recoveryKey(order.maker_base_pubkey_hex, order.order_nonce_hex);
}

function betterStatus(
  current: OwnOrderRecoveryStatus | null,
  candidate: OwnOrderRecoveryStatus,
): OwnOrderRecoveryStatus {
  if (!current) {
    return candidate;
  }
  if (RECOVERY_STATUS_RANK[candidate] > RECOVERY_STATUS_RANK[current]) {
    return candidate;
  }
  return current;
}

function preferredSyntheticRecord(
  current: RecoveredOwnLimitOrder | null,
  candidate: RecoveredOwnLimitOrder,
): RecoveredOwnLimitOrder | null {
  if (!candidate.is_cancelable) {
    return current;
  }
  if (!current) {
    return candidate;
  }
  if (!current.is_cancelable) {
    return candidate;
  }
  if (
    RECOVERY_STATUS_RANK[candidate.status] >
    RECOVERY_STATUS_RANK[current.status]
  ) {
    return candidate;
  }
  return current;
}

function directionLabelForRecovered(
  direction: "sell-base" | "sell-quote",
  side: "yes" | "no",
): string {
  return `${direction === "sell-quote" ? "buy" : "sell"}-${side}`;
}

function syntheticRecoveredOrderForMarket(
  market: Market,
  recovered: RecoveredOwnLimitOrder,
): DiscoveredOrder | null {
  if (!recovered.is_cancelable) return null;
  if (!recovered.order_params) return null;
  if (!recovered.maker_base_pubkey_hex || !recovered.order_nonce_hex)
    return null;

  const params = recovered.order_params;
  if (
    params.quote_asset_id_hex.toLowerCase() !==
    market.collateralAssetId.toLowerCase()
  ) {
    return null;
  }

  const baseAsset = params.base_asset_id_hex.toLowerCase();
  const yesAsset = market.yesAssetId.toLowerCase();
  const noAsset = market.noAssetId.toLowerCase();
  const side: "yes" | "no" | null =
    baseAsset === yesAsset ? "yes" : baseAsset === noAsset ? "no" : null;
  if (!side) return null;

  const makerPubkey = recovered.maker_base_pubkey_hex;
  return {
    id: `local:${recovered.outpoint.toLowerCase()}`,
    order_uid: `local:${recovered.outpoint.toLowerCase()}`,
    market_id: market.marketId,
    base_asset_id: params.base_asset_id_hex,
    quote_asset_id: params.quote_asset_id_hex,
    price: params.price,
    min_fill_lots: params.min_fill_lots,
    min_remainder_lots: params.min_remainder_lots,
    direction: params.direction,
    direction_label: directionLabelForRecovered(params.direction, side),
    maker_base_pubkey: makerPubkey,
    order_nonce: recovered.order_nonce_hex,
    covenant_address: "",
    offered_amount: recovered.offered_amount,
    cosigner_pubkey: params.cosigner_pubkey_hex,
    maker_receive_spk_hash: params.maker_receive_spk_hash_hex,
    creator_pubkey: makerPubkey,
    created_at: 0,
    nostr_event_json: null,
    source: "recovered-local",
    is_recoverable_by_current_wallet: true,
    own_order_recovery_status: recovered.status,
  };
}

function buildRecoveryAggregates(
  recoveredOwnOrders: RecoveredOwnLimitOrder[],
): Map<string, RecoveryAggregate> {
  const aggregates = new Map<string, RecoveryAggregate>();
  for (const recovered of recoveredOwnOrders) {
    const key = recoveredOrderKey(recovered);
    if (!key) {
      continue;
    }

    const current = aggregates.get(key) ?? {
      isRecoverableByCurrentWallet: false,
      bestStatus: null,
      preferredRecordForSynthetic: null,
    };

    const next: RecoveryAggregate = {
      isRecoverableByCurrentWallet:
        current.isRecoverableByCurrentWallet || recovered.is_cancelable,
      bestStatus: betterStatus(current.bestStatus, recovered.status),
      preferredRecordForSynthetic: preferredSyntheticRecord(
        current.preferredRecordForSynthetic,
        recovered,
      ),
    };
    aggregates.set(key, next);
  }
  return aggregates;
}

export function attachOrdersToMarkets(
  nextMarkets: Market[],
  orders: DiscoveredOrder[],
  recoveredOwnOrders: RecoveredOwnLimitOrder[],
): Market[] {
  const recoveryByOrder = buildRecoveryAggregates(recoveredOwnOrders);
  const byMarketId = new Map<string, DiscoveredOrder[]>();

  for (const order of orders) {
    const recovered = recoveryByOrder.get(
      recoveryKey(order.maker_base_pubkey, order.order_nonce),
    );
    const annotated: DiscoveredOrder = {
      ...order,
      source: "nostr",
      is_recoverable_by_current_wallet:
        recovered?.isRecoverableByCurrentWallet ?? false,
      own_order_recovery_status: recovered?.bestStatus ?? null,
    };
    const key = order.market_id.toLowerCase();
    const existing = byMarketId.get(key);
    if (existing) {
      existing.push(annotated);
    } else {
      byMarketId.set(key, [annotated]);
    }
  }

  return nextMarkets.map((market) => {
    const limitOrders = (
      byMarketId.get(market.marketId.toLowerCase()) ?? []
    ).slice();
    const seenKeys = new Set<string>();
    for (const order of limitOrders) {
      seenKeys.add(recoveryKey(order.maker_base_pubkey, order.order_nonce));
    }

    for (const [key, aggregate] of recoveryByOrder.entries()) {
      const preferred = aggregate.preferredRecordForSynthetic;
      if (!preferred) {
        continue;
      }
      if (seenKeys.has(key)) {
        continue;
      }

      const synthetic = syntheticRecoveredOrderForMarket(market, preferred);
      if (!synthetic) {
        continue;
      }
      synthetic.own_order_recovery_status = aggregate.bestStatus;
      limitOrders.push(synthetic);
      seenKeys.add(key);
    }

    return {
      ...market,
      limitOrders,
    };
  });
}
