use std::time::Duration;

use deadcat_sdk::announcement::{ContractAnnouncement, ContractMetadata};
use deadcat_sdk::discovery::{
    build_announcement_event, build_attestation_event, build_attestation_filter,
    build_contract_filter, parse_announcement_event, sign_attestation, AttestationContent,
};
use deadcat_sdk::params::{ContractParams, MarketId};
use nostr_relay_builder::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::secp256k1;

fn test_metadata() -> ContractMetadata {
    ContractMetadata {
        question: "Will BTC close above $120k by Dec 2026?".to_string(),
        description: "Resolved using median close basket.".to_string(),
        category: "Bitcoin".to_string(),
        resolution_source: "Exchange close basket".to_string(),
        starting_yes_price: 57,
    }
}

fn test_params(oracle_pubkey: [u8; 32]) -> ContractParams {
    ContractParams {
        oracle_public_key: oracle_pubkey,
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: [0x01; 32],
        no_token_asset: [0x02; 32],
        yes_reissuance_token: [0x03; 32],
        no_reissuance_token: [0x04; 32],
        collateral_per_token: 5000,
        expiry_time: 3_650_000,
    }
}

#[tokio::test]
async fn publish_discover_roundtrip() {
    // Start a mock relay
    let mock = MockRelay::run().await.unwrap();
    let relay_url = mock.url();

    // Generate keys
    let keys = Keys::generate();
    let oracle_pubkey: [u8; 32] = {
        let h = keys.public_key().to_hex();
        let b = hex::decode(&h).unwrap();
        <[u8; 32]>::try_from(b.as_slice()).unwrap()
    };

    // Build announcement
    let params = test_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: Some("abc123def456".to_string()),
    };

    let event = build_announcement_event(&keys, &announcement).unwrap();

    // Connect client and publish
    let client = Client::new(keys.clone());
    client.add_relay(&relay_url).await.unwrap();
    client.connect().await;

    client.send_event(event.clone()).await.unwrap();

    // Small delay to let the relay process
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch announcements
    let filter = build_contract_filter();
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(5))
        .await
        .unwrap();

    assert!(!events.is_empty(), "should have fetched at least one event");

    // Parse and verify
    let fetched_event = events.iter().next().unwrap();
    let market = parse_announcement_event(fetched_event).unwrap();

    assert_eq!(market.question, "Will BTC close above $120k by Dec 2026?");
    assert_eq!(market.category, "Bitcoin");
    assert_eq!(market.description, "Resolved using median close basket.");
    assert_eq!(market.resolution_source, "Exchange close basket");
    assert_eq!(market.starting_yes_price, 57);
    assert_eq!(market.expiry_height, 3_650_000);
    assert_eq!(market.cpt_sats, 5000);
    assert_eq!(market.oracle_pubkey, hex::encode(oracle_pubkey));
    assert_eq!(market.creator_pubkey, keys.public_key().to_hex());
    assert_eq!(market.creation_txid, Some("abc123def456".to_string()));

    // Verify market_id is correct
    let expected_market_id = params.market_id();
    assert_eq!(market.market_id, hex::encode(expected_market_id.as_bytes()));

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn oracle_attestation_roundtrip() {
    let mock = MockRelay::run().await.unwrap();
    let relay_url = mock.url();

    let keys = Keys::generate();
    let oracle_pubkey: [u8; 32] = {
        let h = keys.public_key().to_hex();
        let b = hex::decode(&h).unwrap();
        <[u8; 32]>::try_from(b.as_slice()).unwrap()
    };
    let params = test_params(oracle_pubkey);
    let market_id = params.market_id();
    let market_id_hex = hex::encode(market_id.as_bytes());

    // First publish the announcement
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };

    let ann_event = build_announcement_event(&keys, &announcement).unwrap();
    let ann_event_id = ann_event.id.to_hex();

    let client = Client::new(keys.clone());
    client.add_relay(&relay_url).await.unwrap();
    client.connect().await;
    client.send_event(ann_event).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Sign attestation
    let (sig_bytes, msg_bytes) = sign_attestation(&keys, &market_id, true).unwrap();
    let sig_hex = hex::encode(sig_bytes);
    let msg_hex = hex::encode(msg_bytes);

    // Build and publish attestation event
    let att_event = build_attestation_event(
        &keys,
        &market_id_hex,
        &ann_event_id,
        true,
        &sig_hex,
        &msg_hex,
    )
    .unwrap();

    client.send_event(att_event).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch attestation events
    let att_filter = build_attestation_filter(&market_id_hex);
    let att_events = client
        .fetch_events(vec![att_filter], Duration::from_secs(5))
        .await
        .unwrap();

    assert!(!att_events.is_empty(), "should find attestation event");

    let att_ev = att_events.iter().next().unwrap();
    let content: AttestationContent = serde_json::from_str(&att_ev.content).unwrap();

    assert_eq!(content.market_id, market_id_hex);
    assert!(content.outcome_yes);
    assert_eq!(content.oracle_signature, sig_hex);

    // Verify the signature matches
    let secp = secp256k1::Secp256k1::new();
    let message = secp256k1::Message::from_digest(msg_bytes);
    let xonly = secp256k1::XOnlyPublicKey::from_slice(&oracle_pubkey).unwrap();
    let schnorr_sig = secp256k1::schnorr::Signature::from_slice(&sig_bytes).unwrap();
    assert!(secp.verify_schnorr(&schnorr_sig, &message, &xonly).is_ok());

    client.disconnect().await.unwrap();
}

#[tokio::test]
async fn attestation_signature_verification() {
    let keys = Keys::generate();
    let oracle_pubkey: [u8; 32] = {
        let h = keys.public_key().to_hex();
        let b = hex::decode(&h).unwrap();
        <[u8; 32]>::try_from(b.as_slice()).unwrap()
    };
    let market_id = MarketId([0xcd; 32]);

    // Sign YES attestation
    let (sig_yes, msg_yes) = sign_attestation(&keys, &market_id, true).unwrap();
    // Sign NO attestation
    let (sig_no, msg_no) = sign_attestation(&keys, &market_id, false).unwrap();

    let secp = secp256k1::Secp256k1::new();
    let xonly = secp256k1::XOnlyPublicKey::from_slice(&oracle_pubkey).unwrap();

    // Verify YES signature
    let message_yes = secp256k1::Message::from_digest(msg_yes);
    let schnorr_yes = secp256k1::schnorr::Signature::from_slice(&sig_yes).unwrap();
    assert!(
        secp.verify_schnorr(&schnorr_yes, &message_yes, &xonly)
            .is_ok(),
        "YES attestation signature should verify"
    );

    // Verify NO signature
    let message_no = secp256k1::Message::from_digest(msg_no);
    let schnorr_no = secp256k1::schnorr::Signature::from_slice(&sig_no).unwrap();
    assert!(
        secp.verify_schnorr(&schnorr_no, &message_no, &xonly)
            .is_ok(),
        "NO attestation signature should verify"
    );

    // YES and NO should produce different messages and signatures
    assert_ne!(msg_yes, msg_no);
    assert_ne!(sig_yes, sig_no);

    // Cross-verification should fail: YES sig on NO message
    assert!(
        secp.verify_schnorr(&schnorr_yes, &message_no, &xonly)
            .is_err(),
        "YES sig should NOT verify against NO message"
    );
}
