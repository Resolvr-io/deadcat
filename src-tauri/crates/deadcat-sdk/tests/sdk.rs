use deadcat_sdk::params::ContractParams;
use deadcat_sdk::{DeadcatSdk, MarketState};
use lwk_signer::SwSigner;
use lwk_test_util::{
    TEST_MNEMONIC, TestEnv, TestEnvBuilder, generate_mnemonic, regtest_policy_asset,
};
use lwk_wollet::blocking::BlockchainBackend;
use lwk_wollet::elements::Txid;
use lwk_wollet::elements::secp256k1_zkp::{Keypair, Message, Secp256k1, XOnlyPublicKey};
use lwk_wollet::{ElectrumClient, ElectrumUrl, Wollet};
use tempfile::TempDir;

use std::str::FromStr;
use std::time::Duration;

// ── Helpers ──────────────────────────────────────────────────────────────

struct TestFixture {
    env: TestEnv,
    sdk: DeadcatSdk,
    _temp_dir: TempDir,
}

impl TestFixture {
    fn new() -> Self {
        let test_env = TestEnvBuilder::from_env().with_electrum().build();

        let temp_dir = tempfile::tempdir().unwrap();

        let sdk = DeadcatSdk::new(
            TEST_MNEMONIC,
            deadcat_sdk::Network::LiquidRegtest,
            &test_env.electrum_url(),
            temp_dir.path(),
        )
        .unwrap();

        TestFixture {
            env: test_env,
            sdk,
            _temp_dir: temp_dir,
        }
    }

    /// Fund the SDK wallet with `count` separate UTXOs of `sats_each` sats.
    fn fund(&self, count: u32, sats_each: u64) {
        for _ in 0..count {
            let addr = self.sdk.address(None).unwrap();
            self.env
                .elementsd_sendtoaddress(addr.address(), sats_each, None);
        }
        self.env.elementsd_generate(1);
    }

    /// Fund + sync in one call.
    fn fund_and_sync(&mut self, count: u32, sats_each: u64) {
        self.fund(count, sats_each);
        std::thread::sleep(Duration::from_secs(1));
        self.sdk.sync().unwrap();
    }
}

pub fn generate_signer() -> SwSigner {
    let mnemonic = generate_mnemonic();
    SwSigner::new(&mnemonic, false).unwrap()
}

pub fn electrum_client(env: &TestEnv) -> ElectrumClient {
    let electrum_url = ElectrumUrl::from_str(&env.electrum_url()).unwrap();
    ElectrumClient::new(&electrum_url).unwrap()
}

pub fn sync<S: BlockchainBackend>(wollet: &mut Wollet, client: &mut S) {
    let update = client.full_scan(wollet).unwrap();
    if let Some(update) = update {
        wollet.apply_update(update).unwrap();
    }
}

pub fn wait_for_tx<S: BlockchainBackend>(wollet: &mut Wollet, client: &mut S, txid: &Txid) {
    for _ in 0..120 {
        sync(wollet, client);
        let list = wollet.transactions().unwrap();
        if list.iter().any(|e| &e.txid == txid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    panic!("Wallet does not have {txid} in its list");
}

/// A dummy oracle pubkey for tests.
fn test_oracle_pubkey() -> [u8; 32] {
    [0x02; 32]
}

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn test_the_sdk() {
    let test_fixture = TestFixture::new();
    let env = test_fixture.env;
    let mut sdk = test_fixture.sdk;

    let sdk_address_result = sdk.address(None).unwrap();
    let sdk_address = sdk_address_result.address();
    env.elementsd_sendtoaddress(sdk_address, 1234, None);
    env.elementsd_generate(10);
    let lbtc_asset = regtest_policy_asset();
    assert_eq!(*sdk.balance().unwrap().get(&lbtc_asset).unwrap(), 0);
    sdk.sync().unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(*sdk.balance().unwrap().get(&lbtc_asset).unwrap(), 1234);
}

#[test]
fn test_generate_mnemonic() {
    let (mnemonic, _signer) = DeadcatSdk::generate_mnemonic(false).unwrap();
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    assert_eq!(words.len(), 12);
}

#[test]
fn test_network_properties() {
    let fixture = TestFixture::new();
    let sdk = &fixture.sdk;

    assert_eq!(sdk.network(), deadcat_sdk::Network::LiquidRegtest);
    assert!(!sdk.electrum_url().is_empty());

    let policy = sdk.policy_asset();
    assert_eq!(policy, regtest_policy_asset());
}

#[test]
fn test_address_derivation() {
    let fixture = TestFixture::new();
    let sdk = &fixture.sdk;

    let addr0 = sdk.address(Some(0)).unwrap();
    let addr1 = sdk.address(Some(1)).unwrap();
    let addr0_again = sdk.address(Some(0)).unwrap();

    // Same index produces the same address.
    assert_eq!(
        addr0.address().to_string(),
        addr0_again.address().to_string()
    );
    // Different indices produce different addresses.
    assert_ne!(addr0.address().to_string(), addr1.address().to_string());
}

#[test]
fn test_empty_wallet_state() {
    let fixture = TestFixture::new();
    let sdk = &fixture.sdk;

    let balance = sdk.balance().unwrap();
    // A fresh wallet has no balance (or zero policy asset).
    assert!(balance.is_empty() || *balance.get(&regtest_policy_asset()).unwrap_or(&0) == 0);

    let utxos = sdk.utxos().unwrap();
    assert!(utxos.is_empty());

    let txs = sdk.transactions().unwrap();
    assert!(txs.is_empty());
}

#[test]
fn test_sync_and_balance() {
    let mut fixture = TestFixture::new();
    let lbtc = regtest_policy_asset();

    // Fund with two separate UTXOs.
    fixture.fund_and_sync(2, 50_000);

    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&lbtc).unwrap(), 100_000);

    let utxos = fixture.sdk.utxos().unwrap();
    assert_eq!(utxos.len(), 2);
}

#[test]
fn test_transactions_after_receive() {
    let mut fixture = TestFixture::new();

    fixture.fund_and_sync(1, 10_000);

    let txs = fixture.sdk.transactions().unwrap();
    assert_eq!(txs.len(), 1);
}

#[test]
fn test_send_lbtc() {
    let mut fixture = TestFixture::new();
    let lbtc = regtest_policy_asset();

    // Need multiple UTXOs so TxBuilder can construct a valid tx.
    fixture.fund_and_sync(2, 100_000);

    // Create a second SDK instance as the recipient.
    let (recipient_mnemonic, _) = DeadcatSdk::generate_mnemonic(false).unwrap();
    let temp_dir2 = tempfile::tempdir().unwrap();
    let recipient = DeadcatSdk::new(
        &recipient_mnemonic,
        deadcat_sdk::Network::LiquidRegtest,
        fixture.sdk.electrum_url(),
        temp_dir2.path(),
    )
    .unwrap();

    let recv_addr = recipient.address(None).unwrap();
    let (txid, fee) = fixture
        .sdk
        .send_lbtc(&recv_addr.address().to_string(), 50_000, None)
        .unwrap();

    assert!(fee > 0);

    // Confirm and check sender balance decreased.
    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(1));
    fixture.sdk.sync().unwrap();

    let sender_balance = *fixture.sdk.balance().unwrap().get(&lbtc).unwrap();
    assert_eq!(sender_balance, 200_000 - 50_000 - fee);

    // Check the tx appears in sender's transaction list.
    let txs = fixture.sdk.transactions().unwrap();
    assert!(txs.iter().any(|t| t.txid == txid));
}

#[test]
fn test_send_lbtc_insufficient_funds() {
    let mut fixture = TestFixture::new();

    // Fund with a tiny amount.
    fixture.fund_and_sync(1, 1_000);

    let (recipient_mnemonic, _) = DeadcatSdk::generate_mnemonic(false).unwrap();
    let temp_dir2 = tempfile::tempdir().unwrap();
    let recipient = DeadcatSdk::new(
        &recipient_mnemonic,
        deadcat_sdk::Network::LiquidRegtest,
        fixture.sdk.electrum_url(),
        temp_dir2.path(),
    )
    .unwrap();
    let recv_addr = recipient.address(None).unwrap();

    let result = fixture
        .sdk
        .send_lbtc(&recv_addr.address().to_string(), 1_000_000, None);
    assert!(result.is_err());
}

#[test]
fn test_boltz_key_derivation() {
    let fixture = TestFixture::new();
    let sdk = &fixture.sdk;

    let sub_key = sdk.boltz_submarine_refund_pubkey_hex().unwrap();
    let rev_key = sdk.boltz_reverse_claim_pubkey_hex().unwrap();

    // Both should be valid hex-encoded compressed pubkeys (66 hex chars).
    assert_eq!(sub_key.len(), 66);
    assert_eq!(rev_key.len(), 66);

    // The two derivation paths should produce different keys.
    assert_ne!(sub_key, rev_key);

    // Deterministic: calling again returns the same keys.
    assert_eq!(sub_key, sdk.boltz_submarine_refund_pubkey_hex().unwrap());
    assert_eq!(rev_key, sdk.boltz_reverse_claim_pubkey_hex().unwrap());
}

#[test]
fn test_fetch_transaction() {
    let mut fixture = TestFixture::new();

    fixture.fund_and_sync(1, 50_000);

    let txs = fixture.sdk.transactions().unwrap();
    assert!(!txs.is_empty());

    let txid = txs[0].txid;
    let fetched = fixture.sdk.fetch_transaction(&txid).unwrap();
    assert!(!fetched.output.is_empty());
}

#[test]
fn test_utxos_match_balance() {
    let mut fixture = TestFixture::new();
    let lbtc = regtest_policy_asset();

    fixture.fund_and_sync(3, 25_000);

    let balance = *fixture.sdk.balance().unwrap().get(&lbtc).unwrap();
    let utxo_sum: u64 = fixture
        .sdk
        .utxos()
        .unwrap()
        .iter()
        .filter(|u| u.unblinded.asset == lbtc && !u.is_spent)
        .map(|u| u.unblinded.value)
        .sum();

    assert_eq!(balance, utxo_sum);
}

#[test]
fn test_create_contract_onchain() {
    let mut fixture = TestFixture::new();
    let lbtc = regtest_policy_asset();

    // The creation tx needs at least 2 defining UTXOs + fee headroom.
    // Fund with several UTXOs.
    fixture.fund_and_sync(5, 100_000);

    let (txid, params) = fixture
        .sdk
        .create_contract_onchain(
            test_oracle_pubkey(),
            10_000,  // collateral_per_token
            500_000, // expiry_time (block height)
            1_000,   // min_utxo_value
            500,     // fee_amount
        )
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(1));
    fixture.sdk.sync().unwrap();

    // Verify the creation tx is in the wallet's history.
    let txs = fixture.sdk.transactions().unwrap();
    assert!(txs.iter().any(|t| t.txid == txid));

    // Verify the params have meaningful content.
    assert_eq!(params.oracle_public_key, test_oracle_pubkey());
    assert_eq!(params.collateral_per_token, 10_000);
    assert_eq!(params.expiry_time, 500_000);
    assert_eq!(
        params.collateral_asset_id,
        lbtc.into_inner().to_byte_array()
    );

    // Token asset IDs should be non-zero and distinct.
    assert_ne!(params.yes_token_asset, [0u8; 32]);
    assert_ne!(params.no_token_asset, [0u8; 32]);
    assert_ne!(params.yes_token_asset, params.no_token_asset);
    assert_ne!(params.yes_reissuance_token, params.no_reissuance_token);
}

#[test]
fn test_create_contract_insufficient_utxos() {
    let mut fixture = TestFixture::new();

    // Only fund 1 UTXO — creation needs at least 2.
    fixture.fund_and_sync(1, 100_000);

    let result =
        fixture
            .sdk
            .create_contract_onchain(test_oracle_pubkey(), 10_000, 500_000, 1_000, 500);
    assert!(result.is_err());
}

#[test]
fn test_initial_issuance_from_dormant() {
    let mut fixture = TestFixture::new();

    // Fund generously for creation + issuance.
    fixture.fund_and_sync(10, 500_000);

    let (creation_txid, params) = fixture
        .sdk
        .create_contract_onchain(
            test_oracle_pubkey(),
            10_000,  // collateral_per_token
            500_000, // expiry_time
            1_000,   // min_utxo_value
            500,     // fee_amount
        )
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Issue 5 pairs (5 YES + 5 NO tokens).
    let issuance = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 5, 500)
        .unwrap();

    assert_eq!(issuance.previous_state, MarketState::Dormant);
    assert_eq!(issuance.new_state, MarketState::Unresolved);
    assert_eq!(issuance.pairs_issued, 5);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // The wallet should now hold YES and NO tokens.
    let balance = fixture.sdk.balance().unwrap();
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();

    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 5);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 5);
}

#[test]
fn test_subsequent_issuance_from_unresolved() {
    let mut fixture = TestFixture::new();

    // Fund generously.
    fixture.fund_and_sync(15, 500_000);

    let (creation_txid, params) = fixture
        .sdk
        .create_contract_onchain(test_oracle_pubkey(), 10_000, 500_000, 1_000, 500)
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // First issuance: Dormant → Unresolved.
    let issuance1 = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 3, 500)
        .unwrap();
    assert_eq!(issuance1.previous_state, MarketState::Dormant);
    assert_eq!(issuance1.new_state, MarketState::Unresolved);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Second issuance: Unresolved → Unresolved.
    let issuance2 = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 2, 500)
        .unwrap();
    assert_eq!(issuance2.previous_state, MarketState::Unresolved);
    assert_eq!(issuance2.new_state, MarketState::Unresolved);
    assert_eq!(issuance2.pairs_issued, 2);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Total tokens: 3 + 2 = 5 of each.
    let balance = fixture.sdk.balance().unwrap();
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 5);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 5);
}

#[test]
fn test_new_with_invalid_mnemonic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let result = DeadcatSdk::new(
        "not a valid mnemonic phrase at all",
        deadcat_sdk::Network::LiquidRegtest,
        "tcp://localhost:50001",
        temp_dir.path(),
    );
    assert!(result.is_err());
}

#[test]
fn test_send_lbtc_invalid_address() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(2, 100_000);

    let result = fixture.sdk.send_lbtc("not-a-valid-address", 10_000, None);
    assert!(result.is_err());
}

// ── Oracle signing helpers ──────────────────────────────────────────────

/// Generate a random oracle keypair, returning (x-only pubkey bytes, Keypair).
fn generate_oracle_keypair() -> ([u8; 32], Keypair) {
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();
    let keypair = Keypair::new(&secp, &mut rng);
    let (xonly, _parity) = XOnlyPublicKey::from_keypair(&keypair);
    (xonly.serialize(), keypair)
}

/// Sign the oracle message for a market resolution.
fn oracle_sign(params: &ContractParams, outcome_yes: bool, keypair: &Keypair) -> [u8; 64] {
    let market_id = params.market_id();
    let msg_hash = deadcat_sdk::oracle::oracle_message(&market_id, outcome_yes);
    let secp = Secp256k1::new();
    let msg = Message::from_digest(msg_hash);
    let sig = secp.sign_schnorr(&msg, keypair);
    sig.serialize()
}

/// Create a market with a real oracle keypair and issue tokens.
/// Returns (creation_txid, params, oracle_keypair).
fn create_and_issue(
    fixture: &mut TestFixture,
    oracle_pubkey: [u8; 32],
    cpt: u64,
    expiry_time: u32,
    pairs: u64,
) -> (Txid, ContractParams) {
    let (creation_txid, params) = fixture
        .sdk
        .create_contract_onchain(oracle_pubkey, cpt, expiry_time, 1_000, 500)
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    let _issuance = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, pairs, 500)
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    (creation_txid, params)
}

// ── Lifecycle integration tests ─────────────────────────────────────────

#[test]
fn test_full_cancellation_and_reissuance() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();

    // Verify 5 of each token
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 5);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 5);

    // Full cancel: burn all 5 pairs → Unresolved → Dormant
    let cancel = fixture.sdk.cancel_tokens(&params, 5, 500).unwrap();

    assert_eq!(cancel.previous_state, MarketState::Unresolved);
    assert_eq!(cancel.new_state, MarketState::Dormant);
    assert_eq!(cancel.pairs_burned, 5);
    assert!(cancel.is_full_cancellation);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Tokens should be gone
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 0);

    // Re-issue 3 pairs: Dormant → Unresolved
    let reissue = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 3, 500)
        .unwrap();

    assert_eq!(reissue.previous_state, MarketState::Dormant);
    assert_eq!(reissue.new_state, MarketState::Unresolved);
    assert_eq!(reissue.pairs_issued, 3);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 3);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 3);
}

#[test]
fn test_partial_cancellation() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (_creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 10);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();

    // Cancel 3 of 10 pairs
    let cancel = fixture.sdk.cancel_tokens(&params, 3, 500).unwrap();

    assert_eq!(cancel.previous_state, MarketState::Unresolved);
    assert_eq!(cancel.new_state, MarketState::Unresolved);
    assert_eq!(cancel.pairs_burned, 3);
    assert!(!cancel.is_full_cancellation);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // 10 - 3 = 7 of each remain
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 7);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 7);
}

#[test]
fn test_oracle_resolve_and_redeem_yes() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (_creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Oracle resolves YES
    let signature = oracle_sign(&params, true, &keypair);
    let resolve = fixture
        .sdk
        .resolve_market(&params, true, signature, 500)
        .unwrap();

    assert_eq!(resolve.previous_state, MarketState::Unresolved);
    assert_eq!(resolve.new_state, MarketState::ResolvedYes);
    assert!(resolve.outcome_yes);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Redeem YES tokens
    let redeem = fixture.sdk.redeem_tokens(&params, 5, 500).unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedYes);
    assert_eq!(redeem.tokens_redeemed, 5);
    // 5 tokens * 2 * 10_000 cpt = 100_000
    assert_eq!(redeem.payout_sats, 100_000);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // YES tokens should be burned
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
}

#[test]
fn test_oracle_resolve_and_redeem_no() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (_creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Oracle resolves NO
    let signature = oracle_sign(&params, false, &keypair);
    let resolve = fixture
        .sdk
        .resolve_market(&params, false, signature, 500)
        .unwrap();

    assert_eq!(resolve.previous_state, MarketState::Unresolved);
    assert_eq!(resolve.new_state, MarketState::ResolvedNo);
    assert!(!resolve.outcome_yes);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Redeem NO tokens
    let redeem = fixture.sdk.redeem_tokens(&params, 5, 500).unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedNo);
    assert_eq!(redeem.tokens_redeemed, 5);
    assert_eq!(redeem.payout_sats, 100_000);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 0);
}

#[test]
fn test_expiry_redemption() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    // Use a very low expiry height so we can pass it in regtest
    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let expiry_height = 200u32;

    let (_creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, expiry_height, 5);

    // Generate blocks past expiry
    fixture.env.elementsd_generate(250);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Redeem YES tokens via expiry path
    let redeem = fixture
        .sdk
        .redeem_expired(&params, params.yes_token_asset, 5, 500)
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::Unresolved);
    assert_eq!(redeem.tokens_redeemed, 5);
    // Expiry gives 1x payout: 5 * 10_000 = 50_000
    assert_eq!(redeem.payout_sats, 50_000);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
}

#[test]
fn test_multiple_subsequent_issuances() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(25, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (creation_txid, params) = fixture
        .sdk
        .create_contract_onchain(oracle_pubkey, 10_000, 500_000, 1_000, 500)
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Issue 3 pairs: Dormant → Unresolved
    let iss1 = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 3, 500)
        .unwrap();
    assert_eq!(iss1.previous_state, MarketState::Dormant);
    assert_eq!(iss1.new_state, MarketState::Unresolved);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Issue 2 more: Unresolved → Unresolved
    let iss2 = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 2, 500)
        .unwrap();
    assert_eq!(iss2.previous_state, MarketState::Unresolved);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Issue 5 more: Unresolved → Unresolved
    let iss3 = fixture
        .sdk
        .issue_tokens(&params, &creation_txid, 5, 500)
        .unwrap();
    assert_eq!(iss3.previous_state, MarketState::Unresolved);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Total: 3 + 2 + 5 = 10 of each
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 10);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 10);
}

#[test]
fn test_partial_post_resolution_redemption() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (_creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Oracle resolves YES
    let signature = oracle_sign(&params, true, &keypair);
    fixture
        .sdk
        .resolve_market(&params, true, signature, 500)
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Redeem only 3 of 5 YES tokens (partial redemption with token change)
    let redeem = fixture.sdk.redeem_tokens(&params, 3, 500).unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedYes);
    assert_eq!(redeem.tokens_redeemed, 3);
    // 3 tokens * 2 * 10_000 cpt = 60_000
    assert_eq!(redeem.payout_sats, 60_000);

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // 5 - 3 = 2 YES tokens should remain as change
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 2);
}

#[test]
fn test_cancel_wrong_state() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(10, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (_creation_txid, params) = fixture
        .sdk
        .create_contract_onchain(oracle_pubkey, 10_000, 500_000, 1_000, 500)
        .unwrap();

    fixture.env.elementsd_generate(1);
    std::thread::sleep(Duration::from_secs(2));
    fixture.sdk.sync().unwrap();

    // Market is Dormant (no issuance) — cancellation should fail
    let result = fixture.sdk.cancel_tokens(&params, 1, 500);
    assert!(result.is_err(), "cancel should fail in Dormant state");
}

#[test]
fn test_redeem_wrong_state() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (_creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Market is Unresolved — post-resolution redemption should fail
    let result = fixture.sdk.redeem_tokens(&params, 5, 500);
    assert!(result.is_err(), "redeem should fail in Unresolved state");
}
