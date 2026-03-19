use crate::discovery::pool::{
    DiscoveredPool, LMSR_POOL_ANNOUNCEMENT_VERSION, LMSR_WITNESS_SCHEMA_V2, PoolAnnouncement,
    PoolParams,
};
use crate::elements::{OutPoint, Txid};
use crate::lmsr_pool::identity::{derive_lmsr_market_id, validate_initial_reserve_outpoints};
use crate::lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolId, LmsrPoolParams};
use crate::lmsr_pool::table::LmsrTableManifest;
use crate::pool::PoolReserves;
use crate::prediction_market::params::{MarketId, PredictionMarketParams};

/// LMSR scan inputs plus the caller-supplied pool identity hint.
///
/// The fields here are network-agnostic. `DeadcatNode::scan_lmsr_pool` validates
/// canonical `market_id` plus the network-bound canonical `pool_id` before
/// returning a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LmsrPoolLocator {
    pub market_id: MarketId,
    pub pool_id: LmsrPoolId,
    pub params: LmsrPoolParams,
    pub creation_txid: Txid,
    pub initial_reserve_outpoints: [LmsrInitialOutpoint; 3],
    pub hinted_s_index: u64,
    pub witness_schema_version: String,
}

/// Canonical LMSR pool state at a specific chain tip.
///
/// Snapshots returned by `DeadcatNode` have validated canonical `market_id` and
/// network-validated canonical `locator.pool_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LmsrPoolSnapshot {
    pub locator: LmsrPoolLocator,
    pub current_s_index: u64,
    pub reserves: PoolReserves,
    pub current_reserve_outpoints: [OutPoint; 3],
    pub last_transition_txid: Option<Txid>,
}

/// High-level request for bootstrapping a new LMSR reserve bundle on-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateLmsrPoolRequest {
    pub market_params: PredictionMarketParams,
    pub pool_params: LmsrPoolParams,
    pub initial_s_index: u64,
    pub initial_reserves: PoolReserves,
    pub table_values: Vec<u64>,
    pub fee_amount: u64,
}

/// Result returned after a successful on-chain LMSR pool bootstrap.
#[derive(Debug, Clone)]
pub struct CreateLmsrPoolResult {
    pub txid: Txid,
    pub snapshot: LmsrPoolSnapshot,
    pub announcement: PoolAnnouncement,
}

impl TryFrom<&PoolAnnouncement> for LmsrPoolLocator {
    type Error = String;

    fn try_from(announcement: &PoolAnnouncement) -> Result<Self, Self::Error> {
        if announcement.version != LMSR_POOL_ANNOUNCEMENT_VERSION {
            return Err(format!(
                "unsupported pool announcement version: {} (expected {})",
                announcement.version, LMSR_POOL_ANNOUNCEMENT_VERSION
            ));
        }
        let pool_id = parse_pool_id(&announcement.lmsr_pool_id)?;
        let params = announcement_params(announcement)?;
        let market_id = parse_market_id(&announcement.market_id, params)?;
        if announcement.current_s_index > params.s_max_index {
            return Err(format!(
                "current_s_index {} exceeds s_max_index {}",
                announcement.current_s_index, params.s_max_index
            ));
        }
        if announcement.witness_schema_version != LMSR_WITNESS_SCHEMA_V2 {
            return Err(format!(
                "unsupported witness schema version: {}",
                announcement.witness_schema_version
            ));
        }

        let creation_txid = parse_txid("creation_txid", &announcement.creation_txid)?;
        let creation_txid_bytes = txid_to_canonical_bytes(&creation_txid)?;
        let initial_reserve_outpoints =
            parse_initial_reserve_outpoints(&announcement.initial_reserve_outpoints)?;
        validate_initial_reserve_outpoints(initial_reserve_outpoints, creation_txid_bytes)?;

        Ok(Self {
            market_id,
            pool_id,
            params,
            creation_txid,
            initial_reserve_outpoints,
            hinted_s_index: announcement.current_s_index,
            witness_schema_version: announcement.witness_schema_version.clone(),
        })
    }
}

impl TryFrom<&DiscoveredPool> for LmsrPoolLocator {
    type Error = String;

    fn try_from(pool: &DiscoveredPool) -> Result<Self, Self::Error> {
        let announcement = PoolAnnouncement {
            version: LMSR_POOL_ANNOUNCEMENT_VERSION,
            params: PoolParams {
                yes_asset_id: decode_hex32("yes_asset_id", &pool.yes_asset_id)?,
                no_asset_id: decode_hex32("no_asset_id", &pool.no_asset_id)?,
                lbtc_asset_id: decode_hex32("lbtc_asset_id", &pool.lbtc_asset_id)?,
                fee_bps: pool.fee_bps,
                min_r_yes: pool.min_r_yes,
                min_r_no: pool.min_r_no,
                min_r_collateral: pool.min_r_collateral,
                cosigner_pubkey: decode_hex32("cosigner_pubkey", &pool.cosigner_pubkey)?,
            },
            market_id: pool.market_id.clone(),
            reserves: pool.reserves,
            creation_txid: pool.creation_txid.clone(),
            lmsr_pool_id: pool.lmsr_pool_id.clone(),
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
        Self::try_from(&announcement)
    }
}

/// Build a publishable pool announcement from a canonical LMSR snapshot.
///
/// Prefer snapshots returned by `DeadcatNode`; node-side scan validates the
/// locator's canonical `market_id` and network-bound `pool_id` before this
/// helper is normally used.
pub fn build_pool_announcement_from_snapshot(
    snapshot: &LmsrPoolSnapshot,
    table_values: Vec<u64>,
) -> Result<PoolAnnouncement, String> {
    let expected_market_id = derive_lmsr_market_id(snapshot.locator.params);
    if snapshot.locator.market_id != expected_market_id {
        return Err(format!(
            "snapshot market_id {} does not match canonical market_id {}",
            snapshot.locator.market_id, expected_market_id
        ));
    }
    if snapshot.locator.witness_schema_version != LMSR_WITNESS_SCHEMA_V2 {
        return Err(format!(
            "unsupported witness schema version: {}",
            snapshot.locator.witness_schema_version
        ));
    }
    if snapshot.current_s_index > snapshot.locator.params.s_max_index {
        return Err(format!(
            "current_s_index {} exceeds s_max_index {}",
            snapshot.current_s_index, snapshot.locator.params.s_max_index
        ));
    }

    let manifest = LmsrTableManifest::new(snapshot.locator.params.table_depth, table_values)
        .map_err(|e| e.to_string())?;
    manifest
        .verify_matches_pool_params(&snapshot.locator.params)
        .map_err(|e| e.to_string())?;

    Ok(PoolAnnouncement {
        version: LMSR_POOL_ANNOUNCEMENT_VERSION,
        params: PoolParams {
            yes_asset_id: snapshot.locator.params.yes_asset_id,
            no_asset_id: snapshot.locator.params.no_asset_id,
            lbtc_asset_id: snapshot.locator.params.collateral_asset_id,
            fee_bps: snapshot.locator.params.fee_bps,
            min_r_yes: snapshot.locator.params.min_r_yes,
            min_r_no: snapshot.locator.params.min_r_no,
            min_r_collateral: snapshot.locator.params.min_r_collateral,
            cosigner_pubkey: snapshot.locator.params.cosigner_pubkey,
        },
        market_id: snapshot.locator.market_id.to_string(),
        reserves: snapshot.reserves,
        creation_txid: snapshot.locator.creation_txid.to_string(),
        lmsr_pool_id: snapshot.locator.pool_id.to_hex(),
        lmsr_table_root: hex::encode(snapshot.locator.params.lmsr_table_root),
        table_depth: snapshot.locator.params.table_depth,
        q_step_lots: snapshot.locator.params.q_step_lots,
        s_bias: snapshot.locator.params.s_bias,
        s_max_index: snapshot.locator.params.s_max_index,
        half_payout_sats: snapshot.locator.params.half_payout_sats,
        current_s_index: snapshot.current_s_index,
        initial_reserve_outpoints: snapshot
            .locator
            .initial_reserve_outpoints
            .iter()
            .map(|outpoint| canonical_outpoint_string(*outpoint))
            .collect(),
        witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
        table_manifest_hash: None,
        lmsr_table_values: Some(manifest.values),
    })
}

pub(crate) fn txid_to_canonical_bytes(txid: &Txid) -> Result<[u8; 32], String> {
    decode_hex32("txid", &txid.to_string())
}

fn decode_hex32(label: &str, hex_str: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid {label} hex: {e}"))?;
    let len = bytes.len();
    bytes
        .try_into()
        .map_err(|_| format!("invalid {label} length: expected 32 bytes, got {len}"))
}

fn parse_market_id(hex_str: &str, params: LmsrPoolParams) -> Result<MarketId, String> {
    let market_id = MarketId(decode_hex32("market_id", hex_str)?);
    let canonical = market_id.to_string();
    if hex_str != canonical {
        return Err("market_id must be canonical lowercase 32-byte hex".into());
    }
    let expected = derive_lmsr_market_id(params);
    if market_id != expected {
        return Err(format!(
            "market_id does not match canonical derived ID: expected {expected}"
        ));
    }
    Ok(market_id)
}

fn parse_pool_id(hex_str: &str) -> Result<LmsrPoolId, String> {
    let pool_id =
        LmsrPoolId::from_hex(hex_str).map_err(|e| format!("invalid lmsr_pool_id: {e}"))?;
    let canonical = pool_id.to_hex();
    if hex_str != canonical {
        return Err("lmsr_pool_id must be canonical lowercase 32-byte hex".into());
    }
    Ok(pool_id)
}

fn parse_txid(label: &str, txid: &str) -> Result<Txid, String> {
    txid.parse::<Txid>()
        .map_err(|e| format!("invalid {label} '{txid}': {e}"))
}

fn announcement_params(announcement: &PoolAnnouncement) -> Result<LmsrPoolParams, String> {
    let params = LmsrPoolParams {
        yes_asset_id: announcement.params.yes_asset_id,
        no_asset_id: announcement.params.no_asset_id,
        collateral_asset_id: announcement.params.lbtc_asset_id,
        lmsr_table_root: decode_hex32("lmsr_table_root", &announcement.lmsr_table_root)?,
        table_depth: announcement.table_depth,
        q_step_lots: announcement.q_step_lots,
        s_bias: announcement.s_bias,
        s_max_index: announcement.s_max_index,
        half_payout_sats: announcement.half_payout_sats,
        fee_bps: announcement.params.fee_bps,
        min_r_yes: announcement.params.min_r_yes,
        min_r_no: announcement.params.min_r_no,
        min_r_collateral: announcement.params.min_r_collateral,
        cosigner_pubkey: announcement.params.cosigner_pubkey,
    };
    params
        .validate()
        .map_err(|e| format!("invalid LMSR params: {e}"))?;
    Ok(params)
}

fn parse_initial_reserve_outpoints(
    outpoints: &[String],
) -> Result<[LmsrInitialOutpoint; 3], String> {
    if outpoints.len() != 3 {
        return Err(format!(
            "expected 3 LMSR initial reserve outpoints, got {}",
            outpoints.len()
        ));
    }
    Ok([
        parse_outpoint(&outpoints[0], "initial_reserve_outpoints[0]")?,
        parse_outpoint(&outpoints[1], "initial_reserve_outpoints[1]")?,
        parse_outpoint(&outpoints[2], "initial_reserve_outpoints[2]")?,
    ])
}

fn parse_outpoint(outpoint: &str, label: &str) -> Result<LmsrInitialOutpoint, String> {
    let (txid_hex, vout_str) = outpoint
        .split_once(':')
        .ok_or_else(|| format!("invalid {label}: expected '<txid>:<vout>', got '{outpoint}'"))?;
    let parsed = LmsrInitialOutpoint {
        txid: decode_hex32(label, txid_hex)?,
        vout: vout_str
            .parse::<u32>()
            .map_err(|e| format!("invalid {label} vout '{vout_str}': {e}"))?,
    };
    let canonical = canonical_outpoint_string(parsed);
    if outpoint != canonical {
        return Err(format!(
            "{label} must use canonical '<lowercase_txid>:<vout>' formatting"
        ));
    }
    Ok(parsed)
}

fn canonical_outpoint_string(outpoint: LmsrInitialOutpoint) -> String {
    format!("{}:{}", hex::encode(outpoint.txid), outpoint.vout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmsr_pool::identity::derive_lmsr_pool_id;
    use crate::lmsr_pool::table::lmsr_table_root;
    use crate::network::Network;
    use crate::taproot::NUMS_KEY_BYTES;

    fn sample_params() -> LmsrPoolParams {
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        LmsrPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            lmsr_table_root: lmsr_table_root(&table_values).unwrap(),
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    fn sample_locator() -> LmsrPoolLocator {
        let params = sample_params();
        let network = Network::Liquid;
        let market_params = PredictionMarketParams {
            oracle_public_key: [0x11; 32],
            collateral_asset_id: params.collateral_asset_id,
            yes_token_asset: params.yes_asset_id,
            no_token_asset: params.no_asset_id,
            yes_reissuance_token: [0x21; 32],
            no_reissuance_token: [0x22; 32],
            collateral_per_token: params.half_payout_sats,
            expiry_time: 123,
        };
        let creation_txid = hex::encode([0xaa; 32]).parse::<Txid>().unwrap();
        let initial_reserve_outpoints = [
            LmsrInitialOutpoint {
                txid: [0xaa; 32],
                vout: 0,
            },
            LmsrInitialOutpoint {
                txid: [0xaa; 32],
                vout: 1,
            },
            LmsrInitialOutpoint {
                txid: [0xaa; 32],
                vout: 2,
            },
        ];
        let pool_id = derive_lmsr_pool_id(
            network,
            params,
            txid_to_canonical_bytes(&creation_txid).unwrap(),
            initial_reserve_outpoints,
        )
        .unwrap();
        LmsrPoolLocator {
            market_id: market_params.market_id(),
            pool_id,
            params,
            creation_txid,
            initial_reserve_outpoints,
            hinted_s_index: 4,
            witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
        }
    }

    fn sample_snapshot() -> LmsrPoolSnapshot {
        let locator = sample_locator();
        let txid = locator.creation_txid;
        LmsrPoolSnapshot {
            locator,
            current_s_index: 4,
            reserves: PoolReserves {
                r_yes: 500,
                r_no: 400,
                r_lbtc: 1_000,
            },
            current_reserve_outpoints: [
                OutPoint::new(txid, 0),
                OutPoint::new(txid, 1),
                OutPoint::new(txid, 2),
            ],
            last_transition_txid: None,
        }
    }

    #[test]
    fn locator_try_from_pool_announcement_roundtrips() {
        let snapshot = sample_snapshot();
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let announcement = build_pool_announcement_from_snapshot(&snapshot, table_values).unwrap();

        let locator = LmsrPoolLocator::try_from(&announcement).unwrap();
        assert_eq!(locator.market_id, snapshot.locator.market_id);
        assert_eq!(locator.params, snapshot.locator.params);
        assert_eq!(locator.creation_txid, snapshot.locator.creation_txid);
        assert_eq!(
            locator.initial_reserve_outpoints,
            snapshot.locator.initial_reserve_outpoints
        );
        assert_eq!(locator.hinted_s_index, snapshot.current_s_index);
    }

    #[test]
    fn locator_try_from_discovered_pool_roundtrips() {
        let snapshot = sample_snapshot();
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let announcement =
            build_pool_announcement_from_snapshot(&snapshot, table_values.clone()).unwrap();
        let discovered = DiscoveredPool {
            id: "evt1".into(),
            market_id: announcement.market_id.clone(),
            pool_id: announcement.lmsr_pool_id.clone(),
            yes_asset_id: hex::encode(snapshot.locator.params.yes_asset_id),
            no_asset_id: hex::encode(snapshot.locator.params.no_asset_id),
            lbtc_asset_id: hex::encode(snapshot.locator.params.collateral_asset_id),
            fee_bps: snapshot.locator.params.fee_bps,
            min_r_yes: snapshot.locator.params.min_r_yes,
            min_r_no: snapshot.locator.params.min_r_no,
            min_r_collateral: snapshot.locator.params.min_r_collateral,
            cosigner_pubkey: hex::encode(snapshot.locator.params.cosigner_pubkey),
            reserves: snapshot.reserves,
            creator_pubkey: hex::encode([0x55; 32]),
            created_at: 0,
            creation_txid: announcement.creation_txid.clone(),
            lmsr_pool_id: announcement.lmsr_pool_id.clone(),
            lmsr_table_root: announcement.lmsr_table_root.clone(),
            table_depth: snapshot.locator.params.table_depth,
            q_step_lots: snapshot.locator.params.q_step_lots,
            s_bias: snapshot.locator.params.s_bias,
            s_max_index: snapshot.locator.params.s_max_index,
            half_payout_sats: snapshot.locator.params.half_payout_sats,
            current_s_index: snapshot.current_s_index,
            initial_reserve_outpoints: announcement.initial_reserve_outpoints.clone(),
            witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
            table_manifest_hash: None,
            lmsr_table_values: Some(table_values),
            nostr_event_json: None,
        };

        let locator = LmsrPoolLocator::try_from(&discovered).unwrap();
        assert_eq!(locator.pool_id, snapshot.locator.pool_id);
        assert_eq!(locator.hinted_s_index, snapshot.current_s_index);
    }

    #[test]
    fn locator_try_from_pool_announcement_rejects_mismatched_market_id() {
        let snapshot = sample_snapshot();
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let mut announcement =
            build_pool_announcement_from_snapshot(&snapshot, table_values).unwrap();
        announcement.market_id = hex::encode([0xff; 32]);

        let err = LmsrPoolLocator::try_from(&announcement).unwrap_err();
        assert!(err.contains("market_id does not match canonical derived ID"));
    }

    #[test]
    fn locator_try_from_discovered_pool_rejects_mismatched_market_id() {
        let snapshot = sample_snapshot();
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let announcement =
            build_pool_announcement_from_snapshot(&snapshot, table_values.clone()).unwrap();
        let mut discovered = DiscoveredPool {
            id: "evt1".into(),
            market_id: announcement.market_id.clone(),
            pool_id: announcement.lmsr_pool_id.clone(),
            yes_asset_id: hex::encode(snapshot.locator.params.yes_asset_id),
            no_asset_id: hex::encode(snapshot.locator.params.no_asset_id),
            lbtc_asset_id: hex::encode(snapshot.locator.params.collateral_asset_id),
            fee_bps: snapshot.locator.params.fee_bps,
            min_r_yes: snapshot.locator.params.min_r_yes,
            min_r_no: snapshot.locator.params.min_r_no,
            min_r_collateral: snapshot.locator.params.min_r_collateral,
            cosigner_pubkey: hex::encode(snapshot.locator.params.cosigner_pubkey),
            reserves: snapshot.reserves,
            creator_pubkey: hex::encode([0x55; 32]),
            created_at: 0,
            creation_txid: announcement.creation_txid.clone(),
            lmsr_table_root: announcement.lmsr_table_root.clone(),
            table_depth: snapshot.locator.params.table_depth,
            q_step_lots: snapshot.locator.params.q_step_lots,
            s_bias: snapshot.locator.params.s_bias,
            s_max_index: snapshot.locator.params.s_max_index,
            half_payout_sats: snapshot.locator.params.half_payout_sats,
            current_s_index: snapshot.current_s_index,
            initial_reserve_outpoints: announcement.initial_reserve_outpoints.clone(),
            witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
            lmsr_pool_id: announcement.lmsr_pool_id.clone(),
            table_manifest_hash: None,
            lmsr_table_values: Some(table_values),
            nostr_event_json: None,
        };
        discovered.market_id = hex::encode([0xff; 32]);

        let err = LmsrPoolLocator::try_from(&discovered).unwrap_err();
        assert!(err.contains("market_id does not match canonical derived ID"));
    }

    #[test]
    fn build_pool_announcement_from_snapshot_populates_canonical_fields() {
        let snapshot = sample_snapshot();
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let announcement =
            build_pool_announcement_from_snapshot(&snapshot, table_values.clone()).unwrap();

        assert_eq!(announcement.version, LMSR_POOL_ANNOUNCEMENT_VERSION);
        assert_eq!(
            announcement.market_id,
            snapshot.locator.market_id.to_string()
        );
        assert_eq!(
            announcement.creation_txid,
            snapshot.locator.creation_txid.to_string()
        );
        assert_eq!(
            announcement.initial_reserve_outpoints[0],
            format!("{}:0", hex::encode([0xaa; 32]))
        );
        assert_eq!(announcement.witness_schema_version, LMSR_WITNESS_SCHEMA_V2);
        assert_eq!(announcement.table_manifest_hash, None);
        assert_eq!(announcement.lmsr_table_values, Some(table_values));
    }

    #[test]
    fn build_pool_announcement_from_snapshot_rejects_bad_manifest() {
        let snapshot = sample_snapshot();
        let err = build_pool_announcement_from_snapshot(&snapshot, vec![1, 2, 3]).unwrap_err();
        assert!(err.contains("expects 8 values"));
    }

    #[test]
    fn build_pool_announcement_from_snapshot_rejects_mismatched_market_id() {
        let mut snapshot = sample_snapshot();
        snapshot.locator.market_id = MarketId([0xff; 32]);

        let err = build_pool_announcement_from_snapshot(
            &snapshot,
            vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175],
        )
        .unwrap_err();
        assert!(err.contains("snapshot market_id"));
    }
}
