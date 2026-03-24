use deadcat_sdk::PredictionMarketParams;
use deadcat_sdk::{
    DeadcatSdk, MarketState, MinerFeePolicy, OrderDirection, PredictionMarketAnchor,
    ResolvedMinerFee, TxOptions,
};
use lwk_signer::SwSigner;
use lwk_test_util::{
    TEST_MNEMONIC, TestEnv, TestEnvBuilder, generate_mnemonic, regtest_policy_asset,
};
use lwk_wollet::blocking::BlockchainBackend;
use lwk_wollet::elements::secp256k1_zkp::{Keypair, Message, Secp256k1, XOnlyPublicKey};
use lwk_wollet::elements::{Transaction, Txid};
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
        self.mine_and_sync(1);
    }

    /// Mine `n` blocks, wait for electrs to index, then sync the wallet.
    fn mine_and_sync(&mut self, n: u32) {
        self.env.elementsd_generate(n);
        std::thread::sleep(Duration::from_millis(500));
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

fn anchor_txid(anchor: &PredictionMarketAnchor) -> Txid {
    anchor.creation_txid.parse().unwrap()
}

fn sample_rate_tx_options() -> TxOptions {
    TxOptions {
        fee_policy: MinerFeePolicy::RateSatPerVb { sat_per_vb: 1.0 },
    }
}

fn default_confirmation_target_tx_options() -> TxOptions {
    TxOptions {
        fee_policy: MinerFeePolicy::ConfirmationTargetBlocks { blocks: 6 },
    }
}

fn exact_fee_tx_options(amount_sat: u64) -> TxOptions {
    TxOptions {
        fee_policy: MinerFeePolicy::ExactAmountSat { amount_sat },
    }
}

fn actual_fee_amount(tx: &Transaction) -> u64 {
    tx.output
        .iter()
        .filter(|output| output.script_pubkey.is_empty())
        .map(|output| output.value.explicit().unwrap_or(0))
        .sum()
}

fn assert_effective_fee_metadata(tx: &Transaction, fee: &ResolvedMinerFee) {
    assert_eq!(fee.amount_sat, actual_fee_amount(tx));
    let expected_rate = fee.amount_sat as f32 / tx.discount_vsize().max(1) as f32;
    assert!(
        (fee.rate_sat_per_vb - expected_rate).abs() < 1e-6,
        "expected effective rate {expected_rate}, got {}",
        fee.rate_sat_per_vb
    );
}

fn assert_aggregated_fee_metadata(txs: &[Transaction], fee: &ResolvedMinerFee) {
    let expected_amount_sat: u64 = txs.iter().map(actual_fee_amount).sum();
    let expected_discount_vsize: u64 = txs.iter().map(|tx| tx.discount_vsize() as u64).sum();
    let expected_rate = expected_amount_sat as f32 / expected_discount_vsize.max(1) as f32;

    assert_eq!(fee.amount_sat, expected_amount_sat);
    assert_eq!(fee.discount_vsize, expected_discount_vsize);
    assert!(
        (fee.rate_sat_per_vb - expected_rate).abs() < 1e-6,
        "expected aggregated rate {expected_rate}, got {}",
        fee.rate_sat_per_vb
    );
}

// ── Tests ────────────────────────────────────────────────────────────────
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
        .send_lbtc(
            &recv_addr.address().to_string(),
            50_000,
            sample_rate_tx_options(),
        )
        .unwrap();

    assert!(fee > 0);

    // Confirm and check sender balance decreased.
    fixture.mine_and_sync(1);

    let sender_balance = *fixture.sdk.balance().unwrap().get(&lbtc).unwrap();
    assert_eq!(sender_balance, 200_000 - 50_000 - fee);

    // Check the tx appears in sender's transaction list.
    let txs = fixture.sdk.transactions().unwrap();
    assert!(txs.iter().any(|t| t.txid == txid));
}

#[test]
fn test_prepare_send_lbtc_reports_effective_fee_metadata() {
    let mut fixture = TestFixture::new();

    fixture.fund_and_sync(2, 100_000);

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
    let prepared = fixture
        .sdk
        .prepare_send_lbtc(
            &recv_addr.address().to_string(),
            50_000,
            sample_rate_tx_options(),
        )
        .unwrap();

    assert_effective_fee_metadata(&prepared.prepared_tx.tx, &prepared.prepared_tx.fee);
}

#[test]
fn test_send_lbtc_executes_with_default_confirmation_target_fee_policy() {
    let mut fixture = TestFixture::new();

    fixture.fund_and_sync(2, 100_000);

    let (recipient_mnemonic, _) = DeadcatSdk::generate_mnemonic(false).unwrap();
    let temp_dir2 = tempfile::tempdir().unwrap();
    let mut recipient = DeadcatSdk::new(
        &recipient_mnemonic,
        deadcat_sdk::Network::LiquidRegtest,
        fixture.sdk.electrum_url(),
        temp_dir2.path(),
    )
    .unwrap();

    let recv_addr = recipient.address(None).unwrap();
    let tx_options = default_confirmation_target_tx_options();
    let prepared = fixture
        .sdk
        .prepare_send_lbtc(&recv_addr.address().to_string(), 50_000, tx_options.clone())
        .unwrap();

    assert_eq!(prepared.prepared_tx.fee.policy, tx_options.fee_policy);
    assert!(prepared.prepared_tx.fee.amount_sat > 0);
    assert_effective_fee_metadata(&prepared.prepared_tx.tx, &prepared.prepared_tx.fee);

    let (txid, fee_sat) = fixture.sdk.broadcast_prepared_send_lbtc(&prepared).unwrap();
    assert_eq!(txid, prepared.prepared_tx.txid);
    assert_eq!(fee_sat, prepared.prepared_tx.fee.amount_sat);

    let tx = fixture.sdk.fetch_transaction(&txid).unwrap();
    assert_effective_fee_metadata(&tx, &prepared.prepared_tx.fee);

    fixture.mine_and_sync(1);
    recipient.sync().unwrap();

    let lbtc = regtest_policy_asset();
    let recipient_balance = *recipient.balance().unwrap().get(&lbtc).unwrap_or(&0);
    assert_eq!(recipient_balance, 50_000);
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

    let result = fixture.sdk.send_lbtc(
        &recv_addr.address().to_string(),
        1_000_000,
        sample_rate_tx_options(),
    );
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

    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            test_oracle_pubkey(),
            10_000,  // collateral_per_token
            500_000, // expiry_time (block height)
            1_000,   // min_utxo_value
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let anchor = creation.anchor;
    let params = creation.params;
    let txid = anchor_txid(&anchor);

    fixture.mine_and_sync(1);

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

    let tx = fixture.sdk.fetch_transaction(&txid).unwrap();
    let contract = deadcat_sdk::CompiledPredictionMarket::new(params).unwrap();
    assert_eq!(
        tx.output[0].script_pubkey,
        contract.script_pubkey(deadcat_sdk::MarketSlot::DormantYesRt)
    );
    assert_eq!(
        tx.output[1].script_pubkey,
        contract.script_pubkey(deadcat_sdk::MarketSlot::DormantNoRt)
    );
    assert!(matches!(
        tx.output[0].asset,
        lwk_wollet::elements::confidential::Asset::Confidential(_)
    ));
    assert!(matches!(
        tx.output[1].asset,
        lwk_wollet::elements::confidential::Asset::Confidential(_)
    ));
}

#[test]
fn test_create_contract_insufficient_utxos() {
    let mut fixture = TestFixture::new();

    // Only fund 1 UTXO — creation needs at least 2.
    fixture.fund_and_sync(1, 100_000);

    let result = fixture.sdk.prepare_contract_creation(
        test_oracle_pubkey(),
        10_000,
        500_000,
        1_000,
        exact_fee_tx_options(500),
    );
    assert!(result.is_err());
}

#[test]
fn test_initial_issuance_from_dormant() {
    let mut fixture = TestFixture::new();

    // Fund generously for creation + issuance.
    fixture.fund_and_sync(10, 500_000);

    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            test_oracle_pubkey(),
            10_000,  // collateral_per_token
            500_000, // expiry_time
            1_000,   // min_utxo_value
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let creation_txid = creation.anchor;
    let params = creation.params;

    fixture.mine_and_sync(1);

    // Issue 5 pairs (5 YES + 5 NO tokens).
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 5, exact_fee_tx_options(500))
        .unwrap();
    let issuance = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();

    assert_eq!(issuance.previous_state, MarketState::Dormant);
    assert_eq!(issuance.new_state, MarketState::Unresolved);
    assert_eq!(issuance.pairs_issued, 5);

    fixture.mine_and_sync(1);

    // The wallet should now hold YES and NO tokens.
    let balance = fixture.sdk.balance().unwrap();
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();

    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 5);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 5);
}

#[test]
fn test_prepare_issue_tokens_reports_effective_fee_metadata() {
    let mut fixture = TestFixture::new();

    fixture.fund_and_sync(10, 500_000);

    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            test_oracle_pubkey(),
            10_000,
            500_000,
            1_000,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let creation_txid = creation.anchor;
    let params = creation.params;

    fixture.mine_and_sync(1);

    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 5, sample_rate_tx_options())
        .unwrap();

    assert_effective_fee_metadata(&prepared.prepared_tx.tx, &prepared.prepared_tx.fee);
}

#[test]
fn test_market_lifecycle_executes_with_default_confirmation_target_fee_policy() {
    let mut fixture = TestFixture::new();

    fixture.fund_and_sync(10, 500_000);

    let tx_options = default_confirmation_target_tx_options();
    let prepared_creation = fixture
        .sdk
        .prepare_contract_creation(
            test_oracle_pubkey(),
            10_000,
            500_000,
            1_000,
            tx_options.clone(),
        )
        .unwrap();

    assert_eq!(
        prepared_creation.prepared_tx.fee.policy,
        tx_options.fee_policy
    );
    assert!(prepared_creation.prepared_tx.fee.amount_sat > 0);
    assert_effective_fee_metadata(
        &prepared_creation.prepared_tx.tx,
        &prepared_creation.prepared_tx.fee,
    );

    let expected_creation_fee = prepared_creation.prepared_tx.fee.clone();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared_creation)
        .unwrap();
    let creation_anchor = creation.anchor;
    let params = creation.params;
    assert_eq!(creation.fee, expected_creation_fee);
    assert_eq!(
        anchor_txid(&creation_anchor),
        anchor_txid(&prepared_creation.anchor)
    );

    let creation_tx = fixture
        .sdk
        .fetch_transaction(&anchor_txid(&creation_anchor))
        .unwrap();
    assert_effective_fee_metadata(&creation_tx, &expected_creation_fee);

    fixture.mine_and_sync(1);

    let prepared_issue = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_anchor, 5, tx_options.clone())
        .unwrap();

    assert_eq!(prepared_issue.prepared_tx.fee.policy, tx_options.fee_policy);
    assert!(prepared_issue.prepared_tx.fee.amount_sat > 0);
    assert_effective_fee_metadata(
        &prepared_issue.prepared_tx.tx,
        &prepared_issue.prepared_tx.fee,
    );

    let issuance = fixture
        .sdk
        .broadcast_prepared_issuance(&prepared_issue)
        .unwrap();
    assert_eq!(issuance.previous_state, MarketState::Dormant);
    assert_eq!(issuance.new_state, MarketState::Unresolved);
    assert_eq!(issuance.pairs_issued, 5);
    assert_eq!(issuance.fee.policy, tx_options.fee_policy);
    assert!(issuance.fee.amount_sat > 0);

    let issuance_tx = fixture.sdk.fetch_transaction(&issuance.txid).unwrap();
    assert_effective_fee_metadata(&issuance_tx, &issuance.fee);

    fixture.mine_and_sync(1);

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

    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            test_oracle_pubkey(),
            10_000,
            500_000,
            1_000,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let creation_txid = creation.anchor;
    let params = creation.params;

    fixture.mine_and_sync(1);

    // First issuance: Dormant → Unresolved.
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 3, exact_fee_tx_options(500))
        .unwrap();
    let issuance1 = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();
    assert_eq!(issuance1.previous_state, MarketState::Dormant);
    assert_eq!(issuance1.new_state, MarketState::Unresolved);

    fixture.mine_and_sync(1);

    // Second issuance: Unresolved → Unresolved.
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 2, exact_fee_tx_options(500))
        .unwrap();
    let issuance2 = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();
    assert_eq!(issuance2.previous_state, MarketState::Unresolved);
    assert_eq!(issuance2.new_state, MarketState::Unresolved);
    assert_eq!(issuance2.pairs_issued, 2);

    fixture.mine_and_sync(1);

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

    let result = fixture
        .sdk
        .send_lbtc("not-a-valid-address", 10_000, sample_rate_tx_options());
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
fn oracle_sign(params: &PredictionMarketParams, outcome_yes: bool, keypair: &Keypair) -> [u8; 64] {
    let market_id = params.market_id();
    let msg_hash = deadcat_sdk::oracle_message(&market_id, outcome_yes);
    let secp = Secp256k1::new();
    let msg = Message::from_digest(msg_hash);
    let sig = secp.sign_schnorr(&msg, keypair);
    sig.serialize()
}

/// Create a market with a real oracle keypair and issue tokens.
/// Returns (anchor, params).
fn create_and_issue(
    fixture: &mut TestFixture,
    oracle_pubkey: [u8; 32],
    cpt: u64,
    expiry_time: u32,
    pairs: u64,
) -> (PredictionMarketAnchor, PredictionMarketParams) {
    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            oracle_pubkey,
            cpt,
            expiry_time,
            1_000,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let creation_txid = creation.anchor;
    let params = creation.params;

    fixture.mine_and_sync(1);

    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, pairs, exact_fee_tx_options(500))
        .unwrap();
    let _issuance = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();

    fixture.mine_and_sync(1);

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
    let prepared = fixture
        .sdk
        .prepare_cancel_tokens(&params, &creation_txid, 5, exact_fee_tx_options(500))
        .unwrap();
    let cancel = fixture
        .sdk
        .broadcast_prepared_cancellation(&prepared)
        .unwrap();

    assert_eq!(cancel.previous_state, MarketState::Unresolved);
    assert_eq!(cancel.new_state, MarketState::Dormant);
    assert_eq!(cancel.pairs_burned, 5);
    assert!(cancel.is_full_cancellation);

    fixture.mine_and_sync(1);

    // Tokens should be gone
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 0);

    // Re-issue 3 pairs: Dormant → Unresolved
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 3, exact_fee_tx_options(500))
        .unwrap();
    let reissue = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();

    assert_eq!(reissue.previous_state, MarketState::Dormant);
    assert_eq!(reissue.new_state, MarketState::Unresolved);
    assert_eq!(reissue.pairs_issued, 3);

    fixture.mine_and_sync(1);

    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 3);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 3);
}

#[test]
fn test_partial_cancellation() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 10);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();

    // Cancel 3 of 10 pairs
    let prepared = fixture
        .sdk
        .prepare_cancel_tokens(&params, &creation_txid, 3, exact_fee_tx_options(500))
        .unwrap();
    let cancel = fixture
        .sdk
        .broadcast_prepared_cancellation(&prepared)
        .unwrap();

    assert_eq!(cancel.previous_state, MarketState::Unresolved);
    assert_eq!(cancel.new_state, MarketState::Unresolved);
    assert_eq!(cancel.pairs_burned, 3);
    assert!(!cancel.is_full_cancellation);

    fixture.mine_and_sync(1);

    // 10 - 3 = 7 of each remain
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 7);
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 7);
}

#[test]
fn test_full_cancellation_with_exact_fee_utxo() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    fixture.fund_and_sync(1, 700);

    let prepared = fixture
        .sdk
        .prepare_cancel_tokens(&params, &creation_txid, 5, exact_fee_tx_options(700))
        .unwrap();
    let cancel = fixture
        .sdk
        .broadcast_prepared_cancellation(&prepared)
        .unwrap();

    assert_eq!(cancel.previous_state, MarketState::Unresolved);
    assert_eq!(cancel.new_state, MarketState::Dormant);
    assert!(cancel.is_full_cancellation);

    fixture.mine_and_sync(1);
}

#[test]
fn test_partial_cancellation_with_exact_fee_utxo() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 10);

    fixture.fund_and_sync(1, 700);

    let prepared = fixture
        .sdk
        .prepare_cancel_tokens(&params, &creation_txid, 3, exact_fee_tx_options(700))
        .unwrap();
    let cancel = fixture
        .sdk
        .broadcast_prepared_cancellation(&prepared)
        .unwrap();

    assert_eq!(cancel.previous_state, MarketState::Unresolved);
    assert_eq!(cancel.new_state, MarketState::Unresolved);
    assert!(!cancel.is_full_cancellation);

    fixture.mine_and_sync(1);
}

#[test]
fn test_oracle_resolve_and_redeem_yes() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Oracle resolves YES
    let signature = oracle_sign(&params, true, &keypair);
    let prepared = fixture
        .sdk
        .prepare_resolution(
            &params,
            &creation_txid,
            true,
            signature,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let resolve = fixture
        .sdk
        .broadcast_prepared_resolution(&prepared)
        .unwrap();

    assert_eq!(resolve.previous_state, MarketState::Unresolved);
    assert_eq!(resolve.new_state, MarketState::ResolvedYes);
    assert!(resolve.outcome_yes);

    fixture.mine_and_sync(1);

    // Redeem YES tokens
    let prepared = fixture
        .sdk
        .prepare_redeem_tokens(&params, &creation_txid, 5, exact_fee_tx_options(500))
        .unwrap();
    let redeem = fixture
        .sdk
        .broadcast_prepared_redemption(&prepared)
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedYes);
    assert_eq!(redeem.tokens_redeemed, 5);
    // 5 tokens * 2 * 10_000 cpt = 100_000
    assert_eq!(redeem.payout_sats, 100_000);

    fixture.mine_and_sync(1);

    // YES tokens should be burned
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
}

#[test]
fn test_post_resolution_redeem_with_exact_fee_utxo() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    let signature = oracle_sign(&params, true, &keypair);
    let prepared = fixture
        .sdk
        .prepare_resolution(
            &params,
            &creation_txid,
            true,
            signature,
            exact_fee_tx_options(500),
        )
        .unwrap();
    fixture
        .sdk
        .broadcast_prepared_resolution(&prepared)
        .unwrap();

    fixture.mine_and_sync(1);
    fixture.fund_and_sync(1, 700);

    let prepared = fixture
        .sdk
        .prepare_redeem_tokens(&params, &creation_txid, 5, exact_fee_tx_options(700))
        .unwrap();
    let redeem = fixture
        .sdk
        .broadcast_prepared_redemption(&prepared)
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedYes);
    assert_eq!(redeem.tokens_redeemed, 5);
    assert_eq!(redeem.payout_sats, 100_000);

    fixture.mine_and_sync(1);
}

#[test]
fn test_oracle_resolve_with_exact_fee_utxo() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Add an exact-fee UTXO after issuance so resolve_market selects it.
    fixture.fund_and_sync(1, 700);

    let signature = oracle_sign(&params, true, &keypair);
    let prepared = fixture
        .sdk
        .prepare_resolution(
            &params,
            &creation_txid,
            true,
            signature,
            exact_fee_tx_options(700),
        )
        .unwrap();
    let resolve = fixture
        .sdk
        .broadcast_prepared_resolution(&prepared)
        .unwrap();

    assert_eq!(resolve.previous_state, MarketState::Unresolved);
    assert_eq!(resolve.new_state, MarketState::ResolvedYes);

    fixture.mine_and_sync(1);
}

#[test]
fn test_oracle_resolve_and_redeem_no() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Oracle resolves NO
    let signature = oracle_sign(&params, false, &keypair);
    let prepared = fixture
        .sdk
        .prepare_resolution(
            &params,
            &creation_txid,
            false,
            signature,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let resolve = fixture
        .sdk
        .broadcast_prepared_resolution(&prepared)
        .unwrap();

    assert_eq!(resolve.previous_state, MarketState::Unresolved);
    assert_eq!(resolve.new_state, MarketState::ResolvedNo);
    assert!(!resolve.outcome_yes);

    fixture.mine_and_sync(1);

    // Redeem NO tokens
    let prepared = fixture
        .sdk
        .prepare_redeem_tokens(&params, &creation_txid, 5, exact_fee_tx_options(500))
        .unwrap();
    let redeem = fixture
        .sdk
        .broadcast_prepared_redemption(&prepared)
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedNo);
    assert_eq!(redeem.tokens_redeemed, 5);
    assert_eq!(redeem.payout_sats, 100_000);

    fixture.mine_and_sync(1);

    let no_asset = lwk_wollet::elements::AssetId::from_slice(&params.no_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&no_asset).unwrap_or(&0), 0);
}

#[test]
fn test_expiry_redemption_with_rate_fee_tx_options_auto_finalize() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let expiry_height = 200u32;

    let (creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, expiry_height, 5);

    fixture.mine_and_sync(250);

    let txids_before: std::collections::HashSet<_> = fixture
        .sdk
        .transactions()
        .unwrap()
        .into_iter()
        .map(|tx| tx.txid)
        .collect();
    let tx_count_before = txids_before.len();
    let tx_options = sample_rate_tx_options();
    let redeem = fixture
        .sdk
        .redeem_expired_with_tx_options(
            &params,
            &creation_txid,
            params.yes_token_asset,
            5,
            tx_options.clone(),
        )
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::Expired);
    assert_eq!(redeem.tokens_redeemed, 5);
    assert_eq!(redeem.payout_sats, 50_000);
    assert_eq!(redeem.fee.policy, tx_options.fee_policy);
    assert!(redeem.fee.amount_sat > 0);

    let txids_after: std::collections::HashSet<_> = fixture
        .sdk
        .transactions()
        .unwrap()
        .into_iter()
        .map(|tx| tx.txid)
        .collect();
    let tx_count_after = txids_after.len();
    assert_eq!(
        tx_count_after,
        tx_count_before + 2,
        "expected expiry redemption to broadcast auto-expire plus redemption"
    );

    let new_txids: Vec<_> = txids_after.difference(&txids_before).copied().collect();
    assert_eq!(new_txids.len(), 2, "expected exactly two new transactions");
    let redemption_tx = fixture.sdk.fetch_transaction(&redeem.txid).unwrap();
    let expiry_txid = new_txids
        .into_iter()
        .find(|txid| *txid != redeem.txid)
        .expect("expected expiry txid");
    let expiry_tx = fixture.sdk.fetch_transaction(&expiry_txid).unwrap();
    assert!(redeem.fee.amount_sat > actual_fee_amount(&redemption_tx));
    assert!(redeem.fee.amount_sat > actual_fee_amount(&expiry_tx));
    assert_aggregated_fee_metadata(&[expiry_tx, redemption_tx], &redeem.fee);

    fixture.mine_and_sync(1);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
}

#[test]
fn test_expiry_redemption_with_default_confirmation_target_auto_finalize() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let expiry_height = 200u32;

    let (creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, expiry_height, 5);

    fixture.mine_and_sync(250);

    let txids_before: std::collections::HashSet<_> = fixture
        .sdk
        .transactions()
        .unwrap()
        .into_iter()
        .map(|tx| tx.txid)
        .collect();
    let tx_count_before = txids_before.len();
    let tx_options = default_confirmation_target_tx_options();
    let redeem = fixture
        .sdk
        .redeem_expired_with_tx_options(
            &params,
            &creation_txid,
            params.yes_token_asset,
            5,
            tx_options.clone(),
        )
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::Expired);
    assert_eq!(redeem.tokens_redeemed, 5);
    assert_eq!(redeem.payout_sats, 50_000);
    assert_eq!(redeem.fee.policy, tx_options.fee_policy);
    assert!(redeem.fee.amount_sat > 0);

    let txids_after: std::collections::HashSet<_> = fixture
        .sdk
        .transactions()
        .unwrap()
        .into_iter()
        .map(|tx| tx.txid)
        .collect();
    let tx_count_after = txids_after.len();
    assert_eq!(
        tx_count_after,
        tx_count_before + 2,
        "expected expiry redemption to broadcast auto-expire plus redemption"
    );

    let new_txids: Vec<_> = txids_after.difference(&txids_before).copied().collect();
    assert_eq!(new_txids.len(), 2, "expected exactly two new transactions");
    let redemption_tx = fixture.sdk.fetch_transaction(&redeem.txid).unwrap();
    let expiry_txid = new_txids
        .into_iter()
        .find(|txid| *txid != redeem.txid)
        .expect("expected expiry txid");
    let expiry_tx = fixture.sdk.fetch_transaction(&expiry_txid).unwrap();
    assert!(redeem.fee.amount_sat > actual_fee_amount(&redemption_tx));
    assert!(redeem.fee.amount_sat > actual_fee_amount(&expiry_tx));
    assert_aggregated_fee_metadata(&[expiry_tx, redemption_tx], &redeem.fee);

    fixture.mine_and_sync(1);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
}

#[test]
fn test_expiry_redemption_with_exact_fee() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    // Use a very low expiry height so we can pass it in regtest
    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let expiry_height = 200u32;

    let (creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, expiry_height, 5);

    // Generate blocks past expiry
    fixture.mine_and_sync(250);

    // Redeem YES tokens via expiry path
    let redeem = fixture
        .sdk
        .redeem_expired_with_tx_options(
            &params,
            &creation_txid,
            params.yes_token_asset,
            5,
            exact_fee_tx_options(500),
        )
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::Expired);
    assert_eq!(redeem.tokens_redeemed, 5);
    // Expiry gives 1x payout: 5 * 10_000 = 50_000
    assert_eq!(redeem.payout_sats, 50_000);

    fixture.mine_and_sync(1);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 0);
}

#[test]
fn test_expiry_redemption_with_exact_fee_auto_finalize() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let expiry_height = 200u32;
    let (creation_txid, params) =
        create_and_issue(&mut fixture, oracle_pubkey, 10_000, expiry_height, 5);

    fixture.mine_and_sync(250);

    // Add an exact-fee UTXO after expiry so the implicit expire_market path selects it.
    fixture.fund_and_sync(1, 700);

    let redeem = fixture
        .sdk
        .redeem_expired_with_tx_options(
            &params,
            &creation_txid,
            params.yes_token_asset,
            5,
            exact_fee_tx_options(700),
        )
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::Expired);
    assert_eq!(redeem.tokens_redeemed, 5);
    assert_eq!(redeem.payout_sats, 50_000);

    fixture.mine_and_sync(1);
}

#[test]
fn test_multiple_subsequent_issuances() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(25, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            oracle_pubkey,
            10_000,
            500_000,
            1_000,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let creation_txid = creation.anchor;
    let params = creation.params;

    fixture.mine_and_sync(1);

    // Issue 3 pairs: Dormant → Unresolved
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 3, exact_fee_tx_options(500))
        .unwrap();
    let iss1 = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();
    assert_eq!(iss1.previous_state, MarketState::Dormant);
    assert_eq!(iss1.new_state, MarketState::Unresolved);

    fixture.mine_and_sync(1);

    // Issue 2 more: Unresolved → Unresolved
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 2, exact_fee_tx_options(500))
        .unwrap();
    let iss2 = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();
    assert_eq!(iss2.previous_state, MarketState::Unresolved);

    fixture.mine_and_sync(1);

    // Issue 5 more: Unresolved → Unresolved
    let prepared = fixture
        .sdk
        .prepare_issue_tokens(&params, &creation_txid, 5, exact_fee_tx_options(500))
        .unwrap();
    let iss3 = fixture.sdk.broadcast_prepared_issuance(&prepared).unwrap();
    assert_eq!(iss3.previous_state, MarketState::Unresolved);

    fixture.mine_and_sync(1);

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
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Oracle resolves YES
    let signature = oracle_sign(&params, true, &keypair);
    let prepared = fixture
        .sdk
        .prepare_resolution(
            &params,
            &creation_txid,
            true,
            signature,
            exact_fee_tx_options(500),
        )
        .unwrap();
    fixture
        .sdk
        .broadcast_prepared_resolution(&prepared)
        .unwrap();

    fixture.mine_and_sync(1);

    // Redeem only 3 of 5 YES tokens (partial redemption with token change)
    let prepared = fixture
        .sdk
        .prepare_redeem_tokens(&params, &creation_txid, 3, exact_fee_tx_options(500))
        .unwrap();
    let redeem = fixture
        .sdk
        .broadcast_prepared_redemption(&prepared)
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedYes);
    assert_eq!(redeem.tokens_redeemed, 3);
    // 3 tokens * 2 * 10_000 cpt = 60_000
    assert_eq!(redeem.payout_sats, 60_000);

    fixture.mine_and_sync(1);

    // 5 - 3 = 2 YES tokens should remain as change
    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 2);
}

#[test]
fn test_partial_post_resolution_redemption_with_exact_fee_utxo() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    let signature = oracle_sign(&params, true, &keypair);
    let prepared = fixture
        .sdk
        .prepare_resolution(
            &params,
            &creation_txid,
            true,
            signature,
            exact_fee_tx_options(500),
        )
        .unwrap();
    fixture
        .sdk
        .broadcast_prepared_resolution(&prepared)
        .unwrap();

    fixture.mine_and_sync(1);
    fixture.fund_and_sync(1, 700);

    let prepared = fixture
        .sdk
        .prepare_redeem_tokens(&params, &creation_txid, 3, exact_fee_tx_options(700))
        .unwrap();
    let redeem = fixture
        .sdk
        .broadcast_prepared_redemption(&prepared)
        .unwrap();

    assert_eq!(redeem.previous_state, MarketState::ResolvedYes);
    assert_eq!(redeem.tokens_redeemed, 3);
    assert_eq!(redeem.payout_sats, 60_000);

    fixture.mine_and_sync(1);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 2);
}

#[test]
fn test_cancel_wrong_state() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(10, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let prepared = fixture
        .sdk
        .prepare_contract_creation(
            oracle_pubkey,
            10_000,
            500_000,
            1_000,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let creation = fixture
        .sdk
        .broadcast_prepared_contract_creation(&prepared)
        .unwrap();
    let creation_txid = creation.anchor;
    let params = creation.params;

    fixture.mine_and_sync(1);

    // Market is Dormant (no issuance) — cancellation should fail
    let result =
        fixture
            .sdk
            .prepare_cancel_tokens(&params, &creation_txid, 1, exact_fee_tx_options(500));
    assert!(result.is_err(), "cancel should fail in Dormant state");
}

#[test]
fn test_redeem_wrong_state() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    let (creation_txid, params) = create_and_issue(&mut fixture, oracle_pubkey, 10_000, 500_000, 5);

    // Market is Unresolved — post-resolution redemption should fail
    let result =
        fixture
            .sdk
            .prepare_redeem_tokens(&params, &creation_txid, 5, exact_fee_tx_options(500));
    assert!(result.is_err(), "redeem should fail in Unresolved state");
}

// ── Limit order integration tests ───────────────────────────────────────

/// Helper: create a market, issue tokens, and return the params + asset IDs.
fn issue_market_tokens(
    fixture: &mut TestFixture,
    pairs: u64,
) -> (PredictionMarketAnchor, PredictionMarketParams) {
    let (oracle_pubkey, _keypair) = generate_oracle_keypair();
    create_and_issue(fixture, oracle_pubkey, 10_000, 500_000, pairs)
}

#[test]
fn test_create_and_cancel_limit_order() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(20, 500_000);

    let (_creation_txid, params) = issue_market_tokens(&mut fixture, 10);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let lbtc = regtest_policy_asset();

    // Verify we have 10 YES tokens
    let balance_before = fixture.sdk.balance().unwrap();
    assert_eq!(*balance_before.get(&yes_asset).unwrap_or(&0), 10);
    let _lbtc_before = *balance_before.get(&lbtc).unwrap();

    // Create a SellBase limit order: sell 5 YES tokens at price 10 sats/token
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let order_index = 11u32;
    let prepared = fixture
        .sdk
        .prepare_create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            10, // price: 10 sats per YES token
            5,  // order_amount: 5 YES tokens
            OrderDirection::SellBase,
            1, // min_fill_lots
            1, // min_remainder_lots
            order_index,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let create_result = fixture
        .sdk
        .broadcast_prepared_create_order(&prepared)
        .unwrap();

    assert_eq!(create_result.order_amount, 5);
    assert!(!create_result.covenant_address.is_empty());

    fixture.mine_and_sync(1);

    // 5 YES tokens should be locked in the covenant, 5 remain in wallet
    let balance_after_create = fixture.sdk.balance().unwrap();
    assert_eq!(*balance_after_create.get(&yes_asset).unwrap_or(&0), 5);

    // Cancel the order — should refund the 5 YES tokens
    let prepared = fixture
        .sdk
        .prepare_cancel_limit_order(
            &create_result.order_params,
            create_result.maker_base_pubkey,
            order_index,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let cancel_result = fixture
        .sdk
        .broadcast_prepared_cancel_order(&prepared)
        .unwrap();

    assert_eq!(cancel_result.refunded_amount, 5);

    fixture.mine_and_sync(1);

    // All 10 YES tokens should be back in the wallet
    let balance_after_cancel = fixture.sdk.balance().unwrap();
    assert_eq!(*balance_after_cancel.get(&yes_asset).unwrap_or(&0), 10);
}

#[test]
fn test_create_and_full_fill_limit_order() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(25, 500_000);

    let (_creation_txid, params) = issue_market_tokens(&mut fixture, 10);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let lbtc = regtest_policy_asset();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();

    // Create a SellBase limit order: sell 5 YES tokens at price 10 sats/token
    let order_index = 12u32;
    let prepared = fixture
        .sdk
        .prepare_create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            10, // price
            5,  // order_amount
            OrderDirection::SellBase,
            1, // min_fill_lots
            1, // min_remainder_lots
            order_index,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let create_result = fixture
        .sdk
        .broadcast_prepared_create_order(&prepared)
        .unwrap();

    fixture.mine_and_sync(1);

    // Fill the entire order (self-fill: same wallet pays L-BTC to get YES tokens back)
    let prepared = fixture
        .sdk
        .prepare_fill_limit_order(
            &create_result.order_params,
            create_result.maker_base_pubkey,
            create_result.order_nonce,
            5, // lots_to_fill: fill all 5
            exact_fee_tx_options(500),
        )
        .unwrap();
    let fill_result = fixture
        .sdk
        .broadcast_prepared_fill_order(&prepared)
        .unwrap();

    assert_eq!(fill_result.lots_filled, 5);
    assert!(!fill_result.is_partial);

    fixture.mine_and_sync(1);

    // All 10 YES tokens should be back in the wallet (5 never left + 5 received from fill)
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 10);

    // L-BTC should have decreased by the payment amount (5 * 10 = 50) plus fees.
    // The maker receives 50 sats at the P_order address (not in our wallet),
    // so our L-BTC goes down by ~50 + fees.
}

#[test]
fn test_partial_fill_limit_order() {
    let mut fixture = TestFixture::new();
    fixture.fund_and_sync(25, 500_000);

    let (_creation_txid, params) = issue_market_tokens(&mut fixture, 10);

    let yes_asset = lwk_wollet::elements::AssetId::from_slice(&params.yes_token_asset).unwrap();
    let lbtc = regtest_policy_asset();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();

    // Create a SellBase limit order: sell 5 YES tokens at price 10 sats/token
    let order_index = 13u32;
    let prepared = fixture
        .sdk
        .prepare_create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            10, // price
            5,  // order_amount
            OrderDirection::SellBase,
            1, // min_fill_lots
            1, // min_remainder_lots
            order_index,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let create_result = fixture
        .sdk
        .broadcast_prepared_create_order(&prepared)
        .unwrap();

    fixture.mine_and_sync(1);

    // Partial fill: take only 3 of the 5 tokens
    let prepared = fixture
        .sdk
        .prepare_fill_limit_order(
            &create_result.order_params,
            create_result.maker_base_pubkey,
            create_result.order_nonce,
            3, // lots_to_fill: partial — 3 of 5
            exact_fee_tx_options(500),
        )
        .unwrap();
    let fill_result = fixture
        .sdk
        .broadcast_prepared_fill_order(&prepared)
        .unwrap();

    assert_eq!(fill_result.lots_filled, 3);
    assert!(fill_result.is_partial);

    fixture.mine_and_sync(1);

    // Wallet should have: 5 (kept) + 3 (received from fill) = 8 YES tokens
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 8);

    // The remaining 2 YES tokens are still locked in the covenant.
    // Cancel the remainder.
    let prepared = fixture
        .sdk
        .prepare_cancel_limit_order(
            &create_result.order_params,
            create_result.maker_base_pubkey,
            order_index,
            exact_fee_tx_options(500),
        )
        .unwrap();
    let cancel_result = fixture
        .sdk
        .broadcast_prepared_cancel_order(&prepared)
        .unwrap();

    assert_eq!(cancel_result.refunded_amount, 2);

    fixture.mine_and_sync(1);

    // All 10 tokens back in wallet
    let balance = fixture.sdk.balance().unwrap();
    assert_eq!(*balance.get(&yes_asset).unwrap_or(&0), 10);
}
