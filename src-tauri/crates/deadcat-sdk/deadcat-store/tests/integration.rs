use std::collections::HashMap;

use deadcat_sdk::elements::confidential::{Asset, Nonce, Value as ConfValue};
use deadcat_sdk::elements::encode::serialize;
use deadcat_sdk::elements::hashes::Hash;
use deadcat_sdk::elements::secp256k1_zkp::{
    Generator, PedersenCommitment, PublicKey, Secp256k1, Tag, Tweak, ZERO_TWEAK,
};
use deadcat_sdk::elements::{
    AssetId, AssetIssuance, ContractHash, LockTime, OutPoint, Script, Sequence, Transaction, TxIn,
    TxInWitness, TxOut, TxOutWitness, Txid,
};
use deadcat_sdk::{
    ContractMetadataInput, MakerOrderParams, MarketId, MarketSlot, MarketState, OrderDirection,
    PredictionMarketAnchor, PredictionMarketParams, UnblindedUtxo, derive_maker_receive,
    maker_receive_script_pubkey,
};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use deadcat_store::{
    ChainSource, ChainUtxo, DeadcatStore, IssuanceData, MarketCandidateFilter, MarketFilter,
    OrderFilter, OrderStatus, PredictionMarketCandidateIngestInput,
};

// ==================== Test Helpers ====================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CreationInputSpec {
    prevout_txid: [u8; 32],
    prevout_vout: u32,
    contract_hash: [u8; 32],
}

fn derive_market_params(
    oracle_public_key: [u8; 32],
    collateral_asset_id: [u8; 32],
    collateral_per_token: u64,
    expiry_time: u32,
    specs: [CreationInputSpec; 2],
) -> PredictionMarketParams {
    let yes_outpoint = OutPoint::new(
        Txid::from_byte_array(specs[0].prevout_txid),
        specs[0].prevout_vout,
    );
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(specs[0].contract_hash),
    );

    let no_outpoint = OutPoint::new(
        Txid::from_byte_array(specs[1].prevout_txid),
        specs[1].prevout_vout,
    );
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(specs[1].contract_hash),
    );

    PredictionMarketParams {
        oracle_public_key,
        collateral_asset_id,
        yes_token_asset: AssetId::from_entropy(yes_entropy)
            .into_inner()
            .to_byte_array(),
        no_token_asset: AssetId::from_entropy(no_entropy)
            .into_inner()
            .to_byte_array(),
        yes_reissuance_token: AssetId::reissuance_token_from_entropy(yes_entropy, false)
            .into_inner()
            .to_byte_array(),
        no_reissuance_token: AssetId::reissuance_token_from_entropy(no_entropy, false)
            .into_inner()
            .to_byte_array(),
        collateral_per_token,
        expiry_time,
    }
}

fn test_creation_specs() -> [CreationInputSpec; 2] {
    [
        CreationInputSpec {
            prevout_txid: [0x31; 32],
            prevout_vout: 0,
            contract_hash: [0u8; 32],
        },
        CreationInputSpec {
            prevout_txid: [0x32; 32],
            prevout_vout: 1,
            contract_hash: [0u8; 32],
        },
    ]
}

fn test_creation_specs_2() -> [CreationInputSpec; 2] {
    [
        CreationInputSpec {
            prevout_txid: [0x51; 32],
            prevout_vout: 0,
            contract_hash: [0u8; 32],
        },
        CreationInputSpec {
            prevout_txid: [0x52; 32],
            prevout_vout: 1,
            contract_hash: [0u8; 32],
        },
    ]
}

fn test_params() -> PredictionMarketParams {
    derive_market_params(
        [0xaa; 32],
        [0xbb; 32],
        100_000,
        1_000_000,
        test_creation_specs(),
    )
}

fn test_params_2() -> PredictionMarketParams {
    derive_market_params(
        [0xcc; 32],
        [0xbb; 32],
        200_000,
        2_000_000,
        test_creation_specs_2(),
    )
}

fn creation_specs_for_params(params: &PredictionMarketParams) -> Option<[CreationInputSpec; 2]> {
    if *params == test_params() {
        Some(test_creation_specs())
    } else if *params == test_params_2() {
        Some(test_creation_specs_2())
    } else {
        None
    }
}

fn get_market_script(params: &PredictionMarketParams, slot: MarketSlot) -> Script {
    use deadcat_sdk::CompiledPredictionMarket;
    let compiled = CompiledPredictionMarket::new(*params).unwrap();
    compiled.script_pubkey(slot)
}

fn build_canonical_creation_tx(
    params: &PredictionMarketParams,
    specs: [CreationInputSpec; 2],
) -> Transaction {
    build_canonical_creation_tx_with_openings(
        params, specs, [0x11; 32], [0x12; 32], [0x21; 32], [0x22; 32],
    )
}

fn build_canonical_creation_tx_with_anchor(
    params: &PredictionMarketParams,
    specs: [CreationInputSpec; 2],
    anchor: &PredictionMarketAnchor,
) -> Transaction {
    let parsed_anchor =
        deadcat_sdk::parse_prediction_market_anchor(anchor).expect("valid test anchor");
    build_canonical_creation_tx_with_openings(
        params,
        specs,
        parsed_anchor.yes_dormant_opening.asset_blinding_factor,
        parsed_anchor.yes_dormant_opening.value_blinding_factor,
        parsed_anchor.no_dormant_opening.asset_blinding_factor,
        parsed_anchor.no_dormant_opening.value_blinding_factor,
    )
}

fn build_canonical_creation_tx_with_openings(
    params: &PredictionMarketParams,
    specs: [CreationInputSpec; 2],
    yes_abf: [u8; 32],
    yes_vbf: [u8; 32],
    no_abf: [u8; 32],
    no_vbf: [u8; 32],
) -> Transaction {
    let inputs = specs
        .into_iter()
        .map(|spec| deadcat_sdk::elements::TxIn {
            previous_output: OutPoint::new(
                Txid::from_byte_array(spec.prevout_txid),
                spec.prevout_vout,
            ),
            is_pegin: false,
            script_sig: Script::new(),
            sequence: Sequence::MAX,
            asset_issuance: AssetIssuance {
                asset_blinding_nonce: ZERO_TWEAK,
                asset_entropy: spec.contract_hash,
                amount: ConfValue::Null,
                inflation_keys: ConfValue::Explicit(1),
            },
            witness: TxInWitness::default(),
        })
        .collect();

    Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![
            confidential_dormant_creation_txout(
                &params.yes_reissuance_token,
                &yes_abf,
                &yes_vbf,
                &get_market_script(params, MarketSlot::DormantYesRt),
            ),
            confidential_dormant_creation_txout(
                &params.no_reissuance_token,
                &no_abf,
                &no_vbf,
                &get_market_script(params, MarketSlot::DormantNoRt),
            ),
        ],
    }
}

fn build_initial_issuance_tx(params: &PredictionMarketParams, creation_txid: Txid) -> Transaction {
    let issuance = AssetIssuance {
        asset_blinding_nonce: ZERO_TWEAK,
        asset_entropy: [0u8; 32],
        amount: ConfValue::Null,
        inflation_keys: ConfValue::Explicit(1),
    };

    Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::new(creation_txid, 0),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: issuance,
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(creation_txid, 1),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: issuance,
                witness: TxInWitness::default(),
            },
        ],
        output: vec![
            explicit_txout(
                &params.yes_reissuance_token,
                1,
                &get_market_script(params, MarketSlot::UnresolvedYesRt),
            ),
            explicit_txout(
                &params.no_reissuance_token,
                1,
                &get_market_script(params, MarketSlot::UnresolvedNoRt),
            ),
            explicit_txout(
                &params.collateral_asset_id,
                params.collateral_per_token * 2,
                &get_market_script(params, MarketSlot::UnresolvedCollateral),
            ),
        ],
    }
}

fn build_terminal_transition_tx(
    params: &PredictionMarketParams,
    issuance_txid: Txid,
    terminal_slot: MarketSlot,
) -> Transaction {
    let burn_spk = burn_script_pubkey();
    Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::new(issuance_txid, 0),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(issuance_txid, 1),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(issuance_txid, 2),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            },
        ],
        output: vec![
            explicit_txout(&params.yes_token_asset, 1, &burn_spk),
            explicit_txout(&params.no_token_asset, 1, &burn_spk),
            explicit_txout(
                &params.collateral_asset_id,
                params.collateral_per_token * 2,
                &get_market_script(params, terminal_slot),
            ),
        ],
    }
}

fn canonical_creation_txid_for_params(params: &PredictionMarketParams) -> Option<Txid> {
    creation_specs_for_params(params).map(|specs| build_canonical_creation_tx(params, specs).txid())
}

fn test_anchor(txid: Txid) -> PredictionMarketAnchor {
    PredictionMarketAnchor::from_openings(txid, [0x11; 32], [0x12; 32], [0x21; 32], [0x22; 32])
}

fn test_market_metadata(params: &PredictionMarketParams) -> ContractMetadataInput {
    let anchor = test_anchor(
        canonical_creation_txid_for_params(params)
            .unwrap_or_else(|| Txid::from_byte_array(params.market_id().as_bytes().to_owned())),
    );
    ContractMetadataInput {
        question: None,
        description: None,
        category: None,
        resolution_source: None,
        creator_pubkey: None,
        anchor,
        nevent: None,
        nostr_event_id: None,
        nostr_event_json: None,
    }
}

fn metadata_with_creation_txid(txid: Txid) -> ContractMetadataInput {
    ContractMetadataInput {
        question: None,
        description: None,
        category: None,
        resolution_source: None,
        creator_pubkey: None,
        anchor: test_anchor(txid),
        nevent: None,
        nostr_event_id: None,
        nostr_event_json: None,
    }
}

const TEST_CANDIDATE_SEEN_AT: u64 = 1_700_000_000;
const TEST_PROMOTED_AT: u64 = TEST_CANDIDATE_SEEN_AT + 120;
const TEST_PROMOTION_BLOCK_HASH: [u8; 32] = [0x88; 32];

fn candidate_input(
    params: &PredictionMarketParams,
    metadata: ContractMetadataInput,
) -> PredictionMarketCandidateIngestInput {
    let specs = creation_specs_for_params(params).expect("test params must map to creation specs");
    let creation_tx = build_canonical_creation_tx_with_anchor(params, specs, &metadata.anchor);
    candidate_input_with_creation_tx(params, metadata, &creation_tx)
}

fn candidate_input_with_creation_tx(
    params: &PredictionMarketParams,
    metadata: ContractMetadataInput,
    creation_tx: &Transaction,
) -> PredictionMarketCandidateIngestInput {
    PredictionMarketCandidateIngestInput {
        params: *params,
        metadata,
        creation_tx: serialize(creation_tx),
    }
}

fn ingest_candidate(
    store: &mut DeadcatStore,
    params: &PredictionMarketParams,
    metadata: ContractMetadataInput,
    seen_at: u64,
) -> i32 {
    let input = candidate_input(params, metadata);
    store
        .ingest_prediction_market_candidate(&input, seen_at)
        .unwrap()
}

fn ingest_candidate_with_matching_creation_tx(
    store: &mut DeadcatStore,
    params: &PredictionMarketParams,
    mut metadata: ContractMetadataInput,
    seen_at: u64,
) -> i32 {
    let specs = creation_specs_for_params(params).expect("test params must map to creation specs");
    let creation_tx = build_canonical_creation_tx_with_anchor(params, specs, &metadata.anchor);
    metadata.anchor.creation_txid = creation_tx.txid().to_string();
    let input = candidate_input_with_creation_tx(params, metadata, &creation_tx);
    store
        .ingest_prediction_market_candidate(&input, seen_at)
        .unwrap()
}

fn promote_candidate(store: &mut DeadcatStore, candidate_id: i32) {
    store
        .promote_prediction_market_candidate(
            candidate_id,
            TEST_PROMOTED_AT,
            2,
            TEST_PROMOTION_BLOCK_HASH,
        )
        .unwrap();
}

fn ingest_test_market(store: &mut DeadcatStore, params: &PredictionMarketParams) -> MarketId {
    let metadata = test_market_metadata(params);
    let candidate_id = ingest_candidate(store, params, metadata, TEST_CANDIDATE_SEEN_AT);
    promote_candidate(store, candidate_id);
    params.market_id()
}

fn ingest_test_market_with_metadata(
    store: &mut DeadcatStore,
    params: &PredictionMarketParams,
    metadata: ContractMetadataInput,
) -> MarketId {
    let candidate_id = ingest_candidate(store, params, metadata, TEST_CANDIDATE_SEEN_AT);
    promote_candidate(store, candidate_id);
    params.market_id()
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

fn confidential_dormant_creation_txout(
    asset_id: &[u8; 32],
    abf: &[u8; 32],
    vbf: &[u8; 32],
    script_pubkey: &Script,
) -> TxOut {
    let secp = Secp256k1::new();
    let generator = Generator::new_blinded(
        &secp,
        Tag::from(*asset_id),
        Tweak::from_slice(abf).expect("valid ABF"),
    );
    let commitment = PedersenCommitment::new(
        &secp,
        1,
        Tweak::from_slice(vbf).expect("valid VBF"),
        generator,
    );
    let nonce = PublicKey::from_slice(&[
        0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87,
        0x0b, 0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16,
        0xf8, 0x17, 0x98,
    ])
    .expect("valid confidential nonce pubkey");

    TxOut {
        asset: Asset::Confidential(generator),
        value: ConfValue::Confidential(commitment),
        nonce: Nonce::Confidential(nonce),
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

fn market_slot_asset_id(params: &PredictionMarketParams, slot: MarketSlot) -> [u8; 32] {
    match slot {
        MarketSlot::DormantYesRt | MarketSlot::UnresolvedYesRt => params.yes_reissuance_token,
        MarketSlot::DormantNoRt | MarketSlot::UnresolvedNoRt => params.no_reissuance_token,
        MarketSlot::UnresolvedCollateral
        | MarketSlot::ResolvedYesCollateral
        | MarketSlot::ResolvedNoCollateral
        | MarketSlot::ExpiredCollateral => params.collateral_asset_id,
    }
}

/// Find the SPK for a given market slot.
fn get_market_spk(params: &PredictionMarketParams, slot: MarketSlot) -> Vec<u8> {
    get_market_script(params, slot).as_bytes().to_vec()
}

fn burn_script_pubkey() -> Script {
    let mut script = vec![0x00, 0x20];
    script.extend_from_slice(&[0u8; 32]);
    Script::from(script)
}

fn add_chain_market_slot_utxo(
    chain: &mut MockChainSource,
    params: &PredictionMarketParams,
    slot: MarketSlot,
    txid: [u8; 32],
    vout: u32,
    value: u64,
) {
    let spk = get_market_spk(params, slot);
    let asset_id = market_slot_asset_id(params, slot);
    chain
        .unspent
        .entry(spk)
        .or_default()
        .push(make_chain_utxo(txid, vout, asset_id, value));
}

fn add_chain_market_state_utxos(
    chain: &mut MockChainSource,
    params: &PredictionMarketParams,
    state: MarketState,
    _txid_seed: u8,
) {
    let specs = creation_specs_for_params(params).expect("canonical test params");
    let creation_tx = build_canonical_creation_tx(params, specs);
    let creation_txid = creation_tx.txid();
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));

    match state {
        MarketState::Dormant => {
            add_chain_market_slot_utxo(
                chain,
                params,
                MarketSlot::DormantYesRt,
                creation_txid.to_byte_array(),
                0,
                1,
            );
            add_chain_market_slot_utxo(
                chain,
                params,
                MarketSlot::DormantNoRt,
                creation_txid.to_byte_array(),
                1,
                1,
            );
        }
        MarketState::Unresolved
        | MarketState::ResolvedYes
        | MarketState::ResolvedNo
        | MarketState::Expired => {
            let issuance_tx = build_initial_issuance_tx(params, creation_txid);
            let issuance_txid = issuance_tx.txid();
            chain
                .transactions
                .insert(issuance_txid.to_byte_array(), serialize(&issuance_tx));
            chain.spent.insert(
                (creation_txid.to_byte_array(), 0),
                issuance_txid.to_byte_array(),
            );
            chain.spent.insert(
                (creation_txid.to_byte_array(), 1),
                issuance_txid.to_byte_array(),
            );

            if state == MarketState::Unresolved {
                add_chain_market_slot_utxo(
                    chain,
                    params,
                    MarketSlot::UnresolvedYesRt,
                    issuance_txid.to_byte_array(),
                    0,
                    1,
                );
                add_chain_market_slot_utxo(
                    chain,
                    params,
                    MarketSlot::UnresolvedNoRt,
                    issuance_txid.to_byte_array(),
                    1,
                    1,
                );
                add_chain_market_slot_utxo(
                    chain,
                    params,
                    MarketSlot::UnresolvedCollateral,
                    issuance_txid.to_byte_array(),
                    2,
                    params.collateral_per_token * 2,
                );
            } else {
                let terminal_slot = match state {
                    MarketState::ResolvedYes => MarketSlot::ResolvedYesCollateral,
                    MarketState::ResolvedNo => MarketSlot::ResolvedNoCollateral,
                    MarketState::Expired => MarketSlot::ExpiredCollateral,
                    _ => unreachable!(),
                };
                let terminal_tx =
                    build_terminal_transition_tx(params, issuance_txid, terminal_slot);
                let terminal_txid = terminal_tx.txid();
                chain
                    .transactions
                    .insert(terminal_txid.to_byte_array(), serialize(&terminal_tx));
                for vout in 0..=2 {
                    chain.spent.insert(
                        (issuance_txid.to_byte_array(), vout),
                        terminal_txid.to_byte_array(),
                    );
                }
                add_chain_market_slot_utxo(
                    chain,
                    params,
                    terminal_slot,
                    terminal_txid.to_byte_array(),
                    2,
                    params.collateral_per_token * 2,
                );
            }
        }
    }
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

const PRE_NODE_OWNED_HISTORY_MIGRATIONS: &[&str] = &[
    "00000000000000",
    "20260220000001",
    "20260220000002",
    "20260221000001",
    "20260222000001",
    "20260227000001",
    "20260228000001",
    "20260228000002",
    "20260302000001",
    "20260313000001",
    "20260319000001",
    "20260320000001",
];

fn bootstrap_pre_node_owned_history_schema(path: &str) {
    let mut conn = SqliteConnection::establish(path).unwrap();
    diesel::sql_query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .unwrap();
    diesel::sql_query(
        "CREATE TABLE __diesel_schema_migrations (
            version VARCHAR(50) PRIMARY KEY NOT NULL,
            run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(&mut conn)
    .unwrap();
    for version in PRE_NODE_OWNED_HISTORY_MIGRATIONS {
        diesel::sql_query("INSERT INTO __diesel_schema_migrations (version) VALUES (?)")
            .bind::<diesel::sql_types::Text, _>(*version)
            .execute(&mut conn)
            .unwrap();
    }
    diesel::sql_query(
        "CREATE TABLE lmsr_pools (
            pool_id TEXT NOT NULL PRIMARY KEY,
            market_id TEXT NOT NULL,
            creation_txid TEXT NOT NULL,
            witness_schema_version TEXT NOT NULL,
            current_s_index BIGINT NOT NULL,
            reserve_yes BIGINT NOT NULL,
            reserve_no BIGINT NOT NULL,
            reserve_collateral BIGINT NOT NULL,
            reserve_yes_outpoint TEXT NOT NULL,
            reserve_no_outpoint TEXT NOT NULL,
            reserve_collateral_outpoint TEXT NOT NULL,
            state_source TEXT NOT NULL DEFAULT 'announcement',
            last_transition_txid TEXT,
            params_json TEXT NOT NULL,
            nostr_event_id TEXT,
            nostr_event_json TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(&mut conn)
    .unwrap();
    diesel::sql_query("CREATE INDEX idx_lmsr_pools_market_id ON lmsr_pools (market_id)")
        .execute(&mut conn)
        .unwrap();
    diesel::sql_query(
        "CREATE TABLE lmsr_price_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pool_id TEXT NOT NULL,
            market_id TEXT NOT NULL,
            transition_txid TEXT NOT NULL,
            old_s_index INTEGER NOT NULL,
            new_s_index INTEGER NOT NULL,
            reserve_yes INTEGER NOT NULL,
            reserve_no INTEGER NOT NULL,
            reserve_collateral INTEGER NOT NULL,
            implied_yes_price_bps INTEGER NOT NULL,
            recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
            block_height INTEGER
        )",
    )
    .execute(&mut conn)
    .unwrap();
    diesel::sql_query(
        "CREATE INDEX idx_price_history_market ON lmsr_price_history(market_id, recorded_at)",
    )
    .execute(&mut conn)
    .unwrap();
    diesel::sql_query(
        "CREATE UNIQUE INDEX idx_price_history_txid ON lmsr_price_history(transition_txid)",
    )
    .execute(&mut conn)
    .unwrap();
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
        ingest_test_market(&mut store, &test_params())
    };

    // Reopen and verify
    let mut store = DeadcatStore::open(&db_path).unwrap();
    let info = store.get_market(&market_id).unwrap();
    assert!(info.is_some());
    assert_eq!(info.unwrap().params, test_params());
}

#[test]
fn test_open_migrates_price_history_dropping_rows_without_block_height() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("legacy-price-history.db");
    bootstrap_pre_node_owned_history_schema(db_path.to_str().unwrap());

    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
    diesel::sql_query(
        "INSERT INTO lmsr_price_history (
            pool_id, market_id, transition_txid, old_s_index, new_s_index,
            reserve_yes, reserve_no, reserve_collateral, implied_yes_price_bps, block_height
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind::<diesel::sql_types::Text, _>("pool-1")
    .bind::<diesel::sql_types::Text, _>("market-1")
    .bind::<diesel::sql_types::Text, _>("tx-pending")
    .bind::<diesel::sql_types::BigInt, _>(1_i64)
    .bind::<diesel::sql_types::BigInt, _>(2_i64)
    .bind::<diesel::sql_types::BigInt, _>(100_i64)
    .bind::<diesel::sql_types::BigInt, _>(101_i64)
    .bind::<diesel::sql_types::BigInt, _>(102_i64)
    .bind::<diesel::sql_types::Integer, _>(4_900_i32)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Integer>, _>(None::<i32>)
    .execute(&mut conn)
    .unwrap();
    diesel::sql_query(
        "INSERT INTO lmsr_price_history (
            pool_id, market_id, transition_txid, old_s_index, new_s_index,
            reserve_yes, reserve_no, reserve_collateral, implied_yes_price_bps, block_height
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind::<diesel::sql_types::Text, _>("pool-1")
    .bind::<diesel::sql_types::Text, _>("market-1")
    .bind::<diesel::sql_types::Text, _>("tx-confirmed")
    .bind::<diesel::sql_types::BigInt, _>(2_i64)
    .bind::<diesel::sql_types::BigInt, _>(3_i64)
    .bind::<diesel::sql_types::BigInt, _>(110_i64)
    .bind::<diesel::sql_types::BigInt, _>(111_i64)
    .bind::<diesel::sql_types::BigInt, _>(112_i64)
    .bind::<diesel::sql_types::Integer, _>(5_100_i32)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Integer>, _>(Some(321_i32))
    .execute(&mut conn)
    .unwrap();
    drop(conn);

    let mut store = DeadcatStore::open(db_path.to_str().unwrap()).unwrap();
    let history = store
        .get_market_price_history("market-1", None, None)
        .unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].transition_txid, "tx-confirmed");
    assert_eq!(history[0].block_height, 321);
}

#[test]
fn test_open_migrates_lmsr_pools_without_fabricated_initial_anchors() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("legacy-lmsr-pools.db");
    bootstrap_pre_node_owned_history_schema(db_path.to_str().unwrap());

    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
    diesel::sql_query(
        "INSERT INTO lmsr_pools (
            pool_id, market_id, creation_txid, witness_schema_version, current_s_index,
            reserve_yes, reserve_no, reserve_collateral,
            reserve_yes_outpoint, reserve_no_outpoint, reserve_collateral_outpoint,
            state_source, last_transition_txid, params_json, nostr_event_id, nostr_event_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind::<diesel::sql_types::Text, _>("pool-legacy")
    .bind::<diesel::sql_types::Text, _>("market-legacy")
    .bind::<diesel::sql_types::Text, _>("aa".repeat(32))
    .bind::<diesel::sql_types::Text, _>("v2")
    .bind::<diesel::sql_types::BigInt, _>(7_i64)
    .bind::<diesel::sql_types::BigInt, _>(1_000_i64)
    .bind::<diesel::sql_types::BigInt, _>(1_001_i64)
    .bind::<diesel::sql_types::BigInt, _>(1_002_i64)
    .bind::<diesel::sql_types::Text, _>(&format!("{}:0", "bb".repeat(32)))
    .bind::<diesel::sql_types::Text, _>(&format!("{}:1", "cc".repeat(32)))
    .bind::<diesel::sql_types::Text, _>(&format!("{}:2", "dd".repeat(32)))
    .bind::<diesel::sql_types::Text, _>("announcement")
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(None::<&str>)
    .bind::<diesel::sql_types::Text, _>("{}")
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(None::<&str>)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(None::<&str>)
    .execute(&mut conn)
    .unwrap();
    drop(conn);

    let mut store = DeadcatStore::open(db_path.to_str().unwrap()).unwrap();
    let pools = store.list_lmsr_pool_sync_info().unwrap();
    let pool = pools
        .into_iter()
        .find(|pool| pool.pool_id == "pool-legacy")
        .unwrap();

    assert_eq!(pool.stored_initial_reserve_outpoints, None);
}

// ==================== Market Tests ====================

#[test]
fn test_market_ingest_and_query_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();

    let market_id = ingest_test_market(&mut store, &params);
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

    let id1 = ingest_test_market(&mut store, &params);
    let id2 = ingest_test_market(&mut store, &params);
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
    ingest_test_market(&mut store, &test_params());
    ingest_test_market(&mut store, &test_params_2());

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
    let id1 = ingest_test_market(&mut store, &test_params());
    ingest_test_market(&mut store, &test_params_2());

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
    ingest_test_market(&mut store, &test_params()); // expiry = 1_000_000
    ingest_test_market(&mut store, &test_params_2()); // expiry = 2_000_000

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
    ingest_test_market(&mut store, &test_params());
    ingest_test_market(&mut store, &test_params_2());

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
    ingest_test_market(&mut store, &test_params());
    ingest_test_market(&mut store, &test_params_2());

    let filter = MarketFilter {
        limit: Some(1),
        ..Default::default()
    };
    assert_eq!(store.list_markets(&filter).unwrap().len(), 1);
}

#[test]
fn test_update_market_state() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let id = ingest_test_market(&mut store, &test_params());

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

    let order_id = store
        .ingest_maker_order(&params, None, None, None, None)
        .unwrap();
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
        .ingest_maker_order(
            &test_maker_order_params(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
        .unwrap();
    store
        .ingest_maker_order(
            &test_maker_order_params_2(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
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
    store
        .ingest_maker_order(&params, None, None, None, None)
        .unwrap();

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
        .ingest_maker_order(
            &test_maker_order_params(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
        .unwrap();
    store
        .ingest_maker_order(
            &test_maker_order_params_2(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
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
        .ingest_maker_order(
            &test_maker_order_params(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
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
    let market_id = ingest_test_market(&mut store, &test_params());

    let utxo = test_utxo_with_outpoint([0xAA; 32], 0, [0xbb; 32], 100_000);

    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantYesRt, &utxo, Some(100))
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
    let market_id = ingest_test_market(&mut store, &test_params());

    let utxo = test_utxo_with_outpoint([0xAA; 32], 0, [0xbb; 32], 100_000);

    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantYesRt, &utxo, Some(100))
        .unwrap();
    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantYesRt, &utxo, Some(100))
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
        .ingest_maker_order(
            &test_maker_order_params(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
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
    let market_id = ingest_test_market(&mut store, &test_params());

    let utxo1 = test_utxo_with_outpoint([0xAA; 32], 0, [0xbb; 32], 100_000);
    let utxo2 = test_utxo_with_outpoint([0xBB; 32], 0, [0xbb; 32], 200_000);

    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantYesRt, &utxo1, None)
        .unwrap();
    store
        .add_market_slot_utxo(&market_id, MarketSlot::UnresolvedCollateral, &utxo2, None)
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

#[test]
fn test_get_market_slot_utxos_filters_exact_slot() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = ingest_test_market(&mut store, &test_params());

    let yes_utxo = test_utxo_with_outpoint([0xAA; 32], 0, [0x03; 32], 1);
    let no_utxo = test_utxo_with_outpoint([0xBB; 32], 1, [0x04; 32], 1);

    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantYesRt, &yes_utxo, None)
        .unwrap();
    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantNoRt, &no_utxo, None)
        .unwrap();

    let yes = store
        .get_market_slot_utxos(&market_id, MarketSlot::DormantYesRt)
        .unwrap();
    let no = store
        .get_market_slot_utxos(&market_id, MarketSlot::DormantNoRt)
        .unwrap();

    assert_eq!(yes.len(), 1);
    assert_eq!(yes[0].outpoint.txid, Txid::from_byte_array([0xAA; 32]));
    assert_eq!(no.len(), 1);
    assert_eq!(no[0].outpoint.txid, Txid::from_byte_array([0xBB; 32]));
}

// ==================== Watched SPKs ====================

#[test]
fn test_watched_script_pubkeys() {
    let mut store = DeadcatStore::open_in_memory().unwrap();

    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 0);

    ingest_test_market(&mut store, &test_params());
    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 8);

    store
        .ingest_maker_order(
            &test_maker_order_params(),
            Some(&[0xaa; 32]),
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 9);

    // Order without pubkey -> no covenant_spk -> no additional watched SPK
    store
        .ingest_maker_order(&test_maker_order_params_2(), None, None, None, None)
        .unwrap();
    assert_eq!(store.watched_script_pubkeys().unwrap().len(), 9);
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
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Dormant, 0xDD);

    let report = store.sync(&chain).unwrap();
    assert_eq!(
        report.new_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );
    assert_eq!(report.block_height, 500);

    let utxos = store
        .get_market_utxos(&market_id, Some(MarketState::Dormant))
        .unwrap();
    assert_eq!(utxos.len(), MarketState::Dormant.live_slots().len());

    assert_eq!(store.last_synced_height().unwrap(), 500);
}

#[test]
fn test_sync_marks_spent() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 550,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Dormant, 0xDD);
    store.sync(&chain).unwrap();

    let mut chain = MockChainSource {
        block_height: 600,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Unresolved, 0xEE);

    let report = store.sync(&chain).unwrap();
    assert_eq!(
        report.spent_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );

    assert_eq!(
        store
            .get_market_utxos(&market_id, Some(MarketState::Dormant))
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::Unresolved
    );
}

#[test]
fn test_sync_derives_market_state() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Unresolved, 0xDD);

    let report = store.sync(&chain).unwrap();

    assert_eq!(
        report.new_utxos,
        MarketState::Unresolved.live_slots().len() as u32
    );
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
fn test_sync_derives_resolved_yes_from_terminal_slot() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 710,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::ResolvedYes, 0xE1);

    let report = store.sync(&chain).unwrap();

    assert_eq!(report.new_utxos, 1);
    assert_eq!(report.market_state_changes.len(), 1);
    assert_eq!(
        report.market_state_changes[0].new_state,
        MarketState::ResolvedYes
    );
    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::ResolvedYes
    );
}

#[test]
fn test_sync_derives_resolved_no_from_terminal_slot() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 720,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::ResolvedNo, 0xE2);

    let report = store.sync(&chain).unwrap();

    assert_eq!(report.new_utxos, 1);
    assert_eq!(report.market_state_changes.len(), 1);
    assert_eq!(
        report.market_state_changes[0].new_state,
        MarketState::ResolvedNo
    );
    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::ResolvedNo
    );
}

#[test]
fn test_sync_derives_expired_from_terminal_slot() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 730,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Expired, 0xE3);

    let report = store.sync(&chain).unwrap();

    assert_eq!(report.new_utxos, 1);
    assert_eq!(report.market_state_changes.len(), 1);
    assert_eq!(
        report.market_state_changes[0].new_state,
        MarketState::Expired
    );
    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::Expired
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
    let market_id = ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Dormant, 0xDD);

    let report1 = store.sync(&chain).unwrap();
    assert_eq!(
        report1.new_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );

    let report2 = store.sync(&chain).unwrap();
    assert_eq!(report2.new_utxos, 0);

    assert_eq!(
        store
            .get_market_utxos(&market_id, Some(MarketState::Dormant))
            .unwrap()
            .len(),
        MarketState::Dormant.live_slots().len()
    );
}

#[test]
fn test_sync_multi_round_discover_then_spend() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let market_id = ingest_test_market(&mut store, &params);

    // Round 1: discover UTXO
    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain, &params, MarketState::Dormant, 0xDD);
    let r1 = store.sync(&chain).unwrap();
    assert_eq!(r1.new_utxos, MarketState::Dormant.live_slots().len() as u32);
    assert_eq!(
        store.get_market_utxos(&market_id, None).unwrap().len(),
        MarketState::Dormant.live_slots().len()
    );

    // Round 2: UTXOs are now spent, no longer in unspent set
    let mut chain2 = MockChainSource {
        block_height: 600,
        ..Default::default()
    };
    add_chain_market_state_utxos(&mut chain2, &params, MarketState::Unresolved, 0xEE);
    let r2 = store.sync(&chain2).unwrap();
    assert_eq!(
        r2.spent_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );
    assert_eq!(
        store.get_market_utxos(&market_id, None).unwrap().len(),
        MarketState::Unresolved.live_slots().len()
    );
    assert_eq!(
        store.get_market(&market_id).unwrap().unwrap().state,
        MarketState::Unresolved
    );
}

#[test]
fn test_sync_rejects_mixed_live_states() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    ingest_test_market(&mut store, &params);

    let specs = test_creation_specs();
    let creation_tx = build_canonical_creation_tx(&params, specs);
    let creation_txid = creation_tx.txid();
    let malformed_tx = Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::new(creation_txid, 0),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance {
                    asset_blinding_nonce: ZERO_TWEAK,
                    asset_entropy: [0u8; 32],
                    amount: ConfValue::Null,
                    inflation_keys: ConfValue::Explicit(1),
                },
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(creation_txid, 1),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance {
                    asset_blinding_nonce: ZERO_TWEAK,
                    asset_entropy: [0u8; 32],
                    amount: ConfValue::Null,
                    inflation_keys: ConfValue::Explicit(1),
                },
                witness: TxInWitness::default(),
            },
        ],
        output: vec![
            explicit_txout(
                &params.yes_reissuance_token,
                1,
                &get_market_script(&params, MarketSlot::DormantYesRt),
            ),
            explicit_txout(
                &params.yes_reissuance_token,
                1,
                &get_market_script(&params, MarketSlot::UnresolvedYesRt),
            ),
            explicit_txout(
                &params.no_reissuance_token,
                1,
                &get_market_script(&params, MarketSlot::UnresolvedNoRt),
            ),
            explicit_txout(
                &params.collateral_asset_id,
                params.collateral_per_token * 2,
                &get_market_script(&params, MarketSlot::UnresolvedCollateral),
            ),
        ],
    };

    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));
    chain.transactions.insert(
        malformed_tx.txid().to_byte_array(),
        serialize(&malformed_tx),
    );
    chain.spent.insert(
        (creation_txid.to_byte_array(), 0),
        malformed_tx.txid().to_byte_array(),
    );
    chain.spent.insert(
        (creation_txid.to_byte_array(), 1),
        malformed_tx.txid().to_byte_array(),
    );

    let err = store.sync(&chain).unwrap_err().to_string();
    assert!(err.contains("expected"), "got: {err}");
}

#[test]
fn test_sync_rejects_partial_dormant_slot_set() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 740,
        ..Default::default()
    };
    let specs = test_creation_specs();
    let creation_tx = build_canonical_creation_tx(&params, specs);
    let creation_txid = creation_tx.txid();
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));
    chain
        .spent
        .insert((creation_txid.to_byte_array(), 0), [0xC1; 32]);

    let err = store.sync(&chain).unwrap_err().to_string();
    assert!(err.contains("split"), "got: {err}");
}

#[test]
fn test_sync_rejects_collateral_only_unresolved_set() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 750,
        ..Default::default()
    };
    let specs = test_creation_specs();
    let creation_tx = build_canonical_creation_tx(&params, specs);
    let creation_txid = creation_tx.txid();
    let malformed_tx = Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::new(creation_txid, 0),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance {
                    asset_blinding_nonce: ZERO_TWEAK,
                    asset_entropy: [0u8; 32],
                    amount: ConfValue::Null,
                    inflation_keys: ConfValue::Explicit(1),
                },
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(creation_txid, 1),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance {
                    asset_blinding_nonce: ZERO_TWEAK,
                    asset_entropy: [0u8; 32],
                    amount: ConfValue::Null,
                    inflation_keys: ConfValue::Explicit(1),
                },
                witness: TxInWitness::default(),
            },
        ],
        output: vec![explicit_txout(
            &params.collateral_asset_id,
            params.collateral_per_token * 2,
            &get_market_script(&params, MarketSlot::UnresolvedCollateral),
        )],
    };
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));
    chain.transactions.insert(
        malformed_tx.txid().to_byte_array(),
        serialize(&malformed_tx),
    );
    chain.spent.insert(
        (creation_txid.to_byte_array(), 0),
        malformed_tx.txid().to_byte_array(),
    );
    chain.spent.insert(
        (creation_txid.to_byte_array(), 1),
        malformed_tx.txid().to_byte_array(),
    );

    let err = store.sync(&chain).unwrap_err().to_string();
    assert!(err.contains("expected"), "got: {err}");
}

#[test]
fn test_sync_rejects_both_resolved_terminal_slots() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    ingest_test_market(&mut store, &params);

    let mut chain = MockChainSource {
        block_height: 760,
        ..Default::default()
    };
    let specs = test_creation_specs();
    let creation_tx = build_canonical_creation_tx(&params, specs);
    let creation_txid = creation_tx.txid();
    let issuance_tx = build_initial_issuance_tx(&params, creation_txid);
    let issuance_txid = issuance_tx.txid();
    let burn_spk = burn_script_pubkey();
    let malformed_tx = Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::new(issuance_txid, 0),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(issuance_txid, 1),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            },
            TxIn {
                previous_output: OutPoint::new(issuance_txid, 2),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            },
        ],
        output: vec![
            explicit_txout(&params.yes_token_asset, 1, &burn_spk),
            explicit_txout(&params.no_token_asset, 1, &burn_spk),
            explicit_txout(
                &params.collateral_asset_id,
                params.collateral_per_token * 2,
                &get_market_script(&params, MarketSlot::ResolvedYesCollateral),
            ),
            explicit_txout(
                &params.collateral_asset_id,
                params.collateral_per_token * 2,
                &get_market_script(&params, MarketSlot::ResolvedNoCollateral),
            ),
        ],
    };
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));
    chain
        .transactions
        .insert(issuance_txid.to_byte_array(), serialize(&issuance_tx));
    chain.transactions.insert(
        malformed_tx.txid().to_byte_array(),
        serialize(&malformed_tx),
    );
    chain.spent.insert(
        (creation_txid.to_byte_array(), 0),
        issuance_txid.to_byte_array(),
    );
    chain.spent.insert(
        (creation_txid.to_byte_array(), 1),
        issuance_txid.to_byte_array(),
    );
    for vout in 0..=2 {
        chain.spent.insert(
            (issuance_txid.to_byte_array(), vout),
            malformed_tx.txid().to_byte_array(),
        );
    }

    let err = store.sync(&chain).unwrap_err().to_string();
    assert!(err.contains("unrecognized transition"), "got: {err}");
}

#[test]
fn test_sync_chain_error_propagates() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    ingest_test_market(&mut store, &test_params());

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
    let market_id = ingest_test_market(&mut store, &params);

    // Add a UTXO so sync_spent_utxos has work to do
    let utxo = test_utxo_with_outpoint([0xDD; 32], 0, [0xbb; 32], 100_000);
    store
        .add_market_slot_utxo(&market_id, MarketSlot::DormantYesRt, &utxo, Some(100))
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
    let market_id = ingest_test_market(&mut store, &test_params());

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

#[test]
fn test_sync_extracts_issuance_entropy() {
    // Build a market with known issuance entropies, then verify sync extracts them.
    // We compute token IDs from known outpoints + contract hashes, then build
    // PredictionMarketParams using those IDs.
    let yes_prevout_txid = [0xA1; 32];
    let yes_prevout_vout = 0u32;
    let yes_contract_hash = [0u8; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), yes_prevout_vout);
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(yes_contract_hash),
    );
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xA2; 32];
    let no_prevout_vout = 1u32;
    let no_contract_hash = [0u8; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), no_prevout_vout);
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(no_contract_hash),
    );
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    // Create a market with these computed asset/token IDs
    let custom_params = PredictionMarketParams {
        oracle_public_key: [0xaa; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    };

    let specs = [
        CreationInputSpec {
            prevout_txid: yes_prevout_txid,
            prevout_vout: yes_prevout_vout,
            contract_hash: yes_contract_hash,
        },
        CreationInputSpec {
            prevout_txid: no_prevout_txid,
            prevout_vout: no_prevout_vout,
            contract_hash: no_contract_hash,
        },
    ];
    let creation_tx = build_canonical_creation_tx(&custom_params, specs);
    let creation_txid = creation_tx.txid();

    let mut store2 = DeadcatStore::open_in_memory().unwrap();
    let market_id2 = custom_params.market_id();
    let candidate_id = store2
        .ingest_prediction_market_candidate(
            &candidate_input_with_creation_tx(
                &custom_params,
                metadata_with_creation_txid(creation_txid),
                &creation_tx,
            ),
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap();
    promote_candidate(&mut store2, candidate_id);

    let dormant_yes_spk = get_market_spk(&custom_params, MarketSlot::DormantYesRt);
    let dormant_no_spk = get_market_spk(&custom_params, MarketSlot::DormantNoRt);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_yes_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            0,
            custom_params.yes_reissuance_token,
            1,
        )],
    );
    chain.unspent.insert(
        dormant_no_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            1,
            custom_params.no_reissuance_token,
            1,
        )],
    );
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));

    let report = store2.sync(&chain).unwrap();
    assert_eq!(
        report.new_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );

    // Verify entropy was extracted
    let info = store2.get_market(&market_id2).unwrap().unwrap();
    let issuance = info.issuance.expect("issuance data should be populated");

    assert_eq!(issuance.yes_entropy, yes_entropy.to_byte_array());
    assert_eq!(issuance.no_entropy, no_entropy.to_byte_array());
    assert_eq!(issuance.yes_blinding_nonce, [0u8; 32]); // ZERO_TWEAK
    assert_eq!(issuance.no_blinding_nonce, [0u8; 32]); // ZERO_TWEAK
}

#[test]
fn test_sync_retries_entropy_extraction_until_both_sides_are_present() {
    let yes_prevout_txid = [0xA1; 32];
    let yes_prevout_vout = 0u32;
    let yes_contract_hash = [0u8; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), yes_prevout_vout);
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(yes_contract_hash),
    );
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xA2; 32];
    let no_prevout_vout = 1u32;
    let no_contract_hash = [0u8; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), no_prevout_vout);
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(no_contract_hash),
    );
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    let custom_params = PredictionMarketParams {
        oracle_public_key: [0xaa; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    };

    let specs = [
        CreationInputSpec {
            prevout_txid: yes_prevout_txid,
            prevout_vout: yes_prevout_vout,
            contract_hash: yes_contract_hash,
        },
        CreationInputSpec {
            prevout_txid: no_prevout_txid,
            prevout_vout: no_prevout_vout,
            contract_hash: no_contract_hash,
        },
    ];
    let creation_tx = build_canonical_creation_tx(&custom_params, specs);
    let creation_txid = creation_tx.txid();

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("partial-issuance.db");
    let mut store = DeadcatStore::open(db_path.to_str().unwrap()).unwrap();
    let market_id = custom_params.market_id();
    let candidate_id = store
        .ingest_prediction_market_candidate(
            &candidate_input_with_creation_tx(
                &custom_params,
                metadata_with_creation_txid(creation_txid),
                &creation_tx,
            ),
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap();
    promote_candidate(&mut store, candidate_id);

    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
    diesel::sql_query(
        "UPDATE market_candidates
         SET yes_issuance_entropy = ?,
             yes_issuance_blinding_nonce = ?,
             no_issuance_entropy = NULL,
             no_issuance_blinding_nonce = NULL
         WHERE market_id = ?",
    )
    .bind::<diesel::sql_types::Binary, _>(yes_entropy.to_byte_array().to_vec())
    .bind::<diesel::sql_types::Binary, _>([0u8; 32].to_vec())
    .bind::<diesel::sql_types::Binary, _>(market_id.as_bytes().to_vec())
    .execute(&mut conn)
    .unwrap();

    let dormant_yes_spk = get_market_spk(&custom_params, MarketSlot::DormantYesRt);
    let dormant_no_spk = get_market_spk(&custom_params, MarketSlot::DormantNoRt);

    let mut chain = MockChainSource {
        block_height: 500,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_yes_spk.clone(),
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            0,
            custom_params.yes_reissuance_token,
            1,
        )],
    );
    chain.unspent.insert(
        dormant_no_spk.clone(),
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            1,
            custom_params.no_reissuance_token,
            1,
        )],
    );
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));

    let first_report = store.sync(&chain).unwrap();
    assert_eq!(
        first_report.new_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );
    let info = store.get_market(&market_id).unwrap().unwrap();
    let issuance = info.issuance.expect("issuance data should be populated");
    assert_eq!(issuance.yes_entropy, yes_entropy.to_byte_array());
    assert_eq!(issuance.no_entropy, no_entropy.to_byte_array());
    assert_eq!(issuance.yes_blinding_nonce, [0u8; 32]);
    assert_eq!(issuance.no_blinding_nonce, [0u8; 32]);
}

#[test]
fn test_sync_retries_entropy_when_tx_becomes_available_without_new_utxos() {
    let yes_prevout_txid = [0x91; 32];
    let yes_prevout_vout = 0u32;
    let yes_contract_hash = [0u8; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), yes_prevout_vout);
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(yes_contract_hash),
    );
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0x92; 32];
    let no_prevout_vout = 1u32;
    let no_contract_hash = [0u8; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), no_prevout_vout);
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(no_contract_hash),
    );
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    let custom_params = PredictionMarketParams {
        oracle_public_key: [0xaa; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    };

    let specs = [
        CreationInputSpec {
            prevout_txid: yes_prevout_txid,
            prevout_vout: yes_prevout_vout,
            contract_hash: yes_contract_hash,
        },
        CreationInputSpec {
            prevout_txid: no_prevout_txid,
            prevout_vout: no_prevout_vout,
            contract_hash: no_contract_hash,
        },
    ];
    let creation_tx = build_canonical_creation_tx(&custom_params, specs);
    let creation_txid = creation_tx.txid();

    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = custom_params.market_id();
    let candidate_id = store
        .ingest_prediction_market_candidate(
            &candidate_input_with_creation_tx(
                &custom_params,
                metadata_with_creation_txid(creation_txid),
                &creation_tx,
            ),
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap();
    promote_candidate(&mut store, candidate_id);

    let dormant_yes_spk = get_market_spk(&custom_params, MarketSlot::DormantYesRt);
    let dormant_no_spk = get_market_spk(&custom_params, MarketSlot::DormantNoRt);

    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_yes_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            0,
            custom_params.yes_reissuance_token,
            1,
        )],
    );
    chain.unspent.insert(
        dormant_no_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            1,
            custom_params.no_reissuance_token,
            1,
        )],
    );

    let err = store.sync(&chain).unwrap_err().to_string();
    assert!(err.contains("not found"), "got: {err}");

    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));

    let second_report = store.sync(&chain).unwrap();
    assert_eq!(
        second_report.new_utxos,
        MarketState::Dormant.live_slots().len() as u32
    );

    let info = store.get_market(&market_id).unwrap().unwrap();
    let issuance = info.issuance.expect("issuance data should be populated");
    assert_eq!(issuance.yes_entropy, yes_entropy.to_byte_array());
    assert_eq!(issuance.no_entropy, no_entropy.to_byte_array());
    assert_eq!(issuance.yes_blinding_nonce, [0u8; 32]);
    assert_eq!(issuance.no_blinding_nonce, [0u8; 32]);
}

#[test]
fn test_sync_extracts_entropy_from_creation_tx() {
    // Similar to above but specifically tests the initial issuance path
    // where blinding_nonce is ZERO_TWEAK
    let yes_prevout_txid = [0xB1; 32];
    let yes_prevout_vout = 0u32;
    let yes_contract_hash = [0u8; 32];
    let yes_outpoint = OutPoint::new(Txid::from_byte_array(yes_prevout_txid), yes_prevout_vout);
    let yes_entropy = AssetId::generate_asset_entropy(
        yes_outpoint,
        ContractHash::from_byte_array(yes_contract_hash),
    );
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xB2; 32];
    let no_prevout_vout = 1u32;
    let no_contract_hash = [0u8; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), no_prevout_vout);
    let no_entropy = AssetId::generate_asset_entropy(
        no_outpoint,
        ContractHash::from_byte_array(no_contract_hash),
    );
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    let custom_params = PredictionMarketParams {
        oracle_public_key: [0xcc; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 200_000,
        expiry_time: 2_000_000,
    };

    let specs = [
        CreationInputSpec {
            prevout_txid: yes_prevout_txid,
            prevout_vout: yes_prevout_vout,
            contract_hash: yes_contract_hash,
        },
        CreationInputSpec {
            prevout_txid: no_prevout_txid,
            prevout_vout: no_prevout_vout,
            contract_hash: no_contract_hash,
        },
    ];
    let creation_tx = build_canonical_creation_tx(&custom_params, specs);
    let creation_txid = creation_tx.txid();

    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = custom_params.market_id();
    let candidate_id = store
        .ingest_prediction_market_candidate(
            &candidate_input_with_creation_tx(
                &custom_params,
                metadata_with_creation_txid(creation_txid),
                &creation_tx,
            ),
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap();
    promote_candidate(&mut store, candidate_id);

    let dormant_yes_spk = get_market_spk(&custom_params, MarketSlot::DormantYesRt);
    let dormant_no_spk = get_market_spk(&custom_params, MarketSlot::DormantNoRt);

    let mut chain = MockChainSource {
        block_height: 600,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_yes_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            0,
            custom_params.yes_reissuance_token,
            1,
        )],
    );
    chain.unspent.insert(
        dormant_no_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            1,
            custom_params.no_reissuance_token,
            1,
        )],
    );
    chain
        .transactions
        .insert(creation_txid.to_byte_array(), serialize(&creation_tx));

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
        AssetId::generate_asset_entropy(yes_outpoint, ContractHash::from_byte_array([0u8; 32]));
    let yes_asset = AssetId::from_entropy(yes_entropy);
    let yes_token = AssetId::reissuance_token_from_entropy(yes_entropy, false);

    let no_prevout_txid = [0xC2; 32];
    let no_outpoint = OutPoint::new(Txid::from_byte_array(no_prevout_txid), 1);
    let no_entropy =
        AssetId::generate_asset_entropy(no_outpoint, ContractHash::from_byte_array([0u8; 32]));
    let no_asset = AssetId::from_entropy(no_entropy);
    let no_token = AssetId::reissuance_token_from_entropy(no_entropy, false);

    let custom_params = PredictionMarketParams {
        oracle_public_key: [0xdd; 32],
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: yes_asset.into_inner().to_byte_array(),
        no_token_asset: no_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_token.into_inner().to_byte_array(),
        no_reissuance_token: no_token.into_inner().to_byte_array(),
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    };

    let specs = [
        CreationInputSpec {
            prevout_txid: yes_prevout_txid,
            prevout_vout: 0,
            contract_hash: [0u8; 32],
        },
        CreationInputSpec {
            prevout_txid: no_prevout_txid,
            prevout_vout: 1,
            contract_hash: [0u8; 32],
        },
    ];
    let creation_tx = build_canonical_creation_tx(&custom_params, specs);
    let creation_txid = creation_tx.txid();

    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = custom_params.market_id();
    let candidate_id = store
        .ingest_prediction_market_candidate(
            &candidate_input_with_creation_tx(
                &custom_params,
                metadata_with_creation_txid(creation_txid),
                &creation_tx,
            ),
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap();
    promote_candidate(&mut store, candidate_id);

    let dormant_yes_spk = get_market_spk(&custom_params, MarketSlot::DormantYesRt);
    let dormant_no_spk = get_market_spk(&custom_params, MarketSlot::DormantNoRt);

    // Chain source does NOT have the transaction — get_transaction returns None
    let mut chain = MockChainSource {
        block_height: 700,
        ..Default::default()
    };
    chain.unspent.insert(
        dormant_yes_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            0,
            custom_params.yes_reissuance_token,
            1,
        )],
    );
    chain.unspent.insert(
        dormant_no_spk,
        vec![make_chain_utxo(
            creation_txid.to_byte_array(),
            1,
            custom_params.no_reissuance_token,
            1,
        )],
    );

    let err = store.sync(&chain).unwrap_err().to_string();
    assert!(err.contains("not found"), "got: {err}");

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
        creator_pubkey: Some(vec![0xdd; 32]),
        nevent: Some("nevent1qtest".to_string()),
        ..test_market_metadata(&params)
    };

    let market_id = ingest_test_market_with_metadata(&mut store, &params, metadata);
    let info = store.get_market(&market_id).unwrap().unwrap();

    assert_eq!(info.question.as_deref(), Some("Will BTC hit 100k?"));
    assert_eq!(
        info.description.as_deref(),
        Some("Resolves via exchange data.")
    );
    assert_eq!(info.category.as_deref(), Some("Bitcoin"));
    assert_eq!(info.resolution_source.as_deref(), Some("CoinGecko"));
    assert_eq!(info.creator_pubkey.as_deref(), Some([0xdd; 32].as_slice()));
    assert_eq!(
        info.anchor.creation_txid,
        test_market_metadata(&params).anchor.creation_txid
    );
    assert_eq!(info.nevent.as_deref(), Some("nevent1qtest"));
}

#[test]
fn test_ingest_candidate_rejects_invalid_creation_tx_bytes() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let err = store
        .ingest_prediction_market_candidate(
            &PredictionMarketCandidateIngestInput {
                params: test_params(),
                metadata: test_market_metadata(&test_params()),
                creation_tx: Vec::new(),
            },
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap_err();
    assert!(format!("{err}").contains("invalid creation_tx bytes"));
}

#[test]
fn test_ingest_market_rejects_invalid_creation_txid() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let metadata = ContractMetadataInput {
        question: None,
        description: None,
        category: None,
        resolution_source: None,
        creator_pubkey: None,
        anchor: PredictionMarketAnchor {
            creation_txid: "not-a-txid".to_string(),
            ..test_market_metadata(&test_params()).anchor
        },
        nevent: None,
        nostr_event_id: None,
        nostr_event_json: None,
    };
    let err = store
        .ingest_prediction_market_candidate(
            &PredictionMarketCandidateIngestInput {
                params: test_params(),
                metadata,
                creation_tx: serialize(&build_canonical_creation_tx(
                    &test_params(),
                    test_creation_specs(),
                )),
            },
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap_err();
    assert!(format!("{err}").contains("invalid creation_txid"));
}

#[test]
fn test_ingest_market_rejects_invalid_yes_opening() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let mut anchor = test_market_metadata(&test_params()).anchor;
    anchor.yes_dormant_opening.asset_blinding_factor = "not-hex".to_string();
    let metadata = ContractMetadataInput {
        question: None,
        description: None,
        category: None,
        resolution_source: None,
        creator_pubkey: None,
        anchor,
        nevent: None,
        nostr_event_id: None,
        nostr_event_json: None,
    };
    let err = store
        .ingest_prediction_market_candidate(
            &PredictionMarketCandidateIngestInput {
                params: test_params(),
                metadata,
                creation_tx: serialize(&build_canonical_creation_tx(
                    &test_params(),
                    test_creation_specs(),
                )),
            },
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap_err();
    assert!(format!("{err}").contains("yes_dormant_opening.asset_blinding_factor"));
}

#[test]
fn test_ingest_market_rejects_invalid_no_opening() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let mut anchor = test_market_metadata(&test_params()).anchor;
    anchor.no_dormant_opening.value_blinding_factor = "AA".repeat(32);
    let metadata = ContractMetadataInput {
        question: None,
        description: None,
        category: None,
        resolution_source: None,
        creator_pubkey: None,
        anchor,
        nevent: None,
        nostr_event_id: None,
        nostr_event_json: None,
    };
    let err = store
        .ingest_prediction_market_candidate(
            &PredictionMarketCandidateIngestInput {
                params: test_params(),
                metadata,
                creation_tx: serialize(&build_canonical_creation_tx(
                    &test_params(),
                    test_creation_specs(),
                )),
            },
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap_err();
    assert!(format!("{err}").contains("no_dormant_opening.value_blinding_factor"));
}

#[test]
fn test_ingest_market_canonicalizes_anchor_fields_before_persisting() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let expected_anchor = test_market_metadata(&params).anchor;

    let mut metadata = test_market_metadata(&params);
    metadata.anchor.creation_txid = format!(" {} ", metadata.anchor.creation_txid);
    metadata.anchor.yes_dormant_opening.asset_blinding_factor = format!(
        " {} ",
        metadata.anchor.yes_dormant_opening.asset_blinding_factor
    );
    metadata.anchor.no_dormant_opening.value_blinding_factor = format!(
        " {} ",
        metadata.anchor.no_dormant_opening.value_blinding_factor
    );

    let candidate_id = ingest_candidate(&mut store, &params, metadata, TEST_CANDIDATE_SEEN_AT);
    let info = store
        .get_prediction_market_candidate(candidate_id, TEST_CANDIDATE_SEEN_AT)
        .unwrap()
        .unwrap();

    assert_eq!(info.anchor, expected_anchor);
}

#[test]
fn test_ingest_market_reingest_normalizes_existing_anchor_fields() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("normalize-anchor.db");
    let params = test_params();
    let metadata = test_market_metadata(&params);
    let expected_anchor = metadata.anchor.clone();

    {
        let mut store = DeadcatStore::open(db_path.to_str().unwrap()).unwrap();
        let _ = ingest_candidate(
            &mut store,
            &params,
            metadata.clone(),
            TEST_CANDIDATE_SEEN_AT,
        );
    }

    let mut conn = SqliteConnection::establish(db_path.to_str().unwrap()).unwrap();
    diesel::sql_query(
        "UPDATE market_candidates
         SET creation_txid = ?,
             yes_dormant_asset_blinding_factor = ?,
             no_dormant_value_blinding_factor = ?",
    )
    .bind::<diesel::sql_types::Text, _>(format!(" {} ", expected_anchor.creation_txid))
    .bind::<diesel::sql_types::Binary, _>([0x99u8; 32].to_vec())
    .bind::<diesel::sql_types::Binary, _>([0x77u8; 32].to_vec())
    .execute(&mut conn)
    .unwrap();

    let mut store = DeadcatStore::open(db_path.to_str().unwrap()).unwrap();
    let candidate_id = ingest_candidate(&mut store, &params, metadata, TEST_CANDIDATE_SEEN_AT + 1);
    let info = store
        .get_prediction_market_candidate(candidate_id, TEST_CANDIDATE_SEEN_AT + 1)
        .unwrap()
        .unwrap();

    assert_eq!(info.anchor, expected_anchor);
}

#[test]
fn test_candidate_ingest_and_list_roundtrip() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let metadata = ContractMetadataInput {
        question: Some("Candidate question".to_string()),
        ..test_market_metadata(&params)
    };

    let candidate_id = ingest_candidate(&mut store, &params, metadata, TEST_CANDIDATE_SEEN_AT);

    assert!(store.get_market(&params.market_id()).unwrap().is_none());

    let candidate = store
        .get_prediction_market_candidate(candidate_id, TEST_CANDIDATE_SEEN_AT)
        .unwrap()
        .unwrap();
    assert_eq!(candidate.market_id, params.market_id());
    assert_eq!(candidate.question.as_deref(), Some("Candidate question"));
    assert!(candidate.expires_at.is_some());

    let listed = store
        .list_prediction_market_candidates(
            &MarketCandidateFilter {
                market_id: Some(params.market_id()),
                ..Default::default()
            },
            TEST_CANDIDATE_SEEN_AT,
        )
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].candidate_id, candidate_id);
}

#[test]
fn test_candidate_reingest_refreshes_ttl_without_duplication() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let metadata = test_market_metadata(&params);

    let candidate_id_1 = ingest_candidate(
        &mut store,
        &params,
        metadata.clone(),
        TEST_CANDIDATE_SEEN_AT,
    );
    let candidate_id_2 =
        ingest_candidate(&mut store, &params, metadata, TEST_CANDIDATE_SEEN_AT + 60);
    assert_eq!(candidate_id_1, candidate_id_2);

    let old_expiry = TEST_CANDIDATE_SEEN_AT + (6 * 60 * 60);
    let still_visible = store
        .list_prediction_market_candidates(&MarketCandidateFilter::default(), old_expiry + 1)
        .unwrap();
    assert_eq!(still_visible.len(), 1);

    let expired = store
        .list_prediction_market_candidates(
            &MarketCandidateFilter::default(),
            TEST_CANDIDATE_SEEN_AT + 60 + (6 * 60 * 60) + 1,
        )
        .unwrap();
    assert!(expired.is_empty());
}

#[test]
fn test_conflicting_candidate_same_market_id_creates_second_row() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let metadata_1 = test_market_metadata(&params);
    let mut metadata_2 = test_market_metadata(&params);
    metadata_2.anchor.yes_dormant_opening.value_blinding_factor = "33".repeat(32);

    let candidate_id_1 = ingest_candidate(&mut store, &params, metadata_1, TEST_CANDIDATE_SEEN_AT);
    let candidate_id_2 = ingest_candidate_with_matching_creation_tx(
        &mut store,
        &params,
        metadata_2,
        TEST_CANDIDATE_SEEN_AT + 1,
    );

    assert_ne!(candidate_id_1, candidate_id_2);
    let listed = store
        .list_prediction_market_candidates(
            &MarketCandidateFilter {
                market_id: Some(params.market_id()),
                ..Default::default()
            },
            TEST_CANDIDATE_SEEN_AT + 1,
        )
        .unwrap();
    assert_eq!(listed.len(), 2);
}

#[test]
fn test_candidate_purge_hides_then_deletes_expired_rows() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let candidate_id = ingest_candidate(
        &mut store,
        &params,
        test_market_metadata(&params),
        TEST_CANDIDATE_SEEN_AT,
    );

    assert!(
        store
            .get_prediction_market_candidate(
                candidate_id,
                TEST_CANDIDATE_SEEN_AT + (6 * 60 * 60) - 1
            )
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .get_prediction_market_candidate(candidate_id, TEST_CANDIDATE_SEEN_AT + (6 * 60 * 60))
            .unwrap()
            .is_none()
    );

    let purged = store
        .purge_expired_prediction_market_candidates(TEST_CANDIDATE_SEEN_AT + (6 * 60 * 60))
        .unwrap();
    assert_eq!(purged, 1);
    assert!(
        store
            .list_unpromoted_prediction_market_candidates()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_candidate_promotion_creates_canonical_market_and_deletes_siblings() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let metadata_1 = test_market_metadata(&params);
    let mut metadata_2 = test_market_metadata(&params);
    metadata_2.anchor.no_dormant_opening.asset_blinding_factor = "55".repeat(32);

    let candidate_id_1 = ingest_candidate(
        &mut store,
        &params,
        metadata_1.clone(),
        TEST_CANDIDATE_SEEN_AT,
    );
    let _candidate_id_2 = ingest_candidate_with_matching_creation_tx(
        &mut store,
        &params,
        metadata_2,
        TEST_CANDIDATE_SEEN_AT + 1,
    );

    promote_candidate(&mut store, candidate_id_1);

    let market = store.get_market(&params.market_id()).unwrap().unwrap();
    assert_eq!(market.anchor, metadata_1.anchor);
    assert!(
        store
            .list_unpromoted_prediction_market_candidates()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn test_promoting_second_candidate_after_canonicalization_is_rejected() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let metadata_1 = test_market_metadata(&params);
    let mut metadata_2 = test_market_metadata(&params);
    metadata_2.anchor.no_dormant_opening.value_blinding_factor = "77".repeat(32);

    let candidate_id_1 = ingest_candidate(&mut store, &params, metadata_1, TEST_CANDIDATE_SEEN_AT);
    promote_candidate(&mut store, candidate_id_1);

    let candidate_id_2 = ingest_candidate_with_matching_creation_tx(
        &mut store,
        &params,
        metadata_2,
        TEST_CANDIDATE_SEEN_AT + 1,
    );
    let err = store
        .promote_prediction_market_candidate(candidate_id_2, TEST_PROMOTED_AT + 1, 3, [0x99; 32])
        .unwrap_err()
        .to_string();
    assert!(err.contains("already has a canonical candidate"));
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
            ..test_market_metadata(&test_params())
        };
        ingest_test_market_with_metadata(&mut store, &test_params(), metadata)
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
    let params1 = test_params();
    let params2 = test_params_2();

    let metadata1 = ContractMetadataInput {
        question: Some("Question 1".to_string()),
        ..test_market_metadata(&params1)
    };
    let metadata2 = ContractMetadataInput {
        question: Some("Question 2".to_string()),
        ..test_market_metadata(&params2)
    };

    ingest_test_market_with_metadata(&mut store, &params1, metadata1);
    ingest_test_market_with_metadata(&mut store, &params2, metadata2);

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
        ..test_market_metadata(&params)
    };

    let market_id = ingest_test_market_with_metadata(&mut store, &params, metadata);
    let info = store.get_market(&market_id).unwrap().unwrap();

    assert_eq!(info.nostr_event_id.as_deref(), Some("abc123"));
    assert_eq!(info.nostr_event_json.as_deref(), Some(event_json));
}

#[test]
fn test_market_nostr_event_json_none_by_default() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let market_id = ingest_test_market(&mut store, &test_params());
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

#[test]
fn test_update_market_state_txid() {
    let mut store = DeadcatStore::open_in_memory().unwrap();
    let params = test_params();
    let mid = ingest_test_market(&mut store, &params);

    // All state variants should succeed without error
    store
        .update_market_state_txid(&mid, MarketState::Dormant, "txid_dormant")
        .unwrap();
    store
        .update_market_state_txid(&mid, MarketState::Unresolved, "txid_unresolved")
        .unwrap();
    store
        .update_market_state_txid(&mid, MarketState::ResolvedYes, "txid_yes")
        .unwrap();
    store
        .update_market_state_txid(&mid, MarketState::ResolvedNo, "txid_no")
        .unwrap();
    store
        .update_market_state_txid(&mid, MarketState::Expired, "txid_expired")
        .unwrap();

    // Market still exists and is readable
    let info = store.get_market(&mid).unwrap();
    assert!(info.is_some());
}
