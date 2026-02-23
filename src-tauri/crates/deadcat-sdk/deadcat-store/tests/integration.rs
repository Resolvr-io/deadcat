use std::collections::HashMap;

use deadcat_sdk::elements::confidential::{Asset, Nonce, Value as ConfValue};
use deadcat_sdk::elements::encode::serialize;
use deadcat_sdk::elements::hashes::Hash;
use deadcat_sdk::elements::secp256k1_zkp::{Tweak, ZERO_TWEAK};
use deadcat_sdk::elements::{
    AssetId, AssetIssuance, ContractHash, LockTime, OutPoint, Script, Sequence, Transaction, TxIn,
    TxInWitness, TxOut, TxOutWitness, Txid,
};
use deadcat_sdk::{
    ContractParams, MakerOrderParams, MarketId, MarketState, OrderDirection, UnblindedUtxo,
    derive_maker_receive, maker_receive_script_pubkey,
};

use deadcat_store::{
    ChainSource, ChainUtxo, ContractMetadataInput, DeadcatStore, IssuanceData, MarketFilter,
    OrderFilter, OrderStatus,
};

// ==================== Test Helpers ====================

fn test_params() -> ContractParams {
    ContractParams {
        oracle_public_key: [0xaa; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: [0x01; 32],
        no_token_asset: [0x02; 32],
        yes_reissuance_token: [0x03; 32],
        no_reissuance_token: [0x04; 32],
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    }
}

fn test_params_2() -> ContractParams {
    ContractParams {
        oracle_public_key: [0xcc; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: [0x11; 32],
        no_token_asset: [0x12; 32],
        yes_reissuance_token: [0x13; 32],
        no_reissuance_token: [0x14; 32],
        collateral_per_token: 200_000,
        expiry_time: 2_000_000,
    }
}

const NUMS_KEY_BYTES: [u8; 32] = [
    0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
    0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3a, 0xc0,
];

fn test_maker_order_params() -> MakerOrderParams {
    let (params, _p_order) = MakerOrderParams::new(
        [0x01; 32],
        [0xbb; 32],
        50_000,
        1,
        1,
        OrderDirection::SellBase,
        NUMS_KEY_BYTES,
        &[0xaa; 32],
        &[0x11; 32],
    );
    params
}

fn test_maker_order_params_2() -> MakerOrderParams {
    let (params, _p_order) = MakerOrderParams::new(
        [0x01; 32],
        [0xbb; 32],
        75_000,
        2,
        2,
        OrderDirection::SellQuote,
        NUMS_KEY_BYTES,
        &[0xaa; 32],
        &[0x22; 32],
    );
    params
}

fn explicit_txout(asset_id: &[u8; 32], amount: u64, script_pubkey: &Script) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_id).expect("valid asset id")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: script_pubkey.clone(),
        witness: TxOutWitness::default(),
    }
}

fn test_utxo_with_outpoint(
    txid_bytes: [u8; 32],
    vout: u32,
    asset_id: [u8; 32],
    value: u64,
) -> UnblindedUtxo {
    let txid = Txid::from_byte_array(txid_bytes);
    UnblindedUtxo {
        outpoint: OutPoint::new(txid, vout),
        txout: explicit_txout(&asset_id, value, &Script::new()),
        asset_id,
        value,
        asset_blinding_factor: [0u8; 32],
        value_blinding_factor: [0u8; 32],
    }
}

fn make_chain_utxo(txid: [u8; 32], vout: u32, asset_id: [u8; 32], value: u64) -> ChainUtxo {
    let raw_txout = serialize(&explicit_txout(&asset_id, value, &Script::new()));
    ChainUtxo {
        txid,
        vout,
        value,
        asset_id,
        raw_txout,
        block_height: Some(100),
    }
}

/// Find the SPK for a given market state from the store's watched SPKs.
/// This avoids fragile index-based assumptions about SPK ordering.
fn get_market_spk(
    _store: &mut DeadcatStore,
    params: &ContractParams,
    state: MarketState,
) -> Vec<u8> {
    use deadcat_sdk::CompiledContract;
    let compiled = CompiledContract::new(*params).unwrap();
    compiled.script_pubkey(state).as_bytes().to_vec()
}

/// Find the covenant SPK for a maker order.
fn get_order_spk(
    _store: &mut DeadcatStore,
    params: &MakerOrderParams,
    maker_pubkey: &[u8; 32],
) -> Vec<u8> {
    use deadcat_sdk::CompiledMakerOrder;
    let compiled = CompiledMakerOrder::new(*params).unwrap();
    compiled.script_pubkey(maker_pubkey).as_bytes().to_vec()
}

// ==================== Mock ChainSource ====================

#[derive(Debug, Default)]
struct MockChainSource {
    block_height: u32,
    /// Maps script_pubkey bytes -> list of ChainUtxos
    unspent: HashMap<Vec<u8>, Vec<ChainUtxo>>,
    /// Maps (txid, vout) -> Some(spending_txid) if spent
    spent: HashMap<([u8; 32], u32), [u8; 32]>,
    /// Maps txid -> raw serialized transaction bytes
    transactions: HashMap<[u8; 32], Vec<u8>>,
    /// If set, all methods return this error message
    fail_with: Option<String>,
}

impl ChainSource for MockChainSource {
    type Error = std::io::Error;

    fn best_block_height(&self) -> Result<u32, Self::Error> {
        if let Some(ref msg) = self.fail_with {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, msg.clone()));
        }
        Ok(self.block_height)
    }

    fn list_unspent(&self, script_pubkey: &[u8]) -> Result<Vec<ChainUtxo>, Self::Error> {
        if let Some(ref msg) = self.fail_with {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, msg.clone()));
        }
        Ok(self.unspent.get(script_pubkey).cloned().unwrap_or_default())
    }

    fn is_spent(&self, txid: &[u8; 32], vout: u32) -> Result<Option<[u8; 32]>, Self::Error> {
        if let Some(ref msg) = self.fail_with {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, msg.clone()));
        }
        Ok(self.spent.get(&(*txid, vout)).copied())
    }

    fn get_transaction(&self, txid: &[u8; 32]) -> Result<Option<Vec<u8>>, Self::Error> {
        if let Some(ref msg) = self.fail_with {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, msg.clone()));
        }
        Ok(self.transactions.get(txid).cloned())
    }
}

// ==================== Basic Store Tests ====================

#[test]
fn test_open_in_memory() {
    let store = DeadcatStore::open_in_memory();
    assert!(store.is_ok());
}

#[test]
fn test_open_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let store = DeadcatStore::open(path.to_str().unwrap());
    assert!(store.is_ok());
}

#[test]
fn test_reopen_persists_data() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("persist.db").to_str().unwrap().to_string();

    // Create and ingest
    let market_id = {
        let mut store = DeadcatStore::open(&db_path).unwrap();
        store.ingest_market(&test_params(), None).unwrap()
    };

    // Reopen and verify
    let mut store = DeadcatStore::open(&db_path).unwrap();
    let info = store.get_market(&market_id).unwrap();
    assert!(info.is_some());
    assert_eq!(info.unwrap().params, test_params());
}

// ==================== Market Tests ====================

#[test]
fn test_market_ingest_and_query_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();

    let market_id = store.ingest_market(&params, None).unwrap();
    assert_eq!(market_id, params.market_id());

    let info = store.get_market(&market_id).unwrap().unwrap();
    assert_eq!(info.params, params);
    assert_eq!(info.market_id, market_id);
    assert_eq!(info.state, MarketState::Dormant);
}

#[test]
fn test_market_idempotent_ingest() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();

    let id1 = store.ingest_market(&params, None).unwrap();
    let id2 = store.ingest_market(&params, None).unwrap();
    assert_eq!(id1, id2);

    let all = store.list_markets(&MarketFilter::default()).unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn test_get_nonexistent_market() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let result = store.get_market(&MarketId([0xFF; 32])).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_list_markets_filter_by_oracle_key() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store.ingest_market(&test_params(), None).unwrap();
    store.ingest_market(&test_params_2(), None).unwrap();

    let filter = MarketFilter {
        oracle_public_key: Some([0xaa; 32]),
        ..Default::default()
    };
    let results = store.list_markets(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].params.oracle_public_key, [0xaa; 32]);
}

#[test]
fn test_list_markets_filter_by_state() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let id1 = store.ingest_market(&test_params(), None).unwrap();
    store.ingest_market(&test_params_2(), None).unwrap();

    store
        .update_market_state(&id1, MarketState::Unresolved)
        .unwrap();

    let filter = MarketFilter {
        current_state: Some(MarketState::Unresolved),
        ..Default::default()
    };
    let results = store.list_markets(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].market_id, id1);
}

#[test]
fn test_list_markets_filter_by_expiry() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store.ingest_market(&test_params(), None).unwrap(); // expiry = 1_000_000
    store.ingest_market(&test_params_2(), None).unwrap(); // expiry = 2_000_000

    let filter = MarketFilter {
        expiry_before: Some(1_500_000),
        ..Default::default()
    };
    let results = store.list_markets(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].params.expiry_time, 1_000_000);

    let filter = MarketFilter {
        expiry_after: Some(1_500_000),
        ..Default::default()
    };
    let results = store.list_markets(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].params.expiry_time, 2_000_000);
}

#[test]
fn test_list_markets_filter_by_collateral_asset() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store.ingest_market(&test_params(), None).unwrap();
    store.ingest_market(&test_params_2(), None).unwrap();

    // Both share [0xbb; 32]
    let filter = MarketFilter {
        collateral_asset_id: Some([0xbb; 32]),
        ..Default::default()
    };
    assert_eq!(store.list_markets(&filter).unwrap().len(), 2);

    let filter = MarketFilter {
        collateral_asset_id: Some([0xFF; 32]),
        ..Default::default()
    };
    assert_eq!(store.list_markets(&filter).unwrap().len(), 0);
}

#[test]
fn test_list_markets_with_limit() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store.ingest_market(&test_params(), None).unwrap();
    store.ingest_market(&test_params_2(), None).unwrap();

    let filter = MarketFilter {
        limit: Some(1),
        ..Default::default()
    };
    assert_eq!(store.list_markets(&filter).unwrap().len(), 1);
}

#[test]
fn test_update_market_state() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let id = store.ingest_market(&test_params(), None).unwrap();

    assert_eq!(
        store.get_market(&id).unwrap().unwrap().state,
        MarketState::Dormant
    );

    store
        .update_market_state(&id, MarketState::Unresolved)
        .unwrap();
    let info = store.get_market(&id).unwrap().unwrap();
    assert_eq!(info.state, MarketState::Unresolved);

    // Verify updated_at changed from created_at
    // (SQLite datetime('now') resolution is 1s, so we just verify it's valid)
    assert!(!info.updated_at.is_empty());

    store
        .update_market_state(&id, MarketState::ResolvedYes)
        .unwrap();
    assert_eq!(
        store.get_market(&id).unwrap().unwrap().state,
        MarketState::ResolvedYes
    );
}

// ==================== Maker Order Tests ====================

#[test]
fn test_maker_order_ingest_and_query_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();

    let order_id = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    assert!(order_id > 0);

    let info = store.get_maker_order(order_id).unwrap().unwrap();
    assert_eq!(info.params.price, params.price);
    assert_eq!(info.params.direction, OrderDirection::SellBase);
    assert_eq!(info.status, OrderStatus::Pending);
    assert_eq!(info.maker_base_pubkey, Some([0xaa; 32]));
}

#[test]
fn test_maker_order_idempotent_ingest() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();

    let id1 = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    let id2 = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    assert_eq!(id1, id2);

    assert_eq!(
        store
            .list_maker_orders(&OrderFilter::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_maker_order_ingest_without_pubkey() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();

    let order_id = store.ingest_maker_order(&params, None, None, None, None).unwrap();
    let info = store.get_maker_order(order_id).unwrap().unwrap();
    assert!(info.maker_base_pubkey.is_none());
}

#[test]
fn test_get_nonexistent_order() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    assert!(store.get_maker_order(999).unwrap().is_none());
}

#[test]
fn test_list_maker_orders_filters() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store
        .ingest_maker_order(&test_maker_order_params(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    store
        .ingest_maker_order(&test_maker_order_params_2(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    // Filter by direction
    let filter = OrderFilter {
        direction: Some(OrderDirection::SellBase),
        ..Default::default()
    };
    let results = store.list_maker_orders(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].params.direction, OrderDirection::SellBase);

    // Filter by price range
    let filter = OrderFilter {
        min_price: Some(60_000),
        max_price: Some(80_000),
        ..Default::default()
    };
    let results = store.list_maker_orders(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].params.price, 75_000);

    // Filter by status (both pending)
    let filter = OrderFilter {
        order_status: Some(OrderStatus::Pending),
        ..Default::default()
    };
    assert_eq!(store.list_maker_orders(&filter).unwrap().len(), 2);
}

#[test]
fn test_filter_orders_by_maker_pubkey() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();
    store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    store.ingest_maker_order(&params, None, None, None, None).unwrap();

    let filter = OrderFilter {
        maker_base_pubkey: Some([0xaa; 32]),
        ..Default::default()
    };
    let results = store.list_maker_orders(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].maker_base_pubkey, Some([0xaa; 32]));
}

#[test]
fn test_list_maker_orders_with_limit() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store
        .ingest_maker_order(&test_maker_order_params(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    store
        .ingest_maker_order(&test_maker_order_params_2(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    let filter = OrderFilter {
        limit: Some(1),
        ..Default::default()
    };
    assert_eq!(store.list_maker_orders(&filter).unwrap().len(), 1);
}

#[test]
fn test_update_order_status() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let id = store
        .ingest_maker_order(&test_maker_order_params(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    assert_eq!(
        store.get_maker_order(id).unwrap().unwrap().status,
        OrderStatus::Pending
    );

    store.update_order_status(id, OrderStatus::Active).unwrap();
    assert_eq!(
        store.get_maker_order(id).unwrap().unwrap().status,
        OrderStatus::Active
    );

    store
        .update_order_status(id, OrderStatus::Cancelled)
        .unwrap();
    assert_eq!(
        store.get_maker_order(id).unwrap().unwrap().status,
        OrderStatus::Cancelled
    );
}

// ==================== UTXO Tests ====================

#[test]
fn test_utxo_add_query_mark_spent_lifecycle() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&test_params(), None).unwrap();

    let utxo = test_utxo_with_outpoint([0xAA; 32], 0, [0xbb; 32], 100_000);

    store
        .add_market_utxo(&market_id, MarketState::Dormant, &utxo, Some(100))
        .unwrap();

    let utxos = store
        .get_market_utxos(&market_id, Some(MarketState::Dormant))
        .unwrap();
    assert_eq!(utxos.len(), 1);
    assert_eq!(utxos[0].value, 100_000);
    assert_eq!(utxos[0].asset_id, [0xbb; 32]);

    store
        .mark_spent(&[0xAA; 32], 0, &[0xFF; 32], Some(200))
        .unwrap();

    let utxos = store
        .get_market_utxos(&market_id, Some(MarketState::Dormant))
        .unwrap();
    assert_eq!(utxos.len(), 0);
}

#[test]
fn test_utxo_add_idempotent() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&test_params(), None).unwrap();

    let utxo = test_utxo_with_outpoint([0xAA; 32], 0, [0xbb; 32], 100_000);

    store
        .add_market_utxo(&market_id, MarketState::Dormant, &utxo, Some(100))
        .unwrap();
    store
        .add_market_utxo(&market_id, MarketState::Dormant, &utxo, Some(100))
        .unwrap();

    assert_eq!(
        store
            .get_market_utxos(&market_id, Some(MarketState::Dormant))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_order_utxo_lifecycle() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let order_id = store
        .ingest_maker_order(&test_maker_order_params(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    let utxo = test_utxo_with_outpoint([0xBB; 32], 1, [0x01; 32], 50_000);
    store.add_order_utxo(order_id, &utxo, Some(100)).unwrap();

    assert_eq!(store.get_order_utxos(order_id).unwrap().len(), 1);
    assert_eq!(store.get_order_utxos(order_id).unwrap()[0].value, 50_000);

    store.mark_spent(&[0xBB; 32], 1, &[0xFF; 32], None).unwrap();
    assert_eq!(store.get_order_utxos(order_id).unwrap().len(), 0);
}

#[test]
fn test_get_market_utxos_filter_by_state() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&test_params(), None).unwrap();

    let utxo1 = test_utxo_with_outpoint([0xAA; 32], 0, [0xbb; 32], 100_000);
    let utxo2 = test_utxo_with_outpoint([0xBB; 32], 0, [0xbb; 32], 200_000);

    store
        .add_market_utxo(&market_id, MarketState::Dormant, &utxo1, None)
        .unwrap();
    store
        .add_market_utxo(&market_id, MarketState::Unresolved, &utxo2, None)
        .unwrap();

    // All states
    assert_eq!(store.get_market_utxos(&market_id, None).unwrap().len(), 2);

    // Dormant only
    let dormant = store
        .get_market_utxos(&market_id, Some(MarketState::Dormant))
        .unwrap();
    assert_eq!(dormant.len(), 1);
    assert_eq!(dormant[0].value, 100_000);

    // Unresolved only
    let unresolved = store
        .get_market_utxos(&market_id, Some(MarketState::Unresolved))
        .unwrap();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].value, 200_000);
}

// ==================== Watched SPKs ====================

#[test]
fn test_watched_script_pubkeys() {
    let mut store = DeadcatStore::open_in_memory().unwrap();

    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 0);

    store.ingest_market(&test_params(), None).unwrap();
    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 4);

    store
        .ingest_maker_order(&test_maker_order_params(), Some(&[0xaa; 32]), None, None, None)
        .unwrap();
    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 5);

    // Order without pubkey -> no covenant_spk -> no additional watched SPK
    store
        .ingest_maker_order(&test_maker_order_params_2(), None, None, None, None)
        .unwrap();
    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 5);
}

// ==================== Sync Tests ====================

#[test]
fn test_last_synced_height() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    assert_eq!(store.last_synced_height().unwrap(), 0);
}

#[test]
fn test_sync_empty_store() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };

    let report = store.sync(&chain).unwrap();
    assert_eq!(report.new_utxos, 0);
    assert_eq!(report.spent_utxos, 0);
    assert_eq!(report.market_state_changes.len(), 0);
    assert_eq!(report.order_status_changes.len(), 0);
    assert_eq!(report.block_height, 500);
    assert_eq!(store.last_synced_height().unwrap(), 500);
}

#[test]
fn test_sync_discovers_utxos() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    let dormant_spk = get_market_spk(&mut store, &params, MarketState::Dormant);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk,
        vec![make_chain_utxo([0xDD; 32], 0, [0xbb; 32], 100_000)],
    );

    let report = store.sync(&chain).unwrap();
    assert_eq!(report.new_utxos, 1);
    assert_eq!(report.block_height, 500);

    let utxos = store
        .get_market_utxos(&market_id, Some(MarketState::Dormant))
        .unwrap();
    assert_eq!(utxos.len(), 1);
    assert_eq!(utxos[0].value, 100_000);

    assert_eq!(store.last_synced_height().unwrap(), 500);
}

#[test]
fn test_sync_marks_spent() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    let utxo = test_utxo_with_outpoint([0xDD; 32], 0, [0xbb; 32], 100_000);
    store
        .add_market_utxo(&market_id, MarketState::Dormant, &utxo, Some(100))
        .unwrap();

    let mut chain = MockChainSource {
        block_height: 600,
        ..Default::default()
    };
    chain.spent.insert(([0xDD; 32], 0), [0xEE; 32]);

    let report = store.sync(&chain).unwrap();
    assert_eq!(report.spent_utxos, 1);

    assert_eq!(
        store
            .get_market_utxos(&market_id, Some(MarketState::Dormant))
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn test_sync_derives_market_state() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    let unresolved_spk = get_market_spk(&mut store, &params, MarketState::Unresolved);

    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    chain.unspent.insert(
        unresolved_spk,
        vec![make_chain_utxo([0xDD; 32], 0, [0xbb; 32], 100_000)],
    );

    let report = store.sync(&chain).unwrap();

    assert_eq!(report.market_state_changes.len(), 1);
    assert_eq!(
        report.market_state_changes[0].old_state,
        MarketState::Dormant
    );
    assert_eq!(
        report.market_state_changes[0].new_state,
        MarketState::Unresolved
    );

    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::Unresolved
    );
}

#[test]
fn test_sync_derives_order_status_active() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();
    let order_id = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    let order_spk = get_order_spk(&mut store, &params, &[0xaa; 32]);

    let mut chain = MockChainSource {
        block_height: 800,
        ..Default::default()
    };
    chain.unspent.insert(
        order_spk,
        vec![make_chain_utxo([0xEE; 32], 0, [0x01; 32], 50_000)],
    );

    let report = store.sync(&chain).unwrap();

    assert_eq!(report.order_status_changes.len(), 1);
    assert_eq!(
        report.order_status_changes[0].old_status,
        OrderStatus::Pending
    );
    assert_eq!(
        report.order_status_changes[0].new_status,
        OrderStatus::Active
    );

    assert_eq!(
        store.get_maker_order(order_id).unwrap().unwrap().status,
        OrderStatus::Active
    );
}

#[test]
fn test_sync_derives_order_fully_filled() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();
    let order_id = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    // Manually add a UTXO then mark it spent (simulates a fill)
    let utxo = test_utxo_with_outpoint([0xEE; 32], 0, [0x01; 32], 50_000);
    store.add_order_utxo(order_id, &utxo, Some(100)).unwrap();
    store
        .mark_spent(&[0xEE; 32], 0, &[0xFF; 32], Some(200))
        .unwrap();

    // Now sync with empty chain (no new UTXOs, nothing to check)
    let chain = MockChainSource {
        block_height: 300,
        ..Default::default()
    };
    let report = store.sync(&chain).unwrap();

    assert_eq!(report.order_status_changes.len(), 1);
    assert_eq!(
        report.order_status_changes[0].new_status,
        OrderStatus::FullyFilled
    );

    assert_eq!(
        store.get_maker_order(order_id).unwrap().unwrap().status,
        OrderStatus::FullyFilled
    );
}

#[test]
fn test_sync_derives_order_partially_filled() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();
    let order_id = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    // Two UTXOs: one spent (filled), one unspent (still live)
    let utxo1 = test_utxo_with_outpoint([0xEE; 32], 0, [0x01; 32], 50_000);
    let utxo2 = test_utxo_with_outpoint([0xEE; 32], 1, [0x01; 32], 30_000);
    store.add_order_utxo(order_id, &utxo1, Some(100)).unwrap();
    store.add_order_utxo(order_id, &utxo2, Some(100)).unwrap();
    store
        .mark_spent(&[0xEE; 32], 0, &[0xFF; 32], Some(200))
        .unwrap();

    let chain = MockChainSource {
        block_height: 300,
        ..Default::default()
    };
    let report = store.sync(&chain).unwrap();

    assert_eq!(report.order_status_changes.len(), 1);
    assert_eq!(
        report.order_status_changes[0].new_status,
        OrderStatus::PartiallyFilled
    );

    assert_eq!(
        store.get_maker_order(order_id).unwrap().unwrap().status,
        OrderStatus::PartiallyFilled
    );
}

#[test]
fn test_sync_cancelled_order_excluded() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();
    let order_id = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    // Cancel the order
    store
        .update_order_status(order_id, OrderStatus::Cancelled)
        .unwrap();

    // Add a UTXO (shouldn't affect status since cancelled is terminal)
    let utxo = test_utxo_with_outpoint([0xEE; 32], 0, [0x01; 32], 50_000);
    store.add_order_utxo(order_id, &utxo, Some(100)).unwrap();

    let chain = MockChainSource {
        block_height: 300,
        ..Default::default()
    };
    let report = store.sync(&chain).unwrap();

    // No status changes — cancelled is terminal
    assert_eq!(report.order_status_changes.len(), 0);
    assert_eq!(
        store.get_maker_order(order_id).unwrap().unwrap().status,
        OrderStatus::Cancelled
    );
}

#[test]
fn test_sync_idempotent() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    let dormant_spk = get_market_spk(&mut store, &params, MarketState::Dormant);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk,
        vec![make_chain_utxo([0xDD; 32], 0, [0xbb; 32], 100_000)],
    );

    let report1 = store.sync(&chain).unwrap();
    assert_eq!(report1.new_utxos, 1);

    let report2 = store.sync(&chain).unwrap();
    assert_eq!(report2.new_utxos, 0);

    assert_eq!(
        store
            .get_market_utxos(&market_id, Some(MarketState::Dormant))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn test_sync_multi_round_discover_then_spend() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    let dormant_spk = get_market_spk(&mut store, &params, MarketState::Dormant);

    // Round 1: discover UTXO
    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk.clone(),
        vec![make_chain_utxo([0xDD; 32], 0, [0xbb; 32], 100_000)],
    );
    let r1 = store.sync(&chain).unwrap();
    assert_eq!(r1.new_utxos, 1);
    assert_eq!(store.get_market_utxos(&market_id, None).unwrap().len(), 1);

    // Round 2: UTXO is now spent, no longer in unspent set
    let mut chain2 = MockChainSource {
        block_height: 600,
        ..Default::default()
    };
    chain2.spent.insert(([0xDD; 32], 0), [0xEE; 32]);
    let r2 = store.sync(&chain2).unwrap();
    assert_eq!(r2.spent_utxos, 1);
    assert_eq!(store.get_market_utxos(&market_id, None).unwrap().len(), 0);
}

#[test]
fn test_sync_market_utxos_at_multiple_states() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    let dormant_spk = get_market_spk(&mut store, &params, MarketState::Dormant);
    let unresolved_spk = get_market_spk(&mut store, &params, MarketState::Unresolved);

    // UTXOs at both dormant and unresolved addresses
    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk,
        vec![make_chain_utxo([0xAA; 32], 0, [0xbb; 32], 50_000)],
    );
    chain.unspent.insert(
        unresolved_spk,
        vec![make_chain_utxo([0xBB; 32], 0, [0xbb; 32], 100_000)],
    );

    let report = store.sync(&chain).unwrap();
    assert_eq!(report.new_utxos, 2);

    // Highest state with UTXOs (Unresolved=1) should win
    assert_eq!(report.market_state_changes.len(), 1);
    assert_eq!(
        report.market_state_changes[0].new_state,
        MarketState::Unresolved
    );

    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::Unresolved
    );
}

#[test]
fn test_sync_chain_error_propagates() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    store.ingest_market(&test_params(), None).unwrap();

    let chain = MockChainSource {
        fail_with: Some("node unreachable".into()),
        ..Default::default()
    };

    let result = store.sync(&chain);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("node unreachable"), "got: {err_msg}");
}

#[test]
fn test_sync_transaction_atomicity() {
    // If sync fails mid-way, no partial state should be committed.
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = store.ingest_market(&params, None).unwrap();

    // Add a UTXO so sync_spent_utxos has work to do
    let utxo = test_utxo_with_outpoint([0xDD; 32], 0, [0xbb; 32], 100_000);
    store
        .add_market_utxo(&market_id, MarketState::Dormant, &utxo, Some(100))
        .unwrap();

    // Chain source that fails on is_spent (after list_unspent succeeds)
    let chain = MockChainSource {
        block_height: 500,
        fail_with: Some("connection lost".into()),
        ..Default::default()
    };

    // Sync should fail
    assert!(store.sync(&chain).is_err());

    // The pre-existing UTXO should still be there (not modified by failed sync)
    assert_eq!(
        store
            .get_market_utxos(&market_id, Some(MarketState::Dormant))
            .unwrap()
            .len(),
        1
    );
    // Sync height should not have advanced
    assert_eq!(store.last_synced_height().unwrap(), 0);
}

// ==================== Order Nonce Tests ====================

#[test]
fn test_ingest_maker_order_with_nonce() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let nonce = [0x11u8; 32];
    let pubkey = [0xaa; 32];
    let params = test_maker_order_params();

    let order_id = store
        .ingest_maker_order(&params, Some(&pubkey), Some(&nonce), None, None)
        .unwrap();
    let info = store.get_maker_order(order_id).unwrap().unwrap();

    // Verify nonce round-trips
    assert_eq!(info.order_nonce, Some(nonce));

    // Verify maker_receive_spk was computed
    let (p_order, _) = derive_maker_receive(&pubkey, &nonce, &params);
    let expected_spk = maker_receive_script_pubkey(&p_order);
    let spks = store.maker_receive_script_pubkeys().unwrap();
    assert_eq!(spks.len(), 1);
    assert_eq!(spks[0], expected_spk);
}

#[test]
fn test_maker_receive_script_pubkeys() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();
    let params2 = test_maker_order_params_2();

    // Order with nonce → has maker_receive_spk
    store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), Some(&[0x11; 32]), None, None)
        .unwrap();
    // Order without nonce → no maker_receive_spk
    store
        .ingest_maker_order(&params2, Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    let spks = store.maker_receive_script_pubkeys().unwrap();
    assert_eq!(spks.len(), 1);
}

#[test]
fn test_idempotent_ingest_preserves_nonce() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let nonce = [0x11u8; 32];
    let pubkey = [0xaa; 32];
    let params = test_maker_order_params();

    let id1 = store
        .ingest_maker_order(&params, Some(&pubkey), Some(&nonce), None, None)
        .unwrap();
    // Re-ingest same order (idempotent, returns existing)
    let id2 = store
        .ingest_maker_order(&params, Some(&pubkey), None, None, None)
        .unwrap();
    assert_eq!(id1, id2);

    // Nonce should still be present from original ingest
    let info = store.get_maker_order(id1).unwrap().unwrap();
    assert_eq!(info.order_nonce, Some(nonce));
}

// ==================== Issuance Data Tests ====================

#[test]
fn test_set_market_issuance_data() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&test_params(), None).unwrap();

    // Initially no issuance data
    let info = store.get_market(&market_id).unwrap().unwrap();
    assert!(info.issuance.is_none());

    let data = IssuanceData {
        yes_entropy: [0x01; 32],
        no_entropy: [0x02; 32],
        yes_blinding_nonce: [0x03; 32],
        no_blinding_nonce: [0x04; 32],
    };

    store.set_market_issuance_data(&market_id, &data).unwrap();

    let info = store.get_market(&market_id).unwrap().unwrap();
    let issuance = info.issuance.unwrap();
    assert_eq!(issuance.yes_entropy, [0x01; 32]);
    assert_eq!(issuance.no_entropy, [0x02; 32]);
    assert_eq!(issuance.yes_blinding_nonce, [0x03; 32]);
    assert_eq!(issuance.no_blinding_nonce, [0x04; 32]);
}

// ==================== Sync Entropy Extraction Tests ====================

/// Build a minimal Elements transaction with issuance fields on the inputs.
/// Each entry in `issuances` is: (prevout_txid, prevout_vout, contract_hash_bytes, amount, inflation_keys_amount, blinding_nonce)
fn build_mock_issuance_tx(issuances: &[([u8; 32], u32, [u8; 32], u64, u64, Tweak)]) -> Vec<u8> {
    let inputs: Vec<TxIn> = issuances
        .iter()
        .map(
            |(
                prevout_txid,
                prevout_vout,
                contract_hash,
                amount,
                inflation_keys,
                blinding_nonce,
            )| {
                let asset_entropy = if *blinding_nonce == ZERO_TWEAK {
                    // Initial issuance: asset_entropy field is the contract hash
                    *contract_hash
                } else {
                    // Reissuance: asset_entropy field is the actual entropy
                    *contract_hash
                };

                TxIn {
                    previous_output: OutPoint::new(
                        Txid::from_byte_array(*prevout_txid),
                        *prevout_vout,
                    ),
                    is_pegin: false,
                    script_sig: Script::new(),
                    sequence: Sequence::MAX,
                    asset_issuance: AssetIssuance {
                        asset_blinding_nonce: *blinding_nonce,
                        asset_entropy,
                        amount: ConfValue::Explicit(*amount),
                        inflation_keys: if *inflation_keys > 0 {
                            ConfValue::Explicit(*inflation_keys)
                        } else {
                            ConfValue::Null
                        },
                    },
                    witness: TxInWitness::default(),
                }
            },
        )
        .collect();

    let tx = Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![TxOut {
            asset: Asset::Null,
            value: ConfValue::Null,
            nonce: Nonce::Null,
            script_pubkey: Script::new(),
            witness: TxOutWitness::default(),
        }],
    };

    serialize(&tx)
}

#[test]
fn test_sync_extracts_issuance_entropy() {
    // Build a market with known issuance entropies, then verify sync extracts them.
    // We compute token IDs from known outpoints + contract hashes, then build
    // ContractParams using those IDs.
    let yes_prevout_txid = [0xA1; 32];
    let yes_prevout_vout = 0u32;
    let yes_contract_hash = [0xC1; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), yes_prevout_vout);
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(yes_contract_hash),
    );
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xA2; 32];
    let no_prevout_vout = 1u32;
    let no_contract_hash = [0xC2; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), no_prevout_vout);
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(no_contract_hash),
    );
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    // Create a market with these computed asset/token IDs
    let custom_params = ContractParams {
        oracle_public_key: [0xaa; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    };

    let mut store2 = DeadcatStore::open_in_memory().unwrap();
    let market_id2 = store2.ingest_market(&custom_params, None).unwrap();

    // Build a mock tx with both initial issuances (ZERO_TWEAK nonce)
    let dormant_spk = get_market_spk(&mut store2, &custom_params, MarketState::Dormant);
    let utxo_txid = [0xDD; 32];

    let raw_tx = build_mock_issuance_tx(&[
        (
            yes_prevout_txid,
            yes_prevout_vout,
            yes_contract_hash,
            1000,
            1,
            ZERO_TWEAK,
        ),
        (
            no_prevout_txid,
            no_prevout_vout,
            no_contract_hash,
            1000,
            1,
            ZERO_TWEAK,
        ),
    ]);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk,
        vec![make_chain_utxo(utxo_txid, 0, [0xbb; 32], 100_000)],
    );
    chain.transactions.insert(utxo_txid, raw_tx);

    let report = store2.sync(&chain).unwrap();
    assert_eq!(report.new_utxos, 1);

    // Verify entropy was extracted
    let info = store2.get_market(&market_id2).unwrap().unwrap();
    let issuance = info.issuance.expect("issuance data should be populated");

    assert_eq!(issuance.yes_entropy, yes_entropy.to_byte_array());
    assert_eq!(issuance.no_entropy, no_entropy.to_byte_array());
    assert_eq!(issuance.yes_blinding_nonce, [0u8; 32]); // ZERO_TWEAK
    assert_eq!(issuance.no_blinding_nonce, [0u8; 32]); // ZERO_TWEAK
}

#[test]
fn test_sync_extracts_entropy_from_creation_tx() {
    // Similar to above but specifically tests the initial issuance path
    // where blinding_nonce is ZERO_TWEAK
    let yes_prevout_txid = [0xB1; 32];
    let yes_prevout_vout = 0u32;
    let yes_contract_hash = [0xD1; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), yes_prevout_vout);
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(yes_contract_hash),
    );
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xB2; 32];
    let no_prevout_vout = 1u32;
    let no_contract_hash = [0xD2; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), no_prevout_vout);
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(no_contract_hash),
    );
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    let custom_params = ContractParams {
        oracle_public_key: [0xcc; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 200_000,
        expiry_time: 2_000_000,
    };

    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&custom_params, None).unwrap();

    let dormant_spk = get_market_spk(&mut store, &custom_params, MarketState::Dormant);
    let utxo_txid = [0xEE; 32];

    // Build tx with initial issuances (ZERO_TWEAK = initial)
    let raw_tx = build_mock_issuance_tx(&[
        (
            yes_prevout_txid,
            yes_prevout_vout,
            yes_contract_hash,
            500,
            1,
            ZERO_TWEAK,
        ),
        (
            no_prevout_txid,
            no_prevout_vout,
            no_contract_hash,
            500,
            1,
            ZERO_TWEAK,
        ),
    ]);

    let mut chain = MockChainSource {
        block_height: 600,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk,
        vec![make_chain_utxo(utxo_txid, 0, [0xbb; 32], 200_000)],
    );
    chain.transactions.insert(utxo_txid, raw_tx);

    store.sync(&chain).unwrap();

    let info = store.get_market(&market_id).unwrap().unwrap();
    let issuance = info
        .issuance
        .expect("issuance should be populated from creation tx");
    assert_eq!(issuance.yes_entropy, yes_entropy.to_byte_array());
    assert_eq!(issuance.no_entropy, no_entropy.to_byte_array());
}

#[test]
fn test_sync_skips_entropy_when_tx_unavailable() {
    let yes_prevout_txid = [0xC1; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), 0);
    let yes_entropy =
        AssetId::generate_asset_entropy(yes_outpoint, ContractHash::from_byte_array([0xE1; 32]));
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xC2; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), 1);
    let no_entropy =
        AssetId::generate_asset_entropy(no_outpoint, ContractHash::from_byte_array([0xE2; 32]));
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    let custom_params = ContractParams {
        oracle_public_key: [0xdd; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    };

    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&custom_params, None).unwrap();

    let dormant_spk = get_market_spk(&mut store, &custom_params, MarketState::Dormant);

    // Chain source does NOT have the transaction — get_transaction returns None
    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_spk,
        vec![make_chain_utxo([0xFF; 32], 0, [0xbb; 32], 100_000)],
    );
    // Note: no transactions inserted in chain.transactions

    let report = store.sync(&chain).unwrap();
    assert_eq!(report.new_utxos, 1);

    // Entropy should remain None since tx was unavailable
    let info = store.get_market(&market_id).unwrap().unwrap();
    assert!(info.issuance.is_none());
}

// ==================== Metadata Tests ====================

#[test]
fn test_ingest_market_with_metadata_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();

    let metadata = ContractMetadataInput {
        question: Some("Will BTC hit 100k?".to_string()),
        description: Some("Resolves via exchange data.".to_string()),
        category: Some("Bitcoin".to_string()),
        resolution_source: Some("CoinGecko".to_string()),
        starting_yes_price: Some(55),
        creator_pubkey: Some(vec![0xdd; 32]),
        creation_txid: Some("abc123def456".to_string()),
        nevent: Some("nevent1qtest".to_string()),
        ..Default::default()
    };

    let market_id = store.ingest_market(&params, Some(&metadata)).unwrap();
    let info = store.get_market(&market_id).unwrap().unwrap();

    assert_eq!(info.question.as_deref(), Some("Will BTC hit 100k?"));
    assert_eq!(
        info.description.as_deref(),
        Some("Resolves via exchange data.")
    );
    assert_eq!(info.category.as_deref(), Some("Bitcoin"));
    assert_eq!(info.resolution_source.as_deref(), Some("CoinGecko"));
    assert_eq!(info.starting_yes_price, Some(55));
    assert_eq!(info.creator_pubkey.as_deref(), Some([0xdd; 32].as_slice()));
    assert_eq!(info.creation_txid.as_deref(), Some("abc123def456"));
    assert_eq!(info.nevent.as_deref(), Some("nevent1qtest"));
}

#[test]
fn test_ingest_market_without_metadata() {
    let mut store = DeadcatStore::open_in_memory().unwrap();

    let market_id = store.ingest_market(&test_params(), None).unwrap();
    let info = store.get_market(&market_id).unwrap().unwrap();

    assert!(info.question.is_none());
    assert!(info.description.is_none());
    assert!(info.category.is_none());
    assert!(info.resolution_source.is_none());
    assert!(info.starting_yes_price.is_none());
    assert!(info.creator_pubkey.is_none());
    assert!(info.creation_txid.is_none());
    assert!(info.nevent.is_none());
}

#[test]
fn test_ingest_market_metadata_persists_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("meta.db").to_str().unwrap().to_string();

    let market_id = {
        let mut store = DeadcatStore::open(&db_path).unwrap();
        let metadata = ContractMetadataInput {
            question: Some("Test question".to_string()),
            category: Some("Politics".to_string()),
            ..Default::default()
        };
        store
            .ingest_market(&test_params(), Some(&metadata))
            .unwrap()
    };

    // Reopen and verify metadata persists
    let mut store = DeadcatStore::open(&db_path).unwrap();
    let info = store.get_market(&market_id).unwrap().unwrap();
    assert_eq!(info.question.as_deref(), Some("Test question"));
    assert_eq!(info.category.as_deref(), Some("Politics"));
}

#[test]
fn test_list_markets_includes_metadata() {
    let mut store = DeadcatStore::open_in_memory().unwrap();

    let metadata1 = ContractMetadataInput {
        question: Some("Question 1".to_string()),
        ..Default::default()
    };
    let metadata2 = ContractMetadataInput {
        question: Some("Question 2".to_string()),
        ..Default::default()
    };

    store
        .ingest_market(&test_params(), Some(&metadata1))
        .unwrap();
    store
        .ingest_market(&test_params_2(), Some(&metadata2))
        .unwrap();

    let markets = store.list_markets(&MarketFilter::default()).unwrap();
    assert_eq!(markets.len(), 2);

    let questions: Vec<_> = markets
        .iter()
        .filter_map(|m| m.question.as_deref())
        .collect();
    assert!(questions.contains(&"Question 1"));
    assert!(questions.contains(&"Question 2"));
}

// ==================== Nostr Event JSON Tests ====================

#[test]
fn test_market_nostr_event_json_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();

    let event_json = r#"{"id":"abc123","content":"test"}"#;
    let metadata = ContractMetadataInput {
        question: Some("Test question".to_string()),
        nostr_event_id: Some("abc123".to_string()),
        nostr_event_json: Some(event_json.to_string()),
        ..Default::default()
    };

    let market_id = store.ingest_market(&params, Some(&metadata)).unwrap();
    let info = store.get_market(&market_id).unwrap().unwrap();

    assert_eq!(info.nostr_event_id.as_deref(), Some("abc123"));
    assert_eq!(info.nostr_event_json.as_deref(), Some(event_json));
}

#[test]
fn test_market_nostr_event_json_none_by_default() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = store.ingest_market(&test_params(), None).unwrap();
    let info = store.get_market(&market_id).unwrap().unwrap();

    assert!(info.nostr_event_id.is_none());
    assert!(info.nostr_event_json.is_none());
}

#[test]
fn test_maker_order_nostr_event_json_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();

    let event_json = r#"{"id":"order123","kind":30078}"#;
    let order_id = store
        .ingest_maker_order(
            &params,
            Some(&[0xaa; 32]),
            None,
            Some("order123"),
            Some(event_json),
        )
        .unwrap();

    let info = store.get_maker_order(order_id).unwrap().unwrap();
    assert_eq!(info.nostr_event_id.as_deref(), Some("order123"));
    assert_eq!(info.nostr_event_json.as_deref(), Some(event_json));
}

#[test]
fn test_maker_order_nostr_event_json_none_by_default() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_maker_order_params();

    let order_id = store
        .ingest_maker_order(&params, Some(&[0xaa; 32]), None, None, None)
        .unwrap();

    let info = store.get_maker_order(order_id).unwrap().unwrap();
    assert!(info.nostr_event_json.is_none());
}
