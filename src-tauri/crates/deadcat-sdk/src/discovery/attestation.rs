use nostr_sdk::prelude::*;
use nostr_sdk::secp256k1;
use serde::{Deserialize, Serialize};

use crate::oracle::oracle_message;
use crate::params::MarketId;

use super::{APP_EVENT_KIND, ATTESTATION_TAG, NETWORK_TAG};

/// Content of an attestation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationContent {
    pub market_id: String,
    pub outcome_yes: bool,
    pub oracle_signature: String,
    pub message: String,
}

/// Result of an oracle attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationResult {
    pub market_id: String,
    pub outcome_yes: bool,
    pub signature_hex: String,
    pub nostr_event_id: String,
}

/// Build a Nostr event for an oracle attestation.
pub fn build_attestation_event(
    keys: &Keys,
    market_id_hex: &str,
    announcement_event_id: &str,
    outcome_yes: bool,
    signature_hex: &str,
    message_hex: &str,
) -> Result<Event, String> {
    let d_tag = format!("{market_id_hex}:attestation");

    let content = serde_json::to_string(&AttestationContent {
        market_id: market_id_hex.to_string(),
        outcome_yes,
        oracle_signature: signature_hex.to_string(),
        message: message_hex.to_string(),
    })
    .map_err(|e| format!("failed to serialize attestation: {e}"))?;

    let outcome_str = if outcome_yes { "yes" } else { "no" };

    let tags = vec![
        Tag::identifier(&d_tag),
        Tag::hashtag(ATTESTATION_TAG),
        Tag::event(
            EventId::from_hex(announcement_event_id)
                .map_err(|e| format!("invalid event id: {e}"))?,
        ),
        Tag::custom(TagKind::custom("outcome"), vec![outcome_str.to_string()]),
        Tag::custom(TagKind::custom("network"), vec![NETWORK_TAG.to_string()]),
    ];

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build attestation event: {e}"))?;

    Ok(event)
}

/// Build a Nostr filter for fetching attestations for a specific market.
pub fn build_attestation_filter(market_id_hex: &str) -> Filter {
    let d_tag = format!("{market_id_hex}:attestation");
    Filter::new()
        .kind(APP_EVENT_KIND)
        .identifier(&d_tag)
        .hashtag(ATTESTATION_TAG)
}

/// Build a Nostr filter for subscribing to all attestation events.
pub fn build_attestation_subscription_filter() -> Filter {
    Filter::new()
        .kind(APP_EVENT_KIND)
        .hashtag(ATTESTATION_TAG)
}

/// Parse a Nostr event into an AttestationContent.
pub fn parse_attestation_event(event: &Event) -> Result<AttestationContent, String> {
    serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse attestation: {e}"))
}

/// Sign an oracle attestation using the Nostr keypair.
///
/// The Nostr x-only public key doubles as the oracle signing key.
/// Uses BIP-340 Schnorr signature over SHA256(market_id || outcome_byte).
pub fn sign_attestation(
    keys: &Keys,
    market_id: &MarketId,
    outcome_yes: bool,
) -> Result<([u8; 64], [u8; 32]), String> {
    let msg = oracle_message(market_id, outcome_yes);
    let secp = secp256k1::Secp256k1::new();
    let message = secp256k1::Message::from_digest(msg);
    let secret_bytes = keys.secret_key().as_secret_bytes().to_owned();
    let sk = secp256k1::SecretKey::from_slice(&secret_bytes)
        .map_err(|e| format!("invalid secret key: {e}"))?;
    let keypair = secp256k1::Keypair::from_secret_key(&secp, &sk);
    let sig = secp.sign_schnorr_no_aux_rand(&message, &keypair);
    Ok((sig.serialize(), msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::secp256k1;

    #[test]
    fn attestation_filter_construction() {
        let market_id_hex = "abcd1234";
        let filter = build_attestation_filter(market_id_hex);
        assert!(format!("{filter:?}").contains("abcd1234:attestation"));
    }

    #[test]
    fn sign_attestation_works() {
        let keys = Keys::generate();
        let market_id = MarketId([0xab; 32]);

        let (sig, msg) = sign_attestation(&keys, &market_id, true).unwrap();
        assert_eq!(sig.len(), 64);
        assert_eq!(msg.len(), 32);

        // Verify the signature
        let secp = secp256k1::Secp256k1::new();
        let message = secp256k1::Message::from_digest(msg);
        let pk_hex = keys.public_key().to_hex();
        let pk_bytes = hex::decode(&pk_hex).unwrap();
        let xonly = secp256k1::XOnlyPublicKey::from_slice(&pk_bytes).unwrap();
        let schnorr_sig = secp256k1::schnorr::Signature::from_slice(&sig).unwrap();
        assert!(secp.verify_schnorr(&schnorr_sig, &message, &xonly).is_ok());
    }

    #[test]
    fn sign_attestation_yes_no_differ() {
        let keys = Keys::generate();
        let market_id = MarketId([0xab; 32]);

        let (sig_yes, msg_yes) = sign_attestation(&keys, &market_id, true).unwrap();
        let (sig_no, msg_no) = sign_attestation(&keys, &market_id, false).unwrap();

        assert_ne!(msg_yes, msg_no);
        assert_ne!(sig_yes, sig_no);
    }
}
