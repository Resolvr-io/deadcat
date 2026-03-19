use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::announcement::{CONTRACT_ANNOUNCEMENT_VERSION, ContractAnnouncement};
use crate::discovery::store_trait::{ContractMetadataInput, PredictionMarketCandidateIngestInput};
use crate::network::Network;
use crate::prediction_market::anchor::{PredictionMarketAnchor, parse_prediction_market_anchor};
use crate::prediction_market_scan::validate_prediction_market_creation_tx;

use super::{APP_EVENT_KIND, CONTRACT_TAG, DEFAULT_RELAYS, bytes_to_hex};

/// What the frontend receives — maps to existing Market type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredMarket {
    pub id: String,
    pub nevent: String,
    pub market_id: String,
    pub question: String,
    pub category: String,
    pub description: String,
    pub resolution_source: String,
    pub oracle_pubkey: String,
    pub expiry_height: u32,
    pub cpt_sats: u64,
    pub collateral_asset_id: String,
    pub yes_asset_id: String,
    pub no_asset_id: String,
    pub yes_reissuance_token: String,
    pub no_reissuance_token: String,
    pub creator_pubkey: String,
    pub created_at: u64,
    pub anchor: PredictionMarketAnchor,
    pub state: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_event_json: Option<String>,
    /// Live YES probability in basis points (0–10000) from pool reserves, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yes_price_bps: Option<u16>,
    /// Live NO probability in basis points (0–10000) from pool reserves, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_price_bps: Option<u16>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedDiscoveredMarketAnnouncement {
    pub market: DiscoveredMarket,
    pub ingest: PredictionMarketCandidateIngestInput,
}

fn validate_market_announcement(
    announcement: &ContractAnnouncement,
) -> Result<(PredictionMarketAnchor, Vec<u8>), String> {
    let anchor = announcement.anchor.canonicalized()?;
    let _parsed_anchor = parse_prediction_market_anchor(&anchor)?;
    let raw_tx = hex::decode(&announcement.creation_tx_hex)
        .map_err(|e| format!("invalid creation_tx_hex: {e}"))?;
    let tx: crate::elements::Transaction = crate::elements::encode::deserialize(&raw_tx)
        .map_err(|e| format!("invalid creation_tx_hex transaction: {e}"))?;

    if tx.txid().to_string() != anchor.creation_txid {
        return Err("creation_tx_hex txid must equal anchor.creation_txid".to_string());
    }

    if !validate_prediction_market_creation_tx(&announcement.contract_params, &tx, &anchor)? {
        return Err(
            "creation_tx_hex is not a canonical prediction-market creation bootstrap".into(),
        );
    }

    Ok((anchor, raw_tx))
}

/// Build a Nostr event for a contract announcement.
pub fn build_announcement_event(
    keys: &Keys,
    announcement: &ContractAnnouncement,
    network_tag: &str,
) -> Result<Event, String> {
    let (anchor, raw_tx) = validate_market_announcement(announcement)?;
    network_tag
        .parse::<Network>()
        .map_err(|e| format!("unsupported network tag '{network_tag}': {e}"))?;
    let market_id = announcement.contract_params.market_id();
    let market_id_hex = bytes_to_hex(market_id.as_bytes());
    let category_lower = announcement.metadata.category.to_lowercase();

    let content = serde_json::to_string(&ContractAnnouncement {
        anchor,
        creation_tx_hex: hex::encode(raw_tx),
        ..announcement.clone()
    })
    .map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = vec![
        Tag::identifier(&market_id_hex),
        Tag::hashtag(CONTRACT_TAG),
        Tag::hashtag(&category_lower),
        Tag::custom(TagKind::custom("network"), vec![network_tag.to_string()]),
    ];

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
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

/// Build a Nostr filter for fetching contract announcements.
pub fn build_contract_filter() -> Filter {
    Filter::new().kind(APP_EVENT_KIND).hashtag(CONTRACT_TAG)
}

/// Parse a Nostr event into a DiscoveredMarket.
pub fn parse_announcement_event(
    event: &Event,
    expected_network_tag: &str,
) -> Result<DiscoveredMarket, String> {
    parse_announcement_event_with_ingest(event, expected_network_tag).map(|parsed| parsed.market)
}

pub(crate) fn parse_announcement_event_with_ingest(
    event: &Event,
    expected_network_tag: &str,
) -> Result<ParsedDiscoveredMarketAnnouncement, String> {
    expected_network_tag
        .parse::<Network>()
        .map_err(|e| format!("unsupported network tag '{expected_network_tag}': {e}"))?;
    let network_tag = event_network_tag(event)
        .ok_or_else(|| "missing network tag for contract announcement event".to_string())?;
    if network_tag != expected_network_tag {
        return Err(format!(
            "unsupported network tag for contract announcement event: {network_tag}"
        ));
    }
    let announcement: ContractAnnouncement = serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse announcement: {e}"))?;
    if announcement.version != CONTRACT_ANNOUNCEMENT_VERSION {
        return Err(format!(
            "unsupported contract announcement version {} (expected {})",
            announcement.version, CONTRACT_ANNOUNCEMENT_VERSION
        ));
    }
    let (anchor, raw_tx) = validate_market_announcement(&announcement)?;

    let params = &announcement.contract_params;
    let market_id = params.market_id();

    let nevent = Nip19Event::new(event.id, DEFAULT_RELAYS.iter().map(|r| r.to_string()))
        .to_bech32()
        .unwrap_or_default();

    let metadata = ContractMetadataInput {
        question: Some(announcement.metadata.question.clone()),
        description: Some(announcement.metadata.description.clone()),
        category: Some(announcement.metadata.category.clone()),
        resolution_source: Some(announcement.metadata.resolution_source.clone()),
        creator_pubkey: hex::decode(event.pubkey.to_hex()).ok(),
        anchor: anchor.clone(),
        nevent: Some(nevent.clone()),
        nostr_event_id: Some(event.id.to_hex()),
        nostr_event_json: serde_json::to_string(event).ok(),
    };

    Ok(ParsedDiscoveredMarketAnnouncement {
        market: DiscoveredMarket {
            id: event.id.to_hex(),
            nevent,
            market_id: bytes_to_hex(market_id.as_bytes()),
            question: announcement.metadata.question,
            category: announcement.metadata.category,
            description: announcement.metadata.description,
            resolution_source: announcement.metadata.resolution_source,
            oracle_pubkey: bytes_to_hex(&params.oracle_public_key),
            expiry_height: params.expiry_time,
            cpt_sats: params.collateral_per_token,
            collateral_asset_id: bytes_to_hex(&params.collateral_asset_id),
            yes_asset_id: bytes_to_hex(&params.yes_token_asset),
            no_asset_id: bytes_to_hex(&params.no_token_asset),
            yes_reissuance_token: bytes_to_hex(&params.yes_reissuance_token),
            no_reissuance_token: bytes_to_hex(&params.no_reissuance_token),
            creator_pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_u64(),
            anchor,
            state: 0, // Default Dormant for discovered markets
            nostr_event_json: serde_json::to_string(event).ok(),
            yes_price_bps: None,
            no_price_bps: None,
        },
        ingest: PredictionMarketCandidateIngestInput {
            params: *params,
            metadata,
            creation_tx: raw_tx,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elements::confidential::{Asset, Nonce, Value};
    use crate::elements::{AssetId, Transaction};
    use crate::testing::test_market_announcement;

    fn explicit_bootstrap_announcement(tag: u8) -> ContractAnnouncement {
        let (mut announcement, _params) = test_market_announcement([0xaa; 32], tag);
        let raw_tx = hex::decode(&announcement.creation_tx_hex).unwrap();
        let mut tx: Transaction = crate::elements::encode::deserialize(&raw_tx).unwrap();

        tx.output[0].asset = Asset::Explicit(
            AssetId::from_slice(&announcement.contract_params.yes_reissuance_token).unwrap(),
        );
        tx.output[0].value = Value::Explicit(1);
        tx.output[0].nonce = Nonce::Null;
        tx.output[1].asset = Asset::Explicit(
            AssetId::from_slice(&announcement.contract_params.no_reissuance_token).unwrap(),
        );
        tx.output[1].value = Value::Explicit(1);
        tx.output[1].nonce = Nonce::Null;

        announcement.anchor.creation_txid = tx.txid().to_string();
        announcement.creation_tx_hex = hex::encode(crate::elements::encode::serialize(&tx));
        announcement
    }

    #[test]
    fn contract_announcement_serde_roundtrip() {
        let (announcement, _params) = test_market_announcement([0xaa; 32], 0x11);

        let json = serde_json::to_string(&announcement).unwrap();
        let parsed: ContractAnnouncement = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, CONTRACT_ANNOUNCEMENT_VERSION);
        assert_eq!(parsed.contract_params, announcement.contract_params);
        assert_eq!(
            parsed.metadata.question,
            "Will BTC close above $120k by Dec 2026?"
        );
        assert_eq!(
            parsed.anchor.creation_txid,
            announcement.anchor.creation_txid
        );
        assert_eq!(parsed.creation_tx_hex, announcement.creation_tx_hex);
    }

    #[test]
    fn contract_filter_construction() {
        let filter = build_contract_filter();
        assert!(format!("{filter:?}").contains("30078"));
    }

    #[test]
    fn build_and_parse_announcement_event() {
        let keys = Keys::generate();
        let (announcement, _params) = test_market_announcement([0xaa; 32], 0x12);

        let event = build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap();
        let market = parse_announcement_event(&event, "liquid-testnet").unwrap();

        assert_eq!(market.question, "Will BTC close above $120k by Dec 2026?");
        assert_eq!(market.cpt_sats, 5000);
        assert_eq!(market.expiry_height, 3_650_000);
        assert_eq!(market.creator_pubkey, keys.public_key().to_hex());
        assert_eq!(
            market.anchor.creation_txid,
            announcement.anchor.creation_txid
        );
    }

    #[test]
    fn parse_announcement_event_rejects_network_mismatch() {
        let keys = Keys::generate();
        let (announcement, _params) = test_market_announcement([0xaa; 32], 0x13);

        let event = build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap();
        let err = parse_announcement_event(&event, "liquid-regtest").unwrap_err();
        assert!(
            err.contains("unsupported network tag for contract announcement event"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn build_announcement_event_rejects_invalid_anchor() {
        let keys = Keys::generate();
        let (mut announcement, _params) = test_market_announcement([0xaa; 32], 0x14);
        announcement.anchor.creation_txid = "not-a-txid".to_string();

        let err = build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("invalid creation_txid"));
    }

    #[test]
    fn build_announcement_event_rejects_invalid_yes_opening() {
        let keys = Keys::generate();
        let (mut announcement, _params) = test_market_announcement([0xaa; 32], 0x16);
        announcement
            .anchor
            .yes_dormant_opening
            .asset_blinding_factor = "not-hex".to_string();

        let err = build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("yes_dormant_opening.asset_blinding_factor"));
    }

    #[test]
    fn build_announcement_event_rejects_explicit_dormant_outputs() {
        let keys = Keys::generate();
        let announcement = explicit_bootstrap_announcement(0x18);

        let err = build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap_err();
        assert!(err.contains("canonical prediction-market creation bootstrap"));
    }

    #[test]
    fn parse_announcement_event_rejects_invalid_anchor() {
        let keys = Keys::generate();
        let (mut announcement, _params) = test_market_announcement([0xaa; 32], 0x15);
        announcement.anchor.creation_txid = "zz".repeat(32);

        let content = serde_json::to_string(&announcement).unwrap();
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier("deadbeef"),
                Tag::hashtag(CONTRACT_TAG),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();

        let err = parse_announcement_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("invalid creation_txid"));
    }

    #[test]
    fn parse_announcement_event_rejects_invalid_no_opening() {
        let keys = Keys::generate();
        let (mut announcement, _params) = test_market_announcement([0xaa; 32], 0x17);
        announcement.anchor.no_dormant_opening.value_blinding_factor = "FF".repeat(32);

        let content = serde_json::to_string(&announcement).unwrap();
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier("deadbeef"),
                Tag::hashtag(CONTRACT_TAG),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();

        let err = parse_announcement_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("no_dormant_opening.value_blinding_factor"));
    }

    #[test]
    fn parse_announcement_event_rejects_explicit_dormant_outputs() {
        let keys = Keys::generate();
        let announcement = explicit_bootstrap_announcement(0x19);

        let content = serde_json::to_string(&announcement).unwrap();
        let event = EventBuilder::new(APP_EVENT_KIND, &content)
            .tags(vec![
                Tag::identifier("deadbeef"),
                Tag::hashtag(CONTRACT_TAG),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .sign_with_keys(&keys)
            .unwrap();

        let err = parse_announcement_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("canonical prediction-market creation bootstrap"));
    }
}
