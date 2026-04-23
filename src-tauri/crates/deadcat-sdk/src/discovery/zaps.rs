//! NIP-57 zap request event builder.
//!
//! Callers construct a kind:9734 zap request describing who/what is
//! being tipped, how much, and which relays should see the eventual
//! kind:9735 receipt. The unsigned event is then signed via the
//! caller's `NostrSigner` (local nsec or NIP-46 remote signer) and
//! posted to the recipient's LNURL-pay callback to fetch the
//! Lightning invoice.

use nostr_sdk::prelude::*;

/// Kind for NIP-57 zap request events.
pub const ZAP_REQUEST_KIND: Kind = Kind::Custom(9734);

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
