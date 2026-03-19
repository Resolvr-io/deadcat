//! Helpers for converting Nostr discovery types into typed SDK structs.

use std::collections::HashSet;

use crate::discovery::pool::{PoolAnnouncement, PoolParams, derive_lmsr_pool_id_hex};
use crate::discovery::{DiscoveredOrder, DiscoveredPool};
use crate::error::{Error, Result};
use crate::lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolId, LmsrPoolParams};
use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::pool::PoolReserves;
use crate::prediction_market::params::derive_market_id_from_assets;

/// Decode a hex string into a fixed 32-byte array.
pub(crate) fn hex_to_bytes32(hex: &str) -> Result<[u8; 32]> {
    let bytes =
        hex::decode(hex).map_err(|e| Error::TradeRouting(format!("invalid hex '{hex}': {e}")))?;
    if bytes.len() != 32 {
        return Err(Error::TradeRouting(format!(
            "expected 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Parsed LMSR discovery payload with required canonical anchors.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct ParsedLmsrDiscoveredPool {
    pub params: LmsrPoolParams,
    pub lmsr_pool_id: String,
    pub current_s_index: u64,
    pub creation_txid: [u8; 32],
    pub initial_reserve_outpoints: [LmsrInitialOutpoint; 3],
    pub witness_schema_version: String,
    pub table_values: Option<Vec<u64>>,
}

/// Parse LMSR-specific fields from a discovered pool announcement.
///
/// Returns an error if any v0.1 mandatory LMSR fields are absent.
pub(crate) fn parse_discovered_lmsr_pool(
    pool: &DiscoveredPool,
    network_tag: &str,
) -> Result<ParsedLmsrDiscoveredPool> {
    let lmsr_pool_id = LmsrPoolId::from_hex(&pool.lmsr_pool_id)
        .map_err(|e| Error::TradeRouting(format!("invalid lmsr_pool_id: {e}")))?
        .to_hex();
    let lmsr_table_root = hex_to_bytes32(&pool.lmsr_table_root)?;
    let table_depth = pool.table_depth;
    let q_step_lots = pool.q_step_lots;
    let s_bias = pool.s_bias;
    let s_max_index = pool.s_max_index;
    let half_payout_sats = pool.half_payout_sats;
    let current_s_index = pool.current_s_index;
    let initial_reserve_outpoints = pool.initial_reserve_outpoints.clone();
    if initial_reserve_outpoints.len() != 3 {
        return Err(Error::TradeRouting(format!(
            "expected 3 LMSR initial reserve outpoints, got {}",
            initial_reserve_outpoints.len()
        )));
    }
    let creation_txid = hex_to_bytes32(&pool.creation_txid)?;
    let witness_schema_version = pool.witness_schema_version.clone();
    let parsed_outpoint_0 = parse_outpoint(
        &initial_reserve_outpoints[0],
        "initial_reserve_outpoints[0]",
    )?;
    let parsed_outpoint_1 = parse_outpoint(
        &initial_reserve_outpoints[1],
        "initial_reserve_outpoints[1]",
    )?;
    let parsed_outpoint_2 = parse_outpoint(
        &initial_reserve_outpoints[2],
        "initial_reserve_outpoints[2]",
    )?;
    let canonical_0 = canonical_outpoint_string(parsed_outpoint_0);
    let canonical_1 = canonical_outpoint_string(parsed_outpoint_1);
    let canonical_2 = canonical_outpoint_string(parsed_outpoint_2);
    if initial_reserve_outpoints[0] != canonical_0 {
        return Err(Error::TradeRouting(
            "initial_reserve_outpoints[0] must use canonical '<lowercase_txid>:<vout>' format"
                .into(),
        ));
    }
    if initial_reserve_outpoints[1] != canonical_1 {
        return Err(Error::TradeRouting(
            "initial_reserve_outpoints[1] must use canonical '<lowercase_txid>:<vout>' format"
                .into(),
        ));
    }
    if initial_reserve_outpoints[2] != canonical_2 {
        return Err(Error::TradeRouting(
            "initial_reserve_outpoints[2] must use canonical '<lowercase_txid>:<vout>' format"
                .into(),
        ));
    }
    let initial_reserve_outpoints = [parsed_outpoint_0, parsed_outpoint_1, parsed_outpoint_2];
    let mut seen = HashSet::new();
    for (idx, outpoint) in initial_reserve_outpoints.iter().enumerate() {
        if !seen.insert((outpoint.txid, outpoint.vout)) {
            return Err(Error::TradeRouting(format!(
                "duplicate LMSR initial reserve outpoint at index {idx}"
            )));
        }
    }
    for (idx, outpoint) in initial_reserve_outpoints.iter().enumerate() {
        if outpoint.txid != creation_txid {
            return Err(Error::TradeRouting(format!(
                "initial_reserve_outpoints[{idx}] txid must match creation_txid"
            )));
        }
    }

    let params = LmsrPoolParams {
        yes_asset_id: hex_to_bytes32(&pool.yes_asset_id)?,
        no_asset_id: hex_to_bytes32(&pool.no_asset_id)?,
        collateral_asset_id: hex_to_bytes32(&pool.lbtc_asset_id)?,
        lmsr_table_root,
        table_depth,
        q_step_lots,
        s_bias,
        s_max_index,
        half_payout_sats,
        fee_bps: pool.fee_bps,
        min_r_yes: pool.min_r_yes,
        min_r_no: pool.min_r_no,
        min_r_collateral: pool.min_r_collateral,
        cosigner_pubkey: hex_to_bytes32(&pool.cosigner_pubkey)?,
    };
    params.validate().map_err(|e| {
        Error::TradeRouting(format!("invalid LMSR params in discovery payload: {e}"))
    })?;
    if current_s_index > params.s_max_index {
        return Err(Error::TradeRouting(format!(
            "current_s_index {} exceeds s_max_index {}",
            current_s_index, params.s_max_index
        )));
    }
    let announce = PoolAnnouncement {
        version: crate::discovery::pool::LMSR_POOL_ANNOUNCEMENT_VERSION,
        params: PoolParams {
            yes_asset_id: params.yes_asset_id,
            no_asset_id: params.no_asset_id,
            lbtc_asset_id: params.collateral_asset_id,
            fee_bps: params.fee_bps,
            min_r_yes: params.min_r_yes,
            min_r_no: params.min_r_no,
            min_r_collateral: params.min_r_collateral,
            cosigner_pubkey: params.cosigner_pubkey,
        },
        market_id: pool.market_id.clone(),
        reserves: PoolReserves {
            r_yes: pool.reserves.r_yes,
            r_no: pool.reserves.r_no,
            r_lbtc: pool.reserves.r_lbtc,
        },
        creation_txid: pool.creation_txid.clone(),
        lmsr_pool_id: lmsr_pool_id.clone(),
        lmsr_table_root: pool.lmsr_table_root.clone(),
        table_depth,
        q_step_lots,
        s_bias,
        s_max_index,
        half_payout_sats,
        current_s_index,
        initial_reserve_outpoints: pool.initial_reserve_outpoints.clone(),
        witness_schema_version: witness_schema_version.clone(),
        table_manifest_hash: pool.table_manifest_hash.clone(),
        lmsr_table_values: pool.lmsr_table_values.clone(),
    };
    let derived_pool_id = derive_lmsr_pool_id_hex(&announce, network_tag).map_err(|e| {
        Error::TradeRouting(format!("failed to derive canonical lmsr_pool_id: {e}"))
    })?;
    if derived_pool_id != lmsr_pool_id {
        return Err(Error::TradeRouting(format!(
            "lmsr_pool_id does not match canonical derived ID: expected {derived_pool_id}"
        )));
    }
    let expected_market_id = derive_market_id_from_assets(params.yes_asset_id, params.no_asset_id);
    if pool.market_id != expected_market_id.to_string() {
        return Err(Error::TradeRouting(format!(
            "market_id does not match canonical derived ID: expected {expected_market_id}"
        )));
    }

    Ok(ParsedLmsrDiscoveredPool {
        params,
        lmsr_pool_id: derived_pool_id,
        current_s_index,
        creation_txid,
        initial_reserve_outpoints,
        witness_schema_version,
        table_values: pool.lmsr_table_values.clone(),
    })
}

fn parse_outpoint(s: &str, field: &str) -> Result<LmsrInitialOutpoint> {
    let (txid_hex, vout_str) = s.split_once(':').ok_or_else(|| {
        Error::TradeRouting(format!(
            "invalid outpoint in {field}: expected '<txid>:<vout>', got '{s}'"
        ))
    })?;
    let txid = hex_to_bytes32(txid_hex)?;
    let vout = vout_str.parse::<u32>().map_err(|e| {
        Error::TradeRouting(format!(
            "invalid outpoint vout in {field}: '{vout_str}' ({e})"
        ))
    })?;
    Ok(LmsrInitialOutpoint { txid, vout })
}

fn canonical_outpoint_string(outpoint: LmsrInitialOutpoint) -> String {
    format!("{}:{}", hex::encode(outpoint.txid), outpoint.vout)
}

/// Parse a `DiscoveredOrder` into `(MakerOrderParams, maker_base_pubkey, order_nonce)`.
pub(crate) fn parse_discovered_order(
    order: &DiscoveredOrder,
) -> Result<(MakerOrderParams, [u8; 32], [u8; 32])> {
    let direction = match order.direction.as_str() {
        "sell-base" => OrderDirection::SellBase,
        "sell-quote" => OrderDirection::SellQuote,
        other => {
            return Err(Error::TradeRouting(format!(
                "unknown order direction: {other}"
            )));
        }
    };
    let maker_base_pubkey = hex_to_bytes32(&order.maker_base_pubkey)?;
    let order_nonce = hex_to_bytes32(&order.order_nonce)?;

    let params = MakerOrderParams {
        base_asset_id: hex_to_bytes32(&order.base_asset_id)?,
        quote_asset_id: hex_to_bytes32(&order.quote_asset_id)?,
        price: order.price,
        min_fill_lots: order.min_fill_lots,
        min_remainder_lots: order.min_remainder_lots,
        direction,
        maker_receive_spk_hash: hex_to_bytes32(&order.maker_receive_spk_hash)?,
        cosigner_pubkey: hex_to_bytes32(&order.cosigner_pubkey)?,
        maker_pubkey: maker_base_pubkey,
    };
    Ok((params, maker_base_pubkey, order_nonce))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::PoolReserves;
    use crate::prediction_market::params::derive_market_id_from_assets;

    fn hex32(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    // ── hex_to_bytes32 ──────────────────────────────────────────────────

    #[test]
    fn hex_to_bytes32_valid() {
        let input = hex::encode([0xab; 32]);
        assert_eq!(hex_to_bytes32(&input).unwrap(), [0xab; 32]);
    }

    #[test]
    fn hex_to_bytes32_wrong_length() {
        let short = hex::encode([0x01; 16]);
        let err = hex_to_bytes32(&short).unwrap_err();
        assert!(err.to_string().contains("expected 32 bytes"));
    }

    #[test]
    fn hex_to_bytes32_invalid_hex() {
        let err = hex_to_bytes32("zzzz").unwrap_err();
        assert!(err.to_string().contains("invalid hex"));
    }

    fn sample_pool_for_network(network_tag: &str) -> DiscoveredPool {
        let creation_txid = hex32(0xaa);
        let mut pool = DiscoveredPool {
            id: "evt1".into(),
            market_id: derive_market_id_from_assets([0x01; 32], [0x02; 32]).to_string(),
            pool_id: "pool1".into(),
            yes_asset_id: hex32(0x01),
            no_asset_id: hex32(0x02),
            lbtc_asset_id: hex32(0x03),
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: hex32(0x06),
            reserves: PoolReserves {
                r_yes: 500,
                r_no: 500,
                r_lbtc: 1000,
            },
            creator_pubkey: "pubkey".into(),
            created_at: 0,
            creation_txid: creation_txid.clone(),
            lmsr_pool_id: hex32(0x99),
            lmsr_table_root: hex32(0x42),
            table_depth: 16,
            q_step_lots: 10,
            s_bias: 1000,
            s_max_index: 65_535,
            half_payout_sats: 5_000,
            current_s_index: 10_001,
            initial_reserve_outpoints: vec![
                format!("{creation_txid}:0"),
                format!("{creation_txid}:1"),
                format!("{creation_txid}:2"),
            ],
            witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".into(),
            table_manifest_hash: None,
            lmsr_table_values: None,
            nostr_event_json: None,
        };
        let announce = PoolAnnouncement {
            version: crate::discovery::pool::LMSR_POOL_ANNOUNCEMENT_VERSION,
            params: PoolParams {
                yes_asset_id: hex_to_bytes32(&pool.yes_asset_id).unwrap(),
                no_asset_id: hex_to_bytes32(&pool.no_asset_id).unwrap(),
                lbtc_asset_id: hex_to_bytes32(&pool.lbtc_asset_id).unwrap(),
                fee_bps: pool.fee_bps,
                min_r_yes: pool.min_r_yes,
                min_r_no: pool.min_r_no,
                min_r_collateral: pool.min_r_collateral,
                cosigner_pubkey: hex_to_bytes32(&pool.cosigner_pubkey).unwrap(),
            },
            market_id: pool.market_id.clone(),
            reserves: pool.reserves,
            creation_txid: pool.creation_txid.clone(),
            lmsr_pool_id: hex32(0x99),
            lmsr_table_root: pool.lmsr_table_root.clone(),
            table_depth: pool.table_depth,
            q_step_lots: pool.q_step_lots,
            s_bias: pool.s_bias,
            s_max_index: pool.s_max_index,
            half_payout_sats: pool.half_payout_sats,
            current_s_index: pool.current_s_index,
            initial_reserve_outpoints: pool.initial_reserve_outpoints.clone(),
            witness_schema_version: pool.witness_schema_version.clone(),
            table_manifest_hash: pool.table_manifest_hash.clone(),
            lmsr_table_values: pool.lmsr_table_values.clone(),
        };
        pool.lmsr_pool_id = derive_lmsr_pool_id_hex(&announce, network_tag).unwrap();
        pool
    }

    fn sample_pool() -> DiscoveredPool {
        sample_pool_for_network("liquid-testnet")
    }

    #[test]
    fn parse_lmsr_pool_success() {
        let mut pool = sample_pool();
        pool.lmsr_table_values = Some(vec![2_000, 2_010, 2_025, 2_050]);
        let expected_id = pool.lmsr_pool_id.clone();

        let parsed = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap();
        assert_eq!(parsed.lmsr_pool_id, expected_id);
        assert_eq!(parsed.current_s_index, 10_001);
        assert_eq!(parsed.params.table_depth, 16);
        assert_eq!(parsed.table_values, Some(vec![2_000, 2_010, 2_025, 2_050]));
        assert_eq!(parsed.initial_reserve_outpoints[0].vout, 0);
        assert_eq!(parsed.initial_reserve_outpoints[1].vout, 1);
        assert_eq!(parsed.initial_reserve_outpoints[2].vout, 2);
    }

    #[test]
    fn parse_lmsr_pool_missing_required_field() {
        let mut pool = sample_pool();
        pool.initial_reserve_outpoints = vec!["abc:0".into(), "abc:1".into()];

        let err = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap_err();
        assert!(
            err.to_string()
                .contains("expected 3 LMSR initial reserve outpoints")
        );
    }

    #[test]
    fn parse_lmsr_pool_invalid_outpoint_format() {
        let mut pool = sample_pool();
        let creation_txid = pool.creation_txid.clone();
        pool.initial_reserve_outpoints = vec![
            "not-an-outpoint".into(),
            format!("{creation_txid}:1"),
            format!("{creation_txid}:2"),
        ];

        let err = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap_err();
        assert!(err.to_string().contains("invalid outpoint"));
    }

    #[test]
    fn parse_lmsr_pool_rejects_anchor_txid_mismatch() {
        let mut pool = sample_pool();
        pool.initial_reserve_outpoints[1] = format!("{}:1", hex32(0xbb));
        let err = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap_err();
        assert!(
            err.to_string()
                .contains("initial_reserve_outpoints[1] txid must match creation_txid")
        );
    }

    #[test]
    fn parse_lmsr_pool_rejects_duplicate_anchor_tuples() {
        let mut pool = sample_pool();
        let creation_txid = pool.creation_txid.clone();
        pool.initial_reserve_outpoints = vec![
            format!("{creation_txid}:0"),
            format!("{creation_txid}:0"),
            format!("{creation_txid}:1"),
        ];
        let err = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap_err();
        assert!(
            err.to_string()
                .contains("duplicate LMSR initial reserve outpoint")
        );
    }

    #[test]
    fn parse_lmsr_pool_rejects_non_canonical_anchor_format() {
        let mut pool = sample_pool();
        let creation_txid = pool.creation_txid.clone();
        pool.initial_reserve_outpoints[2] = format!("{creation_txid}:02");
        let err = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap_err();
        assert!(
            err.to_string()
                .contains("must use canonical '<lowercase_txid>:<vout>' format")
        );
    }

    #[test]
    fn parse_lmsr_pool_rejects_network_mismatch() {
        let pool = sample_pool();
        let err = parse_discovered_lmsr_pool(&pool, "liquid-regtest").unwrap_err();
        assert!(
            err.to_string()
                .contains("lmsr_pool_id does not match canonical derived ID")
        );
    }

    #[test]
    fn parse_lmsr_pool_rejects_mismatched_market_id() {
        let mut pool = sample_pool();
        pool.market_id = hex32(0xff);
        let err = parse_discovered_lmsr_pool(&pool, "liquid-testnet").unwrap_err();
        assert!(
            err.to_string()
                .contains("market_id does not match canonical derived ID")
        );
    }

    #[test]
    fn parse_lmsr_pool_accepts_matching_network_variants() {
        let testnet_pool = sample_pool_for_network("liquid-testnet");
        assert!(parse_discovered_lmsr_pool(&testnet_pool, "liquid-testnet").is_ok());

        let mainnet_pool = sample_pool_for_network("liquid");
        assert!(parse_discovered_lmsr_pool(&mainnet_pool, "liquid").is_ok());

        let regtest_pool = sample_pool_for_network("liquid-regtest");
        assert!(parse_discovered_lmsr_pool(&regtest_pool, "liquid-regtest").is_ok());
    }

    // ── parse_discovered_order ──────────────────────────────────────────

    fn sample_order() -> DiscoveredOrder {
        DiscoveredOrder {
            id: "ord1".into(),
            market_id: "mkt1".into(),
            base_asset_id: hex32(0x01),
            quote_asset_id: hex32(0x02),
            price: 50_000,
            min_fill_lots: 1,
            min_remainder_lots: 1,
            direction: "sell-base".into(),
            direction_label: "sell-yes".into(),
            maker_base_pubkey: hex32(0xaa),
            order_nonce: hex32(0xbb),
            covenant_address: "tex1qtest".into(),
            offered_amount: 100,
            cosigner_pubkey: hex32(0xcc),
            maker_receive_spk_hash: hex32(0xdd),
            creator_pubkey: "pk".into(),
            created_at: 0,
            nostr_event_json: None,
        }
    }

    #[test]
    fn parse_order_sell_base() {
        let order = sample_order();
        let (params, maker_pk, nonce) = parse_discovered_order(&order).unwrap();
        assert_eq!(params.direction, OrderDirection::SellBase);
        assert_eq!(params.price, 50_000);
        assert_eq!(maker_pk, [0xaa; 32]);
        assert_eq!(nonce, [0xbb; 32]);
    }

    #[test]
    fn parse_order_sell_quote() {
        let mut order = sample_order();
        order.direction = "sell-quote".into();
        let (params, _, _) = parse_discovered_order(&order).unwrap();
        assert_eq!(params.direction, OrderDirection::SellQuote);
    }

    #[test]
    fn parse_order_unknown_direction() {
        let mut order = sample_order();
        order.direction = "buy-base".into();
        let err = parse_discovered_order(&order).unwrap_err();
        assert!(err.to_string().contains("unknown order direction"));
    }

    #[test]
    fn parse_order_bad_hex_nonce() {
        let mut order = sample_order();
        order.order_nonce = "short".into();
        assert!(parse_discovered_order(&order).is_err());
    }
}
