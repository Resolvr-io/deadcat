//! NIP-57 zap request event builder and receipt aggregator.
//!
//! - `build_zap_request_event` constructs the unsigned kind:9734
//!   request the payer signs and sends to the recipient's LNURL-pay
//!   endpoint (see `utils-react/zap.ts` on the frontend side).
//! - `fetch_zap_summaries_for_events` subscribes to kind:9735 zap
//!   receipts on relays and aggregates per-event `{count, msats}`
//!   so clients can render live zap counters on comments / notes.

use std::collections::HashMap;
use std::time::Duration;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Kind for NIP-57 zap request events.
pub const ZAP_REQUEST_KIND: Kind = Kind::Custom(9734);

/// Kind for NIP-57 zap receipt events.
pub const ZAP_RECEIPT_KIND: Kind = Kind::Custom(9735);

/// Parameters needed to build an unsigned zap request.
pub struct ZapRequest<'a> {
    /// Hex pubkey of the recipient (who's being zapped).
    pub recipient_pubkey_hex: &'a str,
    /// Amount in millisats (1 sat = 1_000 msat).
    pub amount_msats: u64,
    /// Bech32 `lnurl` string from the recipient's profile, if available.
    /// NIP-57 recommends including it so the receipt references the
    /// same payment endpoint the payer used.
    pub lnurl: Option<&'a str>,
    /// Relays where the receiver's wallet should publish the receipt.
    pub relays: &'a [&'a str],
    /// Optional content — the zap comment. Empty string when omitted.
    pub content: &'a str,
    /// Optional event target — when zapping a specific event (comment,
    /// note, etc.). Encodes as an "e" tag.
    pub event_id_hex: Option<&'a str>,
    /// Optional addressable-event target (kind:pubkey:d), e.g. a
    /// market. Encodes as an "a" tag.
    pub event_coordinate: Option<&'a str>,
}

/// Build an UNSIGNED kind 9734 zap request. Caller signs via their
/// own `NostrSigner`.
pub fn build_zap_request_event(
    author: PublicKey,
    req: &ZapRequest<'_>,
) -> Result<UnsignedEvent, String> {
    if req.amount_msats == 0 {
        return Err("amount_msats must be greater than zero".to_string());
    }
    let recipient = PublicKey::from_hex(req.recipient_pubkey_hex)
        .map_err(|e| format!("invalid recipient pubkey: {e}"))?;

    let mut tags = vec![
        Tag::custom(
            TagKind::custom("relays"),
            req.relays.iter().map(|r| r.to_string()),
        ),
        Tag::custom(
            TagKind::custom("amount"),
            vec![req.amount_msats.to_string()],
        ),
        Tag::public_key(recipient),
    ];
    if let Some(lnurl) = req.lnurl {
        tags.push(Tag::custom(
            TagKind::custom("lnurl"),
            vec![lnurl.to_string()],
        ));
    }
    if let Some(event_id_hex) = req.event_id_hex {
        let event_id =
            EventId::from_hex(event_id_hex).map_err(|e| format!("invalid event id: {e}"))?;
        tags.push(Tag::event(event_id));
    }
    if let Some(coord) = req.event_coordinate {
        tags.push(Tag::custom(TagKind::custom("a"), vec![coord.to_string()]));
    }

    Ok(EventBuilder::new(ZAP_REQUEST_KIND, req.content)
        .tags(tags)
        .build(author))
}

/// Aggregated zap statistics for a single event (comment, note, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZapSummary {
    /// Hex event id the receipts referenced via the `e` tag.
    pub event_id: String,
    /// Number of kind:9735 receipts that referenced this event.
    pub count: u32,
    /// Sum of advertised amounts across all receipts, in millisats.
    /// Sourced from the embedded kind:9734 request's `amount` tag —
    /// this is what the payer declared. Not cryptographically verified
    /// against the paid bolt11 invoice, but matches what mainstream
    /// clients (Jumble, Primal, etc.) show and is good enough for a
    /// display counter.
    pub total_msats: u64,
}

/// Extract the msats amount from a kind:9735 receipt by parsing the
/// embedded kind:9734 zap request JSON stored in the `description`
/// tag. Returns `None` if the tag is missing or malformed.
fn receipt_amount_msats(receipt: &Event) -> Option<u64> {
    let description = receipt.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "description" {
            Some(fields[1].as_str())
        } else {
            None
        }
    })?;
    let parsed: serde_json::Value = serde_json::from_str(description).ok()?;
    let tags = parsed.get("tags")?.as_array()?;
    for tag in tags {
        let arr = tag.as_array()?;
        if arr.len() >= 2
            && arr[0].as_str() == Some("amount")
            && let Some(s) = arr[1].as_str()
        {
            return s.parse::<u64>().ok();
        }
    }
    None
}

/// Subscribe to kind:9735 zap receipts that reference any of the
/// given event ids and return a per-event summary.
///
/// Missing entries in the result mean no receipts were seen on the
/// provided client's relay pool within `timeout`. Callers typically
/// merge this with a zero-default so every comment renders a count,
/// even if it hasn't been zapped yet.
pub async fn fetch_zap_summaries_for_events(
    client: &Client,
    event_ids_hex: &[String],
    timeout: Duration,
) -> Result<Vec<ZapSummary>, String> {
    if event_ids_hex.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<EventId> = event_ids_hex
        .iter()
        .filter_map(|hex| EventId::from_hex(hex).ok())
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let filter = Filter::new().kind(ZAP_RECEIPT_KIND).events(ids);
    let events = client
        .fetch_events(vec![filter], timeout)
        .await
        .map_err(|e| format!("failed to fetch zap receipts: {e}"))?;

    let mut tally: HashMap<String, (u32, u64)> = HashMap::new();
    for receipt in events.iter() {
        let amount = receipt_amount_msats(receipt).unwrap_or(0);
        // A single receipt can reference multiple `e` tags in theory;
        // credit every referenced id so users get a full picture.
        for tag in receipt.tags.iter() {
            let fields = tag.as_slice();
            if fields.len() >= 2 && fields[0] == "e" {
                let entry = tally.entry(fields[1].to_string()).or_default();
                entry.0 += 1;
                entry.1 += amount;
            }
        }
    }

    Ok(tally
        .into_iter()
        .map(|(event_id, (count, total_msats))| ZapSummary {
            event_id,
            count,
            total_msats,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_zap_request_for_event() {
        let keys = Keys::generate();
        let recipient = hex::encode([0xaa; 32]);
        let event_id = "bb".repeat(32);
        let relays = ["wss://relay.example", "wss://relay.two"];
        let req = ZapRequest {
            recipient_pubkey_hex: &recipient,
            amount_msats: 21_000,
            lnurl: Some("lnurl1..."),
            relays: &relays,
            content: "great call",
            event_id_hex: Some(&event_id),
            event_coordinate: None,
        };
        let unsigned = build_zap_request_event(keys.public_key(), &req).unwrap();
        let signed = unsigned.sign_with_keys(&keys).unwrap();

        assert_eq!(signed.kind, ZAP_REQUEST_KIND);
        assert_eq!(signed.content, "great call");

        let has_amount = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "amount" && f[1] == "21000"
        });
        assert!(has_amount, "amount tag missing");

        let has_relays = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 3
                && f[0] == "relays"
                && f.iter().any(|s| s == "wss://relay.example")
                && f.iter().any(|s| s == "wss://relay.two")
        });
        assert!(has_relays, "relays tag missing");

        let has_p = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "p" && f[1] == recipient
        });
        assert!(has_p, "p tag missing");

        let has_e = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "e" && f[1] == event_id
        });
        assert!(has_e, "e tag missing");

        let has_lnurl = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "lnurl" && f[1] == "lnurl1..."
        });
        assert!(has_lnurl, "lnurl tag missing");
    }

    #[test]
    fn build_zap_request_rejects_zero_amount() {
        let keys = Keys::generate();
        let recipient = hex::encode([0xcc; 32]);
        let req = ZapRequest {
            recipient_pubkey_hex: &recipient,
            amount_msats: 0,
            lnurl: None,
            relays: &[],
            content: "",
            event_id_hex: None,
            event_coordinate: None,
        };
        let err = build_zap_request_event(keys.public_key(), &req).unwrap_err();
        assert!(err.contains("amount_msats"));
    }

    #[test]
    fn build_zap_request_for_profile_only() {
        let keys = Keys::generate();
        let recipient = hex::encode([0xdd; 32]);
        let req = ZapRequest {
            recipient_pubkey_hex: &recipient,
            amount_msats: 1000,
            lnurl: None,
            relays: &["wss://relay.example"],
            content: "",
            event_id_hex: None,
            event_coordinate: None,
        };
        let unsigned = build_zap_request_event(keys.public_key(), &req).unwrap();
        let signed = unsigned.sign_with_keys(&keys).unwrap();
        let has_e = signed
            .tags
            .iter()
            .any(|t| t.as_slice().first().map(String::as_str) == Some("e"));
        assert!(!has_e, "profile-only zap should not carry an e tag");
    }
}
