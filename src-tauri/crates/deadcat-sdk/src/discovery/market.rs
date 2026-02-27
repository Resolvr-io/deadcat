use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::announcement::ContractAnnouncement;

use super::{APP_EVENT_KIND, CONTRACT_TAG, DEFAULT_RELAYS, NETWORK_TAG, bytes_to_hex};

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
    pub starting_yes_price: u8,
    pub creator_pubkey: String,
    pub created_at: u64,
    pub creation_txid: Option<String>,
    pub state: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_event_json: Option<String>,
    /// Live YES probability in basis points (0–10000) from AMM pool reserves, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yes_price_bps: Option<u16>,
    /// Live NO probability in basis points (0–10000) from AMM pool reserves, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_price_bps: Option<u16>,
}

/// Build a Nostr event for a contract announcement.
pub fn build_announcement_event(
    keys: &Keys,
    announcement: &ContractAnnouncement,
) -> Result<Event, String> {
    let market_id = announcement.contract_params.market_id();
    let market_id_hex = bytes_to_hex(market_id.as_bytes());
    let category_lower = announcement.metadata.category.to_lowercase();

    let content =
        serde_json::to_string(announcement).map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = vec![
        Tag::identifier(&market_id_hex),
        Tag::hashtag(CONTRACT_TAG),
        Tag::hashtag(&category_lower),
        Tag::custom(TagKind::custom("network"), vec![NETWORK_TAG.to_string()]),
    ];

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
}

/// Build a Nostr filter for fetching contract announcements.
pub fn build_contract_filter() -> Filter {
    Filter::new().kind(APP_EVENT_KIND).hashtag(CONTRACT_TAG)
}

/// Parse a Nostr event into a DiscoveredMarket.
pub fn parse_announcement_event(event: &Event) -> Result<DiscoveredMarket, String> {
    let announcement: ContractAnnouncement = serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse announcement: {e}"))?;

    let params = &announcement.contract_params;
    let market_id = params.market_id();

    let nevent = Nip19Event::new(event.id, DEFAULT_RELAYS.iter().map(|r| r.to_string()))
        .to_bech32()
        .unwrap_or_default();

    Ok(DiscoveredMarket {
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
        starting_yes_price: announcement.metadata.starting_yes_price,
        creator_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_u64(),
        creation_txid: announcement.creation_txid,
        state: 0, // Default Dormant for discovered markets
        nostr_event_json: serde_json::to_string(event).ok(),
        yes_price_bps: None,
        no_price_bps: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PredictionMarketParams;
    use crate::announcement::ContractMetadata;

    fn test_metadata() -> ContractMetadata {
        ContractMetadata {
            question: "Will BTC hit 100k?".to_string(),
            description: "Resolves via exchange data.".to_string(),
            category: "Bitcoin".to_string(),
            resolution_source: "Exchange basket".to_string(),
            starting_yes_price: 55,
        }
    }

    fn test_params() -> PredictionMarketParams {
        PredictionMarketParams {
            oracle_public_key: [0xaa; 32],
            collateral_asset_id: [0xbb; 32],
            yes_token_asset: [0x01; 32],
            no_token_asset: [0x02; 32],
            yes_reissuance_token: [0x03; 32],
            no_reissuance_token: [0x04; 32],
            collateral_per_token: 5000,
            expiry_time: 3_650_000,
        }
    }

    #[test]
    fn contract_announcement_serde_roundtrip() {
        let announcement = ContractAnnouncement {
            version: 1,
            contract_params: test_params(),
            metadata: test_metadata(),
            creation_txid: Some("abc123".to_string()),
        };

        let json = serde_json::to_string(&announcement).unwrap();
        let parsed: ContractAnnouncement = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.contract_params, announcement.contract_params);
        assert_eq!(parsed.metadata.question, "Will BTC hit 100k?");
        assert_eq!(parsed.creation_txid, Some("abc123".to_string()));
    }

    #[test]
    fn contract_filter_construction() {
        let filter = build_contract_filter();
        assert!(format!("{filter:?}").contains("30078"));
    }

    #[test]
    fn build_and_parse_announcement_event() {
        let keys = Keys::generate();
        let announcement = ContractAnnouncement {
            version: 1,
            contract_params: test_params(),
            metadata: test_metadata(),
            creation_txid: None,
        };

        let event = build_announcement_event(&keys, &announcement).unwrap();
        let market = parse_announcement_event(&event).unwrap();

        assert_eq!(market.question, "Will BTC hit 100k?");
        assert_eq!(market.cpt_sats, 5000);
        assert_eq!(market.expiry_height, 3_650_000);
        assert_eq!(market.creator_pubkey, keys.public_key().to_hex());
    }
}
