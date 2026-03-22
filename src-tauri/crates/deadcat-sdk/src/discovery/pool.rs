use std::collections::{HashMap, HashSet};

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::lmsr_pool::identity::derive_lmsr_pool_id;
use crate::lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolId, LmsrPoolParams};
use crate::network::Network;
use crate::pool::PoolReserves;
use crate::prediction_market::params::derive_market_id_from_assets;

use super::{APP_EVENT_KIND, POOL_TAG, bytes_to_hex};

pub const LMSR_POOL_ANNOUNCEMENT_VERSION: u8 = 2;
pub const LMSR_WITNESS_SCHEMA_V2: &str = "DEADCAT/LMSR_WITNESS_SCHEMA_V2";

fn parse_hex32(label: &str, hex_str: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid {label} hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "invalid {label} length: expected 32 bytes, got {}",
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Parse a canonical LMSR initial reserve outpoint string.
///
/// The txid component is compared and persisted in raw lowercase hex byte
/// order; do not round-trip it through `Txid` byte-array helpers.
pub(crate) fn parse_canonical_lmsr_outpoint(
    outpoint: &str,
    label: &str,
) -> Result<LmsrInitialOutpoint, String> {
    let (txid_hex, vout_str) = outpoint
        .split_once(':')
        .ok_or_else(|| format!("invalid {label}: expected '<txid>:<vout>', got '{outpoint}'"))?;
    let txid = parse_hex32(label, txid_hex)?;
    let canonical_txid = hex::encode(txid);
    if txid_hex != canonical_txid {
        return Err(format!(
            "invalid {label} txid '{txid_hex}': expected canonical lowercase 32-byte hex"
        ));
    }
    let vout = vout_str
        .parse::<u32>()
        .map_err(|e| format!("invalid {label} vout '{vout_str}': {e}"))?;
    Ok(LmsrInitialOutpoint { txid, vout })
}

fn canonical_outpoint_string(outpoint: LmsrInitialOutpoint) -> String {
    format!("{}:{}", hex::encode(outpoint.txid), outpoint.vout)
}

fn canonical_lmsr_pool_id_hex(pool_id: &str) -> Result<String, String> {
    LmsrPoolId::from_hex(pool_id)
        .map(|id| id.to_hex())
        .map_err(|e| format!("invalid lmsr_pool_id: {e}"))
}

fn validate_market_id_hex(market_id: &str, params: &PoolParams) -> Result<String, String> {
    let parsed = parse_hex32("market_id", market_id)?;
    let canonical = hex::encode(parsed);
    if market_id != canonical {
        return Err("market_id must be canonical lowercase 32-byte hex".into());
    }
    let expected =
        derive_market_id_from_assets(params.yes_asset_id, params.no_asset_id).to_string();
    if market_id != expected {
        return Err(format!(
            "market_id does not match canonical derived ID: expected {expected}"
        ));
    }
    Ok(expected)
}

fn parse_network_tag(network_tag: &str) -> Result<Network, String> {
    network_tag
        .parse::<Network>()
        .map_err(|e| format!("unsupported network tag '{network_tag}': {e}"))
}

pub(crate) fn derive_lmsr_pool_id_hex(
    announcement: &PoolAnnouncement,
    network_tag: &str,
) -> Result<String, String> {
    if announcement.initial_reserve_outpoints.len() != 3 {
        return Err(format!(
            "expected 3 LMSR initial reserve outpoints, got {}",
            announcement.initial_reserve_outpoints.len()
        ));
    }
    let creation_txid = parse_hex32("creation_txid", &announcement.creation_txid)?;
    let params = LmsrPoolParams {
        yes_asset_id: announcement.params.yes_asset_id,
        no_asset_id: announcement.params.no_asset_id,
        collateral_asset_id: announcement.params.lbtc_asset_id,
        lmsr_table_root: parse_hex32("lmsr_table_root", &announcement.lmsr_table_root)?,
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
        .map_err(|e| format!("invalid LMSR params in pool announcement: {e}"))?;
    let network = parse_network_tag(network_tag)?;
    let initial_yes_outpoint = parse_canonical_lmsr_outpoint(
        &announcement.initial_reserve_outpoints[0],
        "initial_reserve_outpoints[0]",
    )?;
    let initial_no_outpoint = parse_canonical_lmsr_outpoint(
        &announcement.initial_reserve_outpoints[1],
        "initial_reserve_outpoints[1]",
    )?;
    let initial_collateral_outpoint = parse_canonical_lmsr_outpoint(
        &announcement.initial_reserve_outpoints[2],
        "initial_reserve_outpoints[2]",
    )?;
    derive_lmsr_pool_id(
        network,
        params,
        creation_txid,
        [
            initial_yes_outpoint,
            initial_no_outpoint,
            initial_collateral_outpoint,
        ],
    )
    .map(|id| id.to_hex())
    .map_err(|e| format!("failed to derive LMSR pool ID: {e}"))
}

fn event_identifier_tag(event: &Event) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "d" {
            Some(fields[1].to_string())
        } else {
            None
        }
    })
}

fn event_network_tag(event: &Event) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "network" {
            Some(fields[1].to_string())
        } else {
            None
        }
    })
}

/// Canonical LMSR pool params used in discovery announcements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolParams {
    pub yes_asset_id: [u8; 32],
    pub no_asset_id: [u8; 32],
    pub lbtc_asset_id: [u8; 32],
    pub fee_bps: u64,
    pub min_r_yes: u64,
    pub min_r_no: u64,
    pub min_r_collateral: u64,
    pub cosigner_pubkey: [u8; 32],
}

/// Published to Nostr — contains LMSR pool params + canonical discovery anchors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolAnnouncement {
    pub version: u8,
    pub params: PoolParams,
    pub market_id: String,
    pub reserves: PoolReserves,
    pub creation_txid: String,
    pub lmsr_pool_id: String,
    pub lmsr_table_root: String,
    pub table_depth: u32,
    pub q_step_lots: u64,
    pub s_bias: u64,
    pub s_max_index: u64,
    pub half_payout_sats: u64,
    pub current_s_index: u64,
    pub initial_reserve_outpoints: Vec<String>,
    pub witness_schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lmsr_table_values: Option<Vec<u64>>,
}

/// Parsed from a Nostr event — what traders consume for routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPool {
    pub id: String,
    pub market_id: String,
    pub pool_id: String,
    pub yes_asset_id: String,
    pub no_asset_id: String,
    pub lbtc_asset_id: String,
    pub fee_bps: u64,
    pub min_r_yes: u64,
    pub min_r_no: u64,
    pub min_r_collateral: u64,
    pub cosigner_pubkey: String,
    pub reserves: PoolReserves,
    pub creator_pubkey: String,
    pub created_at: u64,
    pub creation_txid: String,
    pub lmsr_pool_id: String,
    pub lmsr_table_root: String,
    pub table_depth: u32,
    pub q_step_lots: u64,
    pub s_bias: u64,
    pub s_max_index: u64,
    pub half_payout_sats: u64,
    pub current_s_index: u64,
    pub initial_reserve_outpoints: Vec<String>,
    pub witness_schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_manifest_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lmsr_table_values: Option<Vec<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_event_json: Option<String>,
}

/// Build a Nostr event for a pool announcement.
///
/// Uses NIP-33 replaceable events with `d` tag = canonical `lmsr_pool_id`.
pub fn build_pool_event(
    keys: &Keys,
    announcement: &PoolAnnouncement,
    network_tag: &str,
) -> Result<Event, String> {
    if announcement.version != LMSR_POOL_ANNOUNCEMENT_VERSION {
        return Err(format!(
            "unsupported pool announcement version: {} (expected {})",
            announcement.version, LMSR_POOL_ANNOUNCEMENT_VERSION
        ));
    }
    if announcement.lmsr_pool_id.trim().is_empty() {
        return Err("lmsr_pool_id cannot be empty".into());
    }
    let canonical_pool_id = canonical_lmsr_pool_id_hex(&announcement.lmsr_pool_id)?;
    if announcement.lmsr_pool_id != canonical_pool_id {
        return Err("lmsr_pool_id must be canonical lowercase 32-byte hex".into());
    }
    let canonical_market_id =
        validate_market_id_hex(&announcement.market_id, &announcement.params)?;
    let creation_txid = parse_hex32("creation_txid", &announcement.creation_txid)?;
    parse_hex32("lmsr_table_root", &announcement.lmsr_table_root)?;
    if announcement.initial_reserve_outpoints.len() != 3 {
        return Err(format!(
            "expected 3 LMSR initial reserve outpoints, got {}",
            announcement.initial_reserve_outpoints.len()
        ));
    }
    let mut seen_outpoints = HashSet::new();
    for (idx, outpoint) in announcement.initial_reserve_outpoints.iter().enumerate() {
        let label = format!("initial_reserve_outpoints[{idx}]");
        let parsed = parse_canonical_lmsr_outpoint(outpoint, &label)?;
        let canonical = canonical_outpoint_string(parsed);
        if outpoint != &canonical {
            return Err(format!(
                "{label} must use canonical '<lowercase_txid>:<vout>' formatting"
            ));
        }
        if parsed.txid != creation_txid {
            return Err(format!(
                "{label} txid must equal creation_txid for canonical LMSR anchors"
            ));
        }
        if !seen_outpoints.insert(parsed) {
            return Err("duplicate LMSR initial reserve outpoint".into());
        }
    }
    if announcement.witness_schema_version != LMSR_WITNESS_SCHEMA_V2 {
        return Err(format!(
            "unsupported witness schema version: {}",
            announcement.witness_schema_version
        ));
    }
    parse_network_tag(network_tag)?;
    let derived_pool_id = derive_lmsr_pool_id_hex(announcement, network_tag)?;
    if canonical_pool_id != derived_pool_id {
        return Err(format!(
            "lmsr_pool_id does not match canonical derived ID: expected {derived_pool_id}"
        ));
    }

    let content =
        serde_json::to_string(announcement).map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = vec![
        Tag::identifier(&canonical_pool_id),
        Tag::hashtag(POOL_TAG),
        Tag::hashtag(&canonical_market_id),
        Tag::custom(TagKind::custom("network"), vec![network_tag.to_string()]),
    ];

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
}

/// Build a Nostr filter for fetching pool announcements.
///
/// If `market_id_hex` is provided, filters to pools for that specific market.
pub fn build_pool_filter(market_id_hex: Option<&str>) -> Filter {
    let mut filter = Filter::new().kind(APP_EVENT_KIND).hashtag(POOL_TAG);

    if let Some(market_id) = market_id_hex {
        filter = filter.hashtag(market_id);
    }

    filter
}

/// Parse a Nostr event into a `DiscoveredPool`.
pub fn parse_pool_event(
    event: &Event,
    expected_network_tag: &str,
) -> Result<DiscoveredPool, String> {
    let announcement: PoolAnnouncement = serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse pool announcement: {e}"))?;

    if announcement.version != LMSR_POOL_ANNOUNCEMENT_VERSION {
        return Err(format!(
            "unsupported pool announcement version: {} (expected {})",
            announcement.version, LMSR_POOL_ANNOUNCEMENT_VERSION
        ));
    }

    if announcement.lmsr_pool_id.trim().is_empty() {
        return Err("missing required LMSR field: lmsr_pool_id".into());
    }
    let canonical_pool_id = canonical_lmsr_pool_id_hex(&announcement.lmsr_pool_id)?;
    let canonical_market_id =
        validate_market_id_hex(&announcement.market_id, &announcement.params)?;
    if announcement.creation_txid.trim().is_empty() {
        return Err("missing required LMSR field: creation_txid".into());
    }
    let creation_txid = parse_hex32("creation_txid", &announcement.creation_txid)?;
    parse_hex32("lmsr_table_root", &announcement.lmsr_table_root)?;
    if announcement.initial_reserve_outpoints.len() != 3 {
        return Err(format!(
            "expected 3 LMSR initial reserve outpoints, got {}",
            announcement.initial_reserve_outpoints.len()
        ));
    }
    let mut seen_outpoints = HashSet::new();
    for (idx, outpoint) in announcement.initial_reserve_outpoints.iter().enumerate() {
        let label = format!("initial_reserve_outpoints[{idx}]");
        let parsed = parse_canonical_lmsr_outpoint(outpoint, &label)?;
        let canonical = canonical_outpoint_string(parsed);
        if outpoint != &canonical {
            return Err(format!(
                "{label} must use canonical '<lowercase_txid>:<vout>' formatting"
            ));
        }
        if parsed.txid != creation_txid {
            return Err(format!(
                "{label} txid must equal creation_txid for canonical LMSR anchors"
            ));
        }
        if !seen_outpoints.insert(parsed) {
            return Err("duplicate LMSR initial reserve outpoint".into());
        }
    }
    if announcement.witness_schema_version != LMSR_WITNESS_SCHEMA_V2 {
        return Err(format!(
            "unsupported witness schema version: {}",
            announcement.witness_schema_version
        ));
    }
    parse_network_tag(expected_network_tag)?;
    let network_tag = event_network_tag(event)
        .ok_or_else(|| "missing required network tag for LMSR pool event".to_string())?;
    if network_tag != expected_network_tag {
        return Err(format!(
            "unsupported network tag for LMSR pool event: {network_tag}"
        ));
    }
    let derived_pool_id = derive_lmsr_pool_id_hex(&announcement, &network_tag)?;
    if canonical_pool_id != derived_pool_id {
        return Err(format!(
            "lmsr_pool_id does not match canonical derived ID: expected {derived_pool_id}"
        ));
    }
    let d_tag = event_identifier_tag(event)
        .ok_or_else(|| "missing required NIP-33 d tag for pool announcement".to_string())?;
    let canonical_d_tag = canonical_lmsr_pool_id_hex(&d_tag)?;
    if canonical_d_tag != derived_pool_id {
        return Err("NIP-33 d tag must match canonical lmsr_pool_id".into());
    }

    Ok(DiscoveredPool {
        id: event.id.to_hex(),
        market_id: canonical_market_id,
        pool_id: derived_pool_id.clone(),
        yes_asset_id: bytes_to_hex(&announcement.params.yes_asset_id),
        no_asset_id: bytes_to_hex(&announcement.params.no_asset_id),
        lbtc_asset_id: bytes_to_hex(&announcement.params.lbtc_asset_id),
        fee_bps: announcement.params.fee_bps,
        min_r_yes: announcement.params.min_r_yes,
        min_r_no: announcement.params.min_r_no,
        min_r_collateral: announcement.params.min_r_collateral,
        cosigner_pubkey: bytes_to_hex(&announcement.params.cosigner_pubkey),
        reserves: announcement.reserves,
        creator_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_u64(),
        creation_txid: announcement.creation_txid,
        lmsr_pool_id: derived_pool_id,
        lmsr_table_root: announcement.lmsr_table_root,
        table_depth: announcement.table_depth,
        q_step_lots: announcement.q_step_lots,
        s_bias: announcement.s_bias,
        s_max_index: announcement.s_max_index,
        half_payout_sats: announcement.half_payout_sats,
        current_s_index: announcement.current_s_index,
        initial_reserve_outpoints: announcement.initial_reserve_outpoints,
        witness_schema_version: announcement.witness_schema_version,
        table_manifest_hash: announcement.table_manifest_hash,
        lmsr_table_values: announcement.lmsr_table_values,
        nostr_event_json: None,
    })
}

/// Fetch pool announcements from relays.
#[allow(dead_code)]
pub async fn fetch_pools(
    client: &Client,
    market_id_hex: Option<&str>,
    expected_network_tag: &str,
) -> Result<Vec<DiscoveredPool>, String> {
    let filter = build_pool_filter(market_id_hex);
    let events = client
        .fetch_events(vec![filter], std::time::Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch pool events: {e}"))?;

    let mut pools = Vec::new();
    for event in events.iter() {
        match parse_pool_event(event, expected_network_tag) {
            Ok(pool) => pools.push(pool),
            Err(e) => {
                log::warn!("skipping unparseable pool event {}: {e}", event.id);
            }
        }
    }

    let mut dedup: HashMap<String, DiscoveredPool> = HashMap::new();
    for pool in pools {
        match dedup.get_mut(&pool.lmsr_pool_id) {
            None => {
                dedup.insert(pool.lmsr_pool_id.clone(), pool);
            }
            Some(existing) => {
                let should_replace = pool.created_at > existing.created_at
                    || (pool.created_at == existing.created_at && pool.id > existing.id);
                if should_replace {
                    *existing = pool;
                }
            }
        }
    }
    let mut pools: Vec<_> = dedup.into_values().collect();
    pools.sort_by(|a, b| a.lmsr_pool_id.cmp(&b.lmsr_pool_id));
    Ok(pools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmsr_pool::table::lmsr_table_root;
    use crate::prediction_market::params::derive_market_id_from_assets;
    use crate::taproot::NUMS_KEY_BYTES;

    fn test_pool_params() -> PoolParams {
        PoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    fn test_announcement(network_tag: &str) -> PoolAnnouncement {
        let creation_txid = hex::encode([0xdd; 32]);
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let table_root = lmsr_table_root(&table_values).unwrap();
        let mut announcement = PoolAnnouncement {
            version: LMSR_POOL_ANNOUNCEMENT_VERSION,
            params: test_pool_params(),
            market_id: derive_market_id_from_assets([0x01; 32], [0x02; 32]).to_string(),
            reserves: PoolReserves {
                r_yes: 500_000,
                r_no: 500_000,
                r_lbtc: 250_000,
            },
            creation_txid: creation_txid.clone(),
            lmsr_pool_id: hex::encode([0x44; 32]),
            lmsr_table_root: hex::encode(table_root),
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            current_s_index: 4,
            initial_reserve_outpoints: vec![
                format!("{creation_txid}:0"),
                format!("{creation_txid}:1"),
                format!("{creation_txid}:2"),
            ],
            witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
            table_manifest_hash: Some(hex::encode([0xaa; 32])),
            lmsr_table_values: Some(table_values),
        };
        announcement.lmsr_pool_id = derive_lmsr_pool_id_hex(&announcement, network_tag).unwrap();
        announcement
    }

    #[test]
    fn pool_announcement_serde_roundtrip() {
        let announcement = test_announcement("liquid-testnet");
        let json = serde_json::to_string(&announcement).unwrap();
        let parsed: PoolAnnouncement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, LMSR_POOL_ANNOUNCEMENT_VERSION);
        assert_eq!(parsed.params, announcement.params);
        assert_eq!(parsed.market_id, announcement.market_id);
        assert_eq!(parsed.lmsr_pool_id, announcement.lmsr_pool_id);
        assert_eq!(parsed.reserves.r_yes, 500_000);
    }

    #[test]
    fn build_and_parse_pool_event() {
        let keys = Keys::generate();
        let network_tag = "liquid-testnet";
        let announcement = test_announcement(network_tag);
        let event = build_pool_event(&keys, &announcement, network_tag).unwrap();

        let discovered = parse_pool_event(&event, network_tag).unwrap();
        assert_eq!(discovered.market_id, announcement.market_id);
        assert_eq!(discovered.fee_bps, 30);
        assert_eq!(discovered.pool_id, announcement.lmsr_pool_id);
        assert_eq!(discovered.lmsr_pool_id, announcement.lmsr_pool_id);
        assert_eq!(discovered.reserves.r_yes, 500_000);
        assert_eq!(discovered.reserves.r_no, 500_000);
        assert_eq!(discovered.reserves.r_lbtc, 250_000);
        assert_eq!(discovered.creator_pubkey, keys.public_key().to_hex());
        assert_eq!(discovered.initial_reserve_outpoints.len(), 3);
    }

    #[test]
    fn build_pool_event_rejects_wrong_version() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        announcement.version = 1;
        let err = build_pool_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("unsupported pool announcement version"));
    }

    #[test]
    fn build_pool_event_rejects_non_canonical_pool_id() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        announcement.lmsr_pool_id = hex::encode([0xab; 32]).to_uppercase();
        let err = build_pool_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("canonical lowercase"));
    }

    #[test]
    fn build_pool_event_rejects_mismatched_market_id() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        announcement.market_id = hex::encode([0xab; 32]);
        let err = build_pool_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("market_id does not match canonical derived ID"));
    }

    #[test]
    fn build_pool_event_rejects_missing_anchors() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        announcement.initial_reserve_outpoints = vec![];
        let err = build_pool_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("expected 3 LMSR initial reserve outpoints"));
    }

    #[test]
    fn build_pool_event_rejects_duplicate_anchor_tuples() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        let txid = announcement.creation_txid.clone();
        announcement.initial_reserve_outpoints = vec![
            format!("{txid}:0"),
            format!("{txid}:0"),
            format!("{txid}:1"),
        ];
        let err = build_pool_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("duplicate LMSR initial reserve outpoint"));
    }

    #[test]
    fn build_pool_event_rejects_non_canonical_anchor_format() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        let txid = announcement.creation_txid.clone();
        announcement.initial_reserve_outpoints[1] = format!("{txid}:01");
        let err = build_pool_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("canonical '<lowercase_txid>:<vout>'"));
    }

    #[test]
    fn parse_pool_event_rejects_mismatched_d_tag() {
        let keys = Keys::generate();
        let announcement = test_announcement("liquid-testnet");
        let content = serde_json::to_string(&announcement).unwrap();
        let wrong_pool_id = hex::encode([0x33; 32]);
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier(&wrong_pool_id),
                Tag::hashtag("deadcat-pool"),
                Tag::hashtag(&announcement.market_id),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = parse_pool_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("NIP-33 d tag must match canonical lmsr_pool_id"));
    }

    #[test]
    fn parse_pool_event_rejects_duplicate_anchor_tuples() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        let txid = announcement.creation_txid.clone();
        announcement.initial_reserve_outpoints = vec![
            format!("{txid}:0"),
            format!("{txid}:0"),
            format!("{txid}:1"),
        ];
        let content = serde_json::to_string(&announcement).unwrap();
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier(&announcement.lmsr_pool_id),
                Tag::hashtag("deadcat-pool"),
                Tag::hashtag(&announcement.market_id),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = parse_pool_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("duplicate LMSR initial reserve outpoint"));
    }

    #[test]
    fn parse_pool_event_rejects_non_canonical_anchor_format() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        let txid = announcement.creation_txid.clone();
        announcement.initial_reserve_outpoints[2] = format!("{txid}:02");
        let content = serde_json::to_string(&announcement).unwrap();
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier(&announcement.lmsr_pool_id),
                Tag::hashtag("deadcat-pool"),
                Tag::hashtag(&announcement.market_id),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = parse_pool_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("canonical '<lowercase_txid>:<vout>'"));
    }

    #[test]
    fn parse_pool_event_rejects_network_mismatch() {
        let keys = Keys::generate();
        let announcement = test_announcement("liquid-testnet");
        let event =
            build_pool_event(&keys, &announcement, "liquid-testnet").expect("build pool event");
        let err = parse_pool_event(&event, "liquid-regtest").unwrap_err();
        assert!(err.contains("unsupported network tag"));
    }

    #[test]
    fn parse_pool_event_rejects_mismatched_market_id() {
        let keys = Keys::generate();
        let mut announcement = test_announcement("liquid-testnet");
        announcement.market_id = hex::encode([0xab; 32]);
        let content = serde_json::to_string(&announcement).unwrap();
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier(&announcement.lmsr_pool_id),
                Tag::hashtag("deadcat-pool"),
                Tag::hashtag(&announcement.market_id),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = parse_pool_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("market_id does not match canonical derived ID"));
    }

    #[test]
    fn derived_pool_id_is_network_specific() {
        let announcement_testnet = test_announcement("liquid-testnet");

        let testnet_id = derive_lmsr_pool_id_hex(&announcement_testnet, "liquid-testnet").unwrap();
        let mainnet_id = derive_lmsr_pool_id_hex(&announcement_testnet, "liquid").unwrap();
        let regtest_id = derive_lmsr_pool_id_hex(&announcement_testnet, "liquid-regtest").unwrap();

        assert_ne!(testnet_id, mainnet_id);
        assert_ne!(testnet_id, regtest_id);
        assert_ne!(mainnet_id, regtest_id);
    }

    #[test]
    fn pool_filter_without_market() {
        let filter = build_pool_filter(None);
        let debug = format!("{filter:?}");
        assert!(debug.contains("30078"));
    }

    #[test]
    fn pool_filter_with_market() {
        let filter = build_pool_filter(Some("abcd1234"));
        let debug = format!("{filter:?}");
        assert!(debug.contains("abcd1234"));
    }
}
