//! Test utilities for validating assembled transactions against ElementsEnv.
//!
//! This module bridges the assembly pipeline with the Simplicity execution
//! environment, enabling integration tests that cover the full path from
//! PSET construction through witness satisfaction without a live network.

use std::sync::Arc;

use lwk_wollet::elements_miniscript::confidential::slip77::MasterBlindingKey;
use simplicityhl::elements::confidential::{Asset, Nonce, Value as ConfValue};
use simplicityhl::elements::hashes::Hash;
use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::secp256k1_zkp::{
    Generator, Keypair, Message, PedersenCommitment, PublicKey, Secp256k1, SecretKey, Tag, Tweak,
    XOnlyPublicKey, ZERO_TWEAK,
};
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::elements::{
    AssetId, AssetIssuance, BlockHash, ContractHash, LockTime, OutPoint, Script, Sequence,
    Transaction, TxIn, TxInWitness, TxOut, TxOutWitness, Txid,
};
use simplicityhl::simplicity::bit_machine::BitMachine;
use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

use crate::assembly::pset_to_pruning_transaction;
use crate::error::{Error, Result};
use crate::prediction_market::anchor::PredictionMarketAnchor;
use crate::prediction_market::assembly::{
    IssuanceAssemblyInputs, assemble_cancellation, assemble_expire_transition,
    assemble_expiry_redemption, assemble_oracle_resolve, assemble_post_resolution_redemption,
    blind_issuance_pset, build_issuance_pset,
};
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::oracle::oracle_message;
use crate::prediction_market::params::{PredictionMarketParams, compute_issuance_assets};
use crate::prediction_market::pset::cancellation::CancellationParams;
use crate::prediction_market::pset::expire_transition::ExpireTransitionParams;
use crate::prediction_market::pset::expiry_redemption::ExpiryRedemptionParams;
use crate::prediction_market::pset::oracle_resolve::OracleResolveParams;
use crate::prediction_market::pset::post_resolution_redemption::PostResolutionRedemptionParams;
use crate::prediction_market::state::{MarketSlot, MarketState};
use crate::prediction_market::witness::{
    AllBlindingFactors, PredictionMarketSpendingPath, ReissuanceBlindingFactors, satisfy_contract,
};
use crate::pset::UnblindedUtxo;

// ---------------------------------------------------------------------------
// Shared test helpers (promoted from tests/elements_env_execution.rs)
// ---------------------------------------------------------------------------

/// Build an explicit (non-confidential) TxOut for tests.
pub fn explicit_txout(asset_bytes: &[u8; 32], amount: u64, spk: &Script) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_bytes).expect("valid asset")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: spk.clone(),
        witness: TxOutWitness::default(),
    }
}

/// Build a simple TxIn with no issuance.
pub fn simple_txin(outpoint: OutPoint) -> TxIn {
    TxIn {
        previous_output: outpoint,
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::ENABLE_LOCKTIME_NO_RBF,
        asset_issuance: Default::default(),
        witness: Default::default(),
    }
}

/// Build a TxOut with confidential (Pedersen committed) asset and value for a
/// reissuance token. The commitments match the given blinding factors.
pub fn confidential_rt_txout(
    asset_bytes: &[u8; 32],
    abf: &[u8; 32],
    vbf: &[u8; 32],
    spk: &Script,
) -> TxOut {
    let secp = Secp256k1::new();
    let tag = Tag::from(*asset_bytes);
    let abf_tweak = Tweak::from_slice(abf).expect("valid ABF");
    let vbf_tweak = Tweak::from_slice(vbf).expect("valid VBF");
    let generator = Generator::new_blinded(&secp, tag, abf_tweak);
    let commitment = PedersenCommitment::new(&secp, 1, vbf_tweak, generator);
    TxOut {
        asset: Asset::Confidential(generator),
        value: ConfValue::Confidential(commitment),
        nonce: Nonce::Null,
        script_pubkey: spk.clone(),
        witness: TxOutWitness::default(),
    }
}

/// Build a dormant creation TxOut with confidential commitments and a
/// non-null nonce so it is valid for proof-carrying market bootstrap
/// validation.
pub fn confidential_dormant_creation_txout(
    asset_bytes: &[u8; 32],
    abf: &[u8; 32],
    vbf: &[u8; 32],
    spk: &Script,
) -> TxOut {
    let secp = Secp256k1::new();
    let tag = Tag::from(*asset_bytes);
    let abf_tweak = Tweak::from_slice(abf).expect("valid ABF");
    let vbf_tweak = Tweak::from_slice(vbf).expect("valid VBF");
    let generator = Generator::new_blinded(&secp, tag, abf_tweak);
    let commitment = PedersenCommitment::new(&secp, 1, vbf_tweak, generator);
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
        script_pubkey: spk.clone(),
        witness: TxOutWitness::default(),
    }
}

/// Build a TxIn with asset issuance set (explicit amount = `pairs`).
pub fn issuance_txin(outpoint: OutPoint, pairs: u64) -> TxIn {
    TxIn {
        previous_output: outpoint,
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::ENABLE_LOCKTIME_NO_RBF,
        asset_issuance: AssetIssuance {
            asset_blinding_nonce: Tweak::from_slice(&[0x10; 32]).expect("valid nonce"),
            asset_entropy: [0x20; 32],
            amount: ConfValue::Explicit(pairs),
            inflation_keys: ConfValue::Null,
        },
        witness: Default::default(),
    }
}

/// Standard test blinding factors.
pub fn test_blinding() -> AllBlindingFactors {
    AllBlindingFactors {
        yes: ReissuanceBlindingFactors {
            input_abf: [0x01; 32],
            input_vbf: [0x02; 32],
            output_abf: [0x03; 32],
            output_vbf: [0x04; 32],
        },
        no: ReissuanceBlindingFactors {
            input_abf: [0x05; 32],
            input_vbf: [0x06; 32],
            output_abf: [0x07; 32],
            output_vbf: [0x08; 32],
        },
    }
}

pub fn test_script(tag: u8) -> Script {
    let mut bytes = vec![0x00, 0x14];
    bytes.extend_from_slice(&[tag; 20]);
    Script::from(bytes)
}

pub fn test_change_script() -> Script {
    test_script(0)
}

pub fn test_slip77_master_blinding_key() -> MasterBlindingKey {
    MasterBlindingKey::from_seed(b"deadcat-elements-env-slip77")
}

pub fn test_blinding_pubkey(spk: &Script) -> PublicKey {
    let secp = Secp256k1::new();
    test_slip77_master_blinding_key().blinding_key(&secp, spk)
}

pub fn test_outpoint(tag: u8) -> OutPoint {
    OutPoint::new(Txid::from_byte_array([tag; 32]), 0)
}

pub fn test_explicit_utxo(asset_id: &[u8; 32], value: u64, spk: &Script, tag: u8) -> UnblindedUtxo {
    UnblindedUtxo {
        outpoint: test_outpoint(tag),
        txout: explicit_txout(asset_id, value, spk),
        asset_id: *asset_id,
        value,
        asset_blinding_factor: [0u8; 32],
        value_blinding_factor: [0u8; 32],
    }
}

pub fn test_confidential_rt_utxo(
    asset_id: &[u8; 32],
    spk: &Script,
    abf: &[u8; 32],
    vbf: &[u8; 32],
    tag: u8,
) -> UnblindedUtxo {
    UnblindedUtxo {
        outpoint: test_outpoint(tag),
        txout: confidential_rt_txout(asset_id, abf, vbf, spk),
        asset_id: *asset_id,
        value: 1,
        asset_blinding_factor: *abf,
        value_blinding_factor: *vbf,
    }
}

pub fn test_oracle_keypair() -> ([u8; 32], Keypair) {
    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[0x42; 32]).expect("valid oracle secret key");
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let (pubkey, _) = XOnlyPublicKey::from_keypair(&keypair);
    (pubkey.serialize(), keypair)
}

pub fn test_oracle_signature(
    params: &PredictionMarketParams,
    outcome_yes: bool,
    keypair: &Keypair,
) -> [u8; 64] {
    let secp = Secp256k1::new();
    let msg = Message::from_digest(oracle_message(&params.market_id(), outcome_yes));
    secp.sign_schnorr(&msg, keypair).serialize()
}

// ---------------------------------------------------------------------------
// ElementsEnv execution
// ---------------------------------------------------------------------------

/// Execute a satisfied Simplicity program against a mock ElementsEnv.
///
/// Returns Ok(()) on success, or an error description on failure.
pub fn execute_against_env(
    contract: &CompiledPredictionMarket,
    slot: MarketSlot,
    path: &PredictionMarketSpendingPath,
    tx: Arc<Transaction>,
    utxos: Vec<ElementsUtxo>,
    input_index: u32,
) -> std::result::Result<(), String> {
    let satisfied = satisfy_contract(contract, path, slot).map_err(|e| format!("satisfy: {e}"))?;
    let redeem = satisfied.redeem();

    let cb_bytes = contract.control_block(slot);
    let control_block =
        ControlBlock::from_slice(&cb_bytes).map_err(|e| format!("control block: {e}"))?;

    let env = ElementsEnv::new(
        tx,
        utxos,
        input_index,
        *contract.cmr(),
        control_block,
        None,
        BlockHash::all_zeros(),
    );

    let mut machine = BitMachine::for_program(redeem).map_err(|e| format!("bit machine: {e}"))?;

    machine
        .exec(redeem, &env)
        .map(|_| ())
        .map_err(|e| format!("execution failed: {e}"))
}

// ---------------------------------------------------------------------------
// Assembly → ElementsEnv bridge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EnvCovenantInput {
    pub input_index: u32,
    pub slot: MarketSlot,
    pub path: PredictionMarketSpendingPath,
}

pub struct AssembledEnvTx {
    pub tx: Arc<Transaction>,
    pub utxos: Vec<ElementsUtxo>,
    pub covenant_inputs: Vec<EnvCovenantInput>,
}

pub type TestCancellationParams = CancellationParams;
pub type TestExpireTransitionParams = ExpireTransitionParams;
pub type TestExpiryRedemptionParams = ExpiryRedemptionParams;
pub type TestOracleResolveParams = OracleResolveParams;
pub type TestPostResolutionRedemptionParams = PostResolutionRedemptionParams;

fn pset_to_env_tx(
    pset: PartiallySignedTransaction,
    covenant_inputs: Vec<EnvCovenantInput>,
) -> Result<AssembledEnvTx> {
    let tx = Arc::new(pset_to_pruning_transaction(&pset)?);
    let utxos = pset
        .inputs()
        .iter()
        .enumerate()
        .map(|(i, input)| {
            input
                .witness_utxo
                .clone()
                .map(ElementsUtxo::from)
                .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(AssembledEnvTx {
        tx,
        utxos,
        covenant_inputs,
    })
}

fn recover_rt_blinding_factors(
    pset: &PartiallySignedTransaction,
    slip77_key: &MasterBlindingKey,
    change_spk: &Script,
    yes_rt_input: &UnblindedUtxo,
    no_rt_input: &UnblindedUtxo,
) -> Result<AllBlindingFactors> {
    let secp = Secp256k1::new();
    let blinding_sk = slip77_key.blinding_private_key(change_spk);

    let yes_secrets = pset.outputs()[0]
        .to_txout()
        .unblind(&secp, blinding_sk)
        .map_err(|e| Error::Blinding(format!("unblind YES RT output: {e}")))?;
    let no_secrets = pset.outputs()[1]
        .to_txout()
        .unblind(&secp, blinding_sk)
        .map_err(|e| Error::Blinding(format!("unblind NO RT output: {e}")))?;

    let mut yes_output_abf = [0u8; 32];
    yes_output_abf.copy_from_slice(yes_secrets.asset_bf.into_inner().as_ref());
    let mut yes_output_vbf = [0u8; 32];
    yes_output_vbf.copy_from_slice(yes_secrets.value_bf.into_inner().as_ref());
    let mut no_output_abf = [0u8; 32];
    no_output_abf.copy_from_slice(no_secrets.asset_bf.into_inner().as_ref());
    let mut no_output_vbf = [0u8; 32];
    no_output_vbf.copy_from_slice(no_secrets.value_bf.into_inner().as_ref());

    Ok(AllBlindingFactors {
        yes: ReissuanceBlindingFactors {
            input_abf: yes_rt_input.asset_blinding_factor,
            input_vbf: yes_rt_input.value_blinding_factor,
            output_abf: yes_output_abf,
            output_vbf: yes_output_vbf,
        },
        no: ReissuanceBlindingFactors {
            input_abf: no_rt_input.asset_blinding_factor,
            input_vbf: no_rt_input.value_blinding_factor,
            output_abf: no_output_abf,
            output_vbf: no_output_vbf,
        },
    })
}

pub fn assemble_issuance_for_env(inputs: IssuanceAssemblyInputs) -> Result<AssembledEnvTx> {
    let state = inputs.current_state;
    let yes_rt_input = inputs.yes_reissuance_utxo.clone();
    let no_rt_input = inputs.no_reissuance_utxo.clone();
    let change_spk = test_change_script();
    let slip77_key = test_slip77_master_blinding_key();
    let blinding_pubkey = test_blinding_pubkey(&change_spk);
    let mut pset = build_issuance_pset(&inputs)?;
    blind_issuance_pset(&mut pset, &inputs, blinding_pubkey)?;
    let blinding =
        recover_rt_blinding_factors(&pset, &slip77_key, &change_spk, &yes_rt_input, &no_rt_input)?;

    let covenant_inputs = match state {
        MarketState::Dormant => vec![
            EnvCovenantInput {
                input_index: 0,
                slot: MarketSlot::DormantYesRt,
                path: PredictionMarketSpendingPath::InitialIssuancePrimary { blinding },
            },
            EnvCovenantInput {
                input_index: 1,
                slot: MarketSlot::DormantNoRt,
                path: PredictionMarketSpendingPath::InitialIssuanceSecondaryNoRt { blinding },
            },
        ],
        MarketState::Unresolved => vec![
            EnvCovenantInput {
                input_index: 0,
                slot: MarketSlot::UnresolvedYesRt,
                path: PredictionMarketSpendingPath::SubsequentIssuancePrimary { blinding },
            },
            EnvCovenantInput {
                input_index: 1,
                slot: MarketSlot::UnresolvedNoRt,
                path: PredictionMarketSpendingPath::SubsequentIssuanceSecondaryNoRt { blinding },
            },
            EnvCovenantInput {
                input_index: 2,
                slot: MarketSlot::UnresolvedCollateral,
                path: PredictionMarketSpendingPath::SubsequentIssuanceSecondaryCollateral,
            },
        ],
        other => return Err(Error::NotIssuable(other)),
    };

    pset_to_env_tx(pset, covenant_inputs)
}

pub fn assemble_post_resolution_redemption_for_env(
    contract: &CompiledPredictionMarket,
    params: TestPostResolutionRedemptionParams,
) -> Result<AssembledEnvTx> {
    let resolved_state = params.resolved_state;
    let tokens_burned = params.tokens_burned;
    let blinding_pubkey = test_blinding_pubkey(&test_change_script());
    let pset = assemble_post_resolution_redemption(contract, &params, blinding_pubkey)?;
    let slot = resolved_state
        .collateral_slot()
        .ok_or(Error::InvalidState)?;
    let covenant_inputs = vec![EnvCovenantInput {
        input_index: 0,
        slot,
        path: PredictionMarketSpendingPath::PostResolutionRedemption { tokens_burned },
    }];

    pset_to_env_tx(pset, covenant_inputs)
}

pub fn assemble_expiry_redemption_for_env(
    contract: &CompiledPredictionMarket,
    params: TestExpiryRedemptionParams,
) -> Result<AssembledEnvTx> {
    let tokens_burned = params.tokens_burned;
    let burn_token_asset = params.burn_token_asset;
    let blinding_pubkey = test_blinding_pubkey(&test_change_script());
    let pset = assemble_expiry_redemption(contract, &params, blinding_pubkey)?;
    let covenant_inputs = vec![EnvCovenantInput {
        input_index: 0,
        slot: MarketSlot::ExpiredCollateral,
        path: PredictionMarketSpendingPath::ExpiryRedemption {
            tokens_burned,
            burn_token_asset,
        },
    }];

    pset_to_env_tx(pset, covenant_inputs)
}

pub fn assemble_cancellation_for_env(
    contract: &CompiledPredictionMarket,
    params: TestCancellationParams,
) -> Result<AssembledEnvTx> {
    let change_spk = test_change_script();
    let slip77_key = test_slip77_master_blinding_key();
    let blinding_pubkey = test_blinding_pubkey(&change_spk);
    let is_full = params.collateral_utxo.value
        == params
            .pairs_burned
            .checked_mul(2)
            .and_then(|v| v.checked_mul(contract.params().collateral_per_token))
            .ok_or(Error::CollateralOverflow)?;
    let yes_rt_input = params.yes_reissuance_utxo.clone();
    let no_rt_input = params.no_reissuance_utxo.clone();
    let pairs_burned = params.pairs_burned;

    let pset = assemble_cancellation(contract, &params, &slip77_key, blinding_pubkey, &change_spk)?;

    let covenant_inputs = if is_full {
        let yes_rt_input = yes_rt_input.ok_or(Error::MissingReissuanceUtxos)?;
        let no_rt_input = no_rt_input.ok_or(Error::MissingReissuanceUtxos)?;
        let blinding = recover_rt_blinding_factors(
            &pset,
            &slip77_key,
            &change_spk,
            &yes_rt_input,
            &no_rt_input,
        )?;
        vec![
            EnvCovenantInput {
                input_index: 0,
                slot: MarketSlot::UnresolvedCollateral,
                path: PredictionMarketSpendingPath::CancellationFullPrimary {
                    pairs_burned,
                    blinding,
                },
            },
            EnvCovenantInput {
                input_index: 1,
                slot: MarketSlot::UnresolvedYesRt,
                path: PredictionMarketSpendingPath::CancellationFullSecondaryYesRt { blinding },
            },
            EnvCovenantInput {
                input_index: 2,
                slot: MarketSlot::UnresolvedNoRt,
                path: PredictionMarketSpendingPath::CancellationFullSecondaryNoRt { blinding },
            },
        ]
    } else {
        vec![EnvCovenantInput {
            input_index: 0,
            slot: MarketSlot::UnresolvedCollateral,
            path: PredictionMarketSpendingPath::CancellationPartial { pairs_burned },
        }]
    };

    pset_to_env_tx(pset, covenant_inputs)
}

pub fn assemble_expire_transition_for_env(
    contract: &CompiledPredictionMarket,
    params: TestExpireTransitionParams,
) -> Result<AssembledEnvTx> {
    let yes_rt_input = params.yes_reissuance_utxo.clone();
    let no_rt_input = params.no_reissuance_utxo.clone();
    let change_spk = test_change_script();
    let slip77_key = test_slip77_master_blinding_key();
    let blinding_pubkey = test_blinding_pubkey(&change_spk);
    let pset = assemble_expire_transition(
        contract,
        &params,
        &slip77_key,
        blinding_pubkey,
        &change_spk,
        &yes_rt_input,
        &no_rt_input,
    )?;
    let blinding =
        recover_rt_blinding_factors(&pset, &slip77_key, &change_spk, &yes_rt_input, &no_rt_input)?;
    let covenant_inputs = vec![
        EnvCovenantInput {
            input_index: 0,
            slot: MarketSlot::UnresolvedYesRt,
            path: PredictionMarketSpendingPath::ExpireTransitionPrimary { blinding },
        },
        EnvCovenantInput {
            input_index: 1,
            slot: MarketSlot::UnresolvedNoRt,
            path: PredictionMarketSpendingPath::ExpireTransitionSecondaryNoRt { blinding },
        },
        EnvCovenantInput {
            input_index: 2,
            slot: MarketSlot::UnresolvedCollateral,
            path: PredictionMarketSpendingPath::ExpireTransitionSecondaryCollateral,
        },
    ];

    pset_to_env_tx(pset, covenant_inputs)
}

pub fn assemble_oracle_resolve_for_env(
    contract: &CompiledPredictionMarket,
    params: TestOracleResolveParams,
    oracle_signature: [u8; 64],
) -> Result<AssembledEnvTx> {
    let yes_rt_input = params.yes_reissuance_utxo.clone();
    let no_rt_input = params.no_reissuance_utxo.clone();
    let outcome_yes = params.outcome_yes;
    let change_spk = test_change_script();
    let slip77_key = test_slip77_master_blinding_key();
    let blinding_pubkey = test_blinding_pubkey(&change_spk);
    let pset = assemble_oracle_resolve(
        contract,
        &params,
        oracle_signature,
        &slip77_key,
        blinding_pubkey,
        &change_spk,
        &yes_rt_input,
        &no_rt_input,
    )?;
    let blinding =
        recover_rt_blinding_factors(&pset, &slip77_key, &change_spk, &yes_rt_input, &no_rt_input)?;
    let covenant_inputs = vec![
        EnvCovenantInput {
            input_index: 0,
            slot: MarketSlot::UnresolvedYesRt,
            path: PredictionMarketSpendingPath::OracleResolvePrimary {
                outcome_yes,
                oracle_signature,
                blinding,
            },
        },
        EnvCovenantInput {
            input_index: 1,
            slot: MarketSlot::UnresolvedNoRt,
            path: PredictionMarketSpendingPath::OracleResolveSecondaryNoRt { blinding },
        },
        EnvCovenantInput {
            input_index: 2,
            slot: MarketSlot::UnresolvedCollateral,
            path: PredictionMarketSpendingPath::OracleResolveSecondaryCollateral,
        },
    ];

    pset_to_env_tx(pset, covenant_inputs)
}

// ---------------------------------------------------------------------------
// In-memory discovery store for tests
// ---------------------------------------------------------------------------

use nostr_sdk::Keys;

use crate::announcement::{CONTRACT_ANNOUNCEMENT_VERSION, ContractAnnouncement, ContractMetadata};
use crate::discovery::store_trait::{
    DiscoveryStore, LmsrPoolIngestInput, LmsrPoolStateSource, LmsrPoolStateUpdateInput, NodeStore,
    OwnMakerOrderRecordInput, OwnOrderStatusChange, PendingOrderDeletion,
    PredictionMarketCandidateIngestInput,
};
use crate::discovery::{OrderAnnouncement, PoolAnnouncement};
use crate::history::{LmsrPoolSyncInfo, LmsrPriceHistoryEntry, LmsrPriceTransitionInput};
use crate::lmsr_pool::api::{
    LmsrPoolLocator, LmsrPoolSnapshot, build_pool_announcement_from_snapshot,
    txid_to_canonical_bytes,
};
use crate::lmsr_pool::identity::{derive_lmsr_market_id, derive_lmsr_pool_id};
use crate::lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolParams};
use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::network::Network;
use crate::pool::PoolReserves;
use crate::taproot::NUMS_KEY_BYTES;

/// Minimal in-memory store implementing `DiscoveryStore` for integration tests.
///
/// Deduplicates markets by `market_id` and stores orders as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedOwnOrder {
    pub params: MakerOrderParams,
    pub maker_pubkey: [u8; 32],
    pub order_nonce: [u8; 32],
    pub nostr_event_id: String,
    pub creation_txid: String,
    pub market_id: String,
    pub direction_label: String,
    pub offered_amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedDeletionResult {
    pub order_id: i32,
    pub delete_event_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct TestStore {
    pub markets: Vec<PredictionMarketCandidateIngestInput>,
    pub orders: Vec<(MakerOrderParams, Option<String>)>,
    pub pools: Vec<LmsrPoolIngestInput>,
    pub pool_states: Vec<LmsrPoolStateUpdateInput>,
    pub price_history: Vec<LmsrPriceHistoryEntry>,
    pub own_orders: Vec<RecordedOwnOrder>,
    pub cancelled_orders: Vec<(MakerOrderParams, [u8; 32])>,
    pub pending_order_deletions: Vec<PendingOrderDeletion>,
    pub synced_order_status_changes: Vec<OwnOrderStatusChange>,
    pub synced_electrum_urls: Vec<String>,
    pub deletion_results: Vec<RecordedDeletionResult>,
}

fn should_preserve_canonical_lmsr_state(
    existing: &LmsrPoolIngestInput,
    incoming: &LmsrPoolIngestInput,
) -> bool {
    existing.state_source == LmsrPoolStateSource::CanonicalScan
        && incoming.state_source == LmsrPoolStateSource::Announcement
}

fn merge_lmsr_pool_ingest(existing: &mut LmsrPoolIngestInput, incoming: &LmsrPoolIngestInput) {
    let preserve_canonical_state = should_preserve_canonical_lmsr_state(existing, incoming);
    let preserved_identity = (
        existing.market_id.clone(),
        existing.creation_txid.clone(),
        existing.witness_schema_version.clone(),
        existing.yes_asset_id,
        existing.no_asset_id,
        existing.collateral_asset_id,
        existing.fee_bps,
        existing.cosigner_pubkey,
        existing.lmsr_table_root,
        existing.table_depth,
        existing.q_step_lots,
        existing.s_bias,
        existing.s_max_index,
        existing.half_payout_sats,
        existing.min_r_yes,
        existing.min_r_no,
        existing.min_r_collateral,
        existing.initial_reserve_outpoints.clone(),
    );
    let preserved_state = preserve_canonical_state.then(|| {
        (
            existing.current_s_index,
            existing.reserve_outpoints.clone(),
            existing.reserve_yes,
            existing.reserve_no,
            existing.reserve_collateral,
            existing.state_source,
            existing.last_transition_txid.clone(),
        )
    });
    let merged_event_id = incoming
        .nostr_event_id
        .clone()
        .or_else(|| existing.nostr_event_id.clone());
    let merged_event_json = incoming
        .nostr_event_json
        .clone()
        .or_else(|| existing.nostr_event_json.clone());
    let merged_table_values = incoming
        .lmsr_table_values
        .clone()
        .or_else(|| existing.lmsr_table_values.clone());

    *existing = incoming.clone();

    let (
        market_id,
        creation_txid,
        witness_schema_version,
        yes_asset_id,
        no_asset_id,
        collateral_asset_id,
        fee_bps,
        cosigner_pubkey,
        lmsr_table_root,
        table_depth,
        q_step_lots,
        s_bias,
        s_max_index,
        half_payout_sats,
        min_r_yes,
        min_r_no,
        min_r_collateral,
        initial_reserve_outpoints,
    ) = preserved_identity;
    existing.market_id = market_id;
    existing.creation_txid = creation_txid;
    existing.witness_schema_version = witness_schema_version;
    existing.yes_asset_id = yes_asset_id;
    existing.no_asset_id = no_asset_id;
    existing.collateral_asset_id = collateral_asset_id;
    existing.fee_bps = fee_bps;
    existing.cosigner_pubkey = cosigner_pubkey;
    existing.lmsr_table_root = lmsr_table_root;
    existing.table_depth = table_depth;
    existing.q_step_lots = q_step_lots;
    existing.s_bias = s_bias;
    existing.s_max_index = s_max_index;
    existing.half_payout_sats = half_payout_sats;
    existing.min_r_yes = min_r_yes;
    existing.min_r_no = min_r_no;
    existing.min_r_collateral = min_r_collateral;
    existing.initial_reserve_outpoints = initial_reserve_outpoints;
    existing.lmsr_table_values = merged_table_values;

    if let Some((
        current_s_index,
        reserve_outpoints,
        reserve_yes,
        reserve_no,
        reserve_collateral,
        state_source,
        last_transition_txid,
    )) = preserved_state
    {
        existing.current_s_index = current_s_index;
        existing.reserve_outpoints = reserve_outpoints;
        existing.reserve_yes = reserve_yes;
        existing.reserve_no = reserve_no;
        existing.reserve_collateral = reserve_collateral;
        existing.state_source = state_source;
        existing.last_transition_txid = last_transition_txid;
    }

    existing.nostr_event_id = merged_event_id;
    existing.nostr_event_json = merged_event_json;
}

impl DiscoveryStore for TestStore {
    fn ingest_prediction_market_candidate(
        &mut self,
        input: &PredictionMarketCandidateIngestInput,
        _seen_at_unix: u64,
    ) -> std::result::Result<(), String> {
        let mid = input.params.market_id();
        let same_anchor = |existing: &PredictionMarketCandidateIngestInput| {
            existing.params.market_id() == mid && existing.metadata.anchor == input.metadata.anchor
        };
        if !self.markets.iter().any(same_anchor) {
            self.markets.push(input.clone());
        }
        Ok(())
    }

    fn ingest_maker_order(
        &mut self,
        params: &MakerOrderParams,
        _maker_pubkey: Option<&[u8; 32]>,
        _nonce: Option<&[u8; 32]>,
        nostr_event_id: Option<&str>,
        _nostr_event_json: Option<&str>,
    ) -> std::result::Result<(), String> {
        self.orders
            .push((*params, nostr_event_id.map(|s| s.to_string())));
        Ok(())
    }

    fn ingest_lmsr_pool(&mut self, input: &LmsrPoolIngestInput) -> std::result::Result<(), String> {
        match self
            .pools
            .iter_mut()
            .find(|existing| existing.pool_id == input.pool_id)
        {
            Some(existing) => merge_lmsr_pool_ingest(existing, input),
            None => self.pools.push(input.clone()),
        }
        Ok(())
    }

    fn upsert_lmsr_pool_state(
        &mut self,
        input: &LmsrPoolStateUpdateInput,
    ) -> std::result::Result<(), String> {
        self.pool_states.push(input.clone());
        Ok(())
    }

    fn record_own_maker_order(
        &mut self,
        input: OwnMakerOrderRecordInput<'_>,
    ) -> std::result::Result<(), String> {
        self.own_orders.push(RecordedOwnOrder {
            params: *input.params,
            maker_pubkey: *input.maker_pubkey,
            order_nonce: *input.order_nonce,
            nostr_event_id: input.nostr_event_id.to_string(),
            creation_txid: input.creation_txid.to_string(),
            market_id: input.market_id.to_string(),
            direction_label: input.direction_label.to_string(),
            offered_amount: input.offered_amount,
        });
        Ok(())
    }

    fn mark_own_maker_order_cancelled(
        &mut self,
        params: &MakerOrderParams,
        maker_pubkey: &[u8; 32],
    ) -> std::result::Result<Option<PendingOrderDeletion>, String> {
        self.cancelled_orders.push((*params, *maker_pubkey));
        Ok(self
            .pending_order_deletions
            .iter()
            .find(|pending| pending.maker_base_pubkey == *maker_pubkey)
            .cloned())
    }

    fn sync_own_order_state(
        &mut self,
        electrum_url: &str,
    ) -> std::result::Result<Vec<OwnOrderStatusChange>, String> {
        self.synced_electrum_urls.push(electrum_url.to_string());
        Ok(self.synced_order_status_changes.clone())
    }

    fn list_pending_order_deletions(
        &mut self,
    ) -> std::result::Result<Vec<PendingOrderDeletion>, String> {
        Ok(self.pending_order_deletions.clone())
    }

    fn list_own_maker_pubkeys(&mut self) -> std::result::Result<Vec<[u8; 32]>, String> {
        let mut pubkeys = Vec::new();
        for order in &self.own_orders {
            if !pubkeys.contains(&order.maker_pubkey) {
                pubkeys.push(order.maker_pubkey);
            }
        }
        Ok(pubkeys)
    }

    fn list_known_prediction_markets(
        &mut self,
    ) -> std::result::Result<Vec<PredictionMarketParams>, String> {
        Ok(self.markets.iter().map(|market| market.params).collect())
    }

    fn record_order_deletion_result(
        &mut self,
        order_id: i32,
        delete_event_id: Option<&str>,
        error: Option<&str>,
    ) -> std::result::Result<(), String> {
        self.deletion_results.push(RecordedDeletionResult {
            order_id,
            delete_event_id: delete_event_id.map(str::to_string),
            error: error.map(str::to_string),
        });
        if delete_event_id.is_some() {
            self.pending_order_deletions
                .retain(|pending| pending.order_id != order_id);
        }
        Ok(())
    }
}

impl NodeStore for TestStore {
    fn list_lmsr_pool_sync_info(&mut self) -> std::result::Result<Vec<LmsrPoolSyncInfo>, String> {
        self.pools
            .iter()
            .map(|pool| {
                let params = crate::lmsr_pool::params::LmsrPoolParams {
                    yes_asset_id: pool.yes_asset_id,
                    no_asset_id: pool.no_asset_id,
                    collateral_asset_id: pool.collateral_asset_id,
                    fee_bps: pool.fee_bps,
                    cosigner_pubkey: pool.cosigner_pubkey,
                    lmsr_table_root: pool.lmsr_table_root,
                    table_depth: pool.table_depth,
                    q_step_lots: pool.q_step_lots,
                    s_bias: pool.s_bias,
                    s_max_index: pool.s_max_index,
                    half_payout_sats: pool.half_payout_sats,
                    min_r_yes: pool.min_r_yes,
                    min_r_no: pool.min_r_no,
                    min_r_collateral: pool.min_r_collateral,
                };
                Ok(LmsrPoolSyncInfo {
                    pool_id: pool.pool_id.clone(),
                    market_id: pool.market_id.clone(),
                    creation_txid: pool.creation_txid.clone(),
                    stored_initial_reserve_outpoints: Some(pool.initial_reserve_outpoints.clone()),
                    witness_schema_version: pool.witness_schema_version.clone(),
                    current_s_index: pool.current_s_index,
                    params_json: serde_json::to_string(&params)
                        .map_err(|e| format!("serialize test lmsr params: {e}"))?,
                    lmsr_table_values: pool.lmsr_table_values.clone(),
                    nostr_event_json: pool.nostr_event_json.clone(),
                })
            })
            .collect()
    }

    fn repair_lmsr_pool_sync_info(
        &mut self,
        input: &crate::LmsrPoolSyncRepairInput,
    ) -> std::result::Result<(), String> {
        let Some(pool) = self
            .pools
            .iter_mut()
            .find(|pool| pool.pool_id == input.pool_id)
        else {
            return Err(format!(
                "cannot repair LMSR sync metadata for unknown pool_id {}",
                input.pool_id
            ));
        };
        pool.market_id = input.market_id.clone();
        pool.creation_txid = input.creation_txid.clone();
        pool.witness_schema_version = input.witness_schema_version.clone();
        pool.yes_asset_id = input.params.yes_asset_id;
        pool.no_asset_id = input.params.no_asset_id;
        pool.collateral_asset_id = input.params.collateral_asset_id;
        pool.fee_bps = input.params.fee_bps;
        pool.cosigner_pubkey = input.params.cosigner_pubkey;
        pool.lmsr_table_root = input.params.lmsr_table_root;
        pool.table_depth = input.params.table_depth;
        pool.q_step_lots = input.params.q_step_lots;
        pool.s_bias = input.params.s_bias;
        pool.s_max_index = input.params.s_max_index;
        pool.half_payout_sats = input.params.half_payout_sats;
        pool.min_r_yes = input.params.min_r_yes;
        pool.min_r_no = input.params.min_r_no;
        pool.min_r_collateral = input.params.min_r_collateral;
        pool.initial_reserve_outpoints = input.initial_reserve_outpoints.clone();
        if let Some(table_values) = input.lmsr_table_values.clone() {
            pool.lmsr_table_values = Some(table_values);
        }
        Ok(())
    }

    fn record_lmsr_price_transition(
        &mut self,
        input: &LmsrPriceTransitionInput,
    ) -> std::result::Result<(), String> {
        if self.price_history.iter().any(|entry| {
            entry.pool_id == input.pool_id && entry.transition_txid == input.transition_txid
        }) {
            return Ok(());
        }
        self.price_history.push(LmsrPriceHistoryEntry {
            pool_id: input.pool_id.clone(),
            market_id: input.market_id.clone(),
            transition_txid: input.transition_txid.clone(),
            old_s_index: input.old_s_index,
            new_s_index: input.new_s_index,
            reserve_yes: input.reserve_yes,
            reserve_no: input.reserve_no,
            reserve_collateral: input.reserve_collateral,
            implied_yes_price_bps: input.implied_yes_price_bps,
            block_height: input.block_height,
        });
        self.price_history.sort_by(|a, b| {
            a.block_height
                .cmp(&b.block_height)
                .then_with(|| a.transition_txid.cmp(&b.transition_txid))
        });
        Ok(())
    }

    fn get_market_price_history(
        &mut self,
        market_id: &str,
        since_block_height: Option<u32>,
        limit: Option<i64>,
    ) -> std::result::Result<Vec<LmsrPriceHistoryEntry>, String> {
        Ok(filter_test_price_history(
            &self.price_history,
            |entry| entry.market_id == market_id,
            since_block_height,
            limit,
        ))
    }

    fn get_pool_price_history(
        &mut self,
        pool_id: &str,
        since_block_height: Option<u32>,
        limit: Option<i64>,
    ) -> std::result::Result<Vec<LmsrPriceHistoryEntry>, String> {
        Ok(filter_test_price_history(
            &self.price_history,
            |entry| entry.pool_id == pool_id,
            since_block_height,
            limit,
        ))
    }
}

fn filter_test_price_history<F>(
    history: &[LmsrPriceHistoryEntry],
    predicate: F,
    since_block_height: Option<u32>,
    limit: Option<i64>,
) -> Vec<LmsrPriceHistoryEntry>
where
    F: Fn(&LmsrPriceHistoryEntry) -> bool,
{
    let mut entries: Vec<_> = history
        .iter()
        .filter(|entry| predicate(entry))
        .filter(|entry| {
            since_block_height
                .map(|height| entry.block_height >= height)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    entries.sort_by(|a, b| {
        a.block_height
            .cmp(&b.block_height)
            .then_with(|| a.pool_id.cmp(&b.pool_id))
            .then_with(|| a.transition_txid.cmp(&b.transition_txid))
    });
    if let Some(limit) = limit.and_then(|value| usize::try_from(value).ok())
        && entries.len() > limit
    {
        entries.drain(0..entries.len() - limit);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lmsr_pool_ingest() -> LmsrPoolIngestInput {
        LmsrPoolIngestInput {
            pool_id: "11".repeat(32),
            market_id: "22".repeat(32),
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            fee_bps: 30,
            cosigner_pubkey: [0x04; 32],
            lmsr_table_root: [0x05; 32],
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            creation_txid: "aa".repeat(32),
            witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".to_string(),
            initial_reserve_outpoints: [
                format!("{}:0", "aa".repeat(32)),
                format!("{}:1", "aa".repeat(32)),
                format!("{}:2", "aa".repeat(32)),
            ],
            current_s_index: 4,
            reserve_outpoints: [
                format!("{}:0", "aa".repeat(32)),
                format!("{}:1", "aa".repeat(32)),
                format!("{}:2", "aa".repeat(32)),
            ],
            reserve_yes: 500,
            reserve_no: 400,
            reserve_collateral: 1_000,
            state_source: LmsrPoolStateSource::Announcement,
            last_transition_txid: None,
            lmsr_table_values: None,
            nostr_event_id: Some("evt-1".to_string()),
            nostr_event_json: Some(r#"{"id":"evt-1"}"#.to_string()),
        }
    }

    #[test]
    fn merge_lmsr_pool_ingest_preserves_canonical_state_for_later_announcements() {
        let mut existing = sample_lmsr_pool_ingest();
        let transition_txid = "bb".repeat(32);
        existing.current_s_index = 6;
        existing.reserve_outpoints = [
            format!("{}:0", "bb".repeat(32)),
            format!("{}:1", "bb".repeat(32)),
            format!("{}:2", "bb".repeat(32)),
        ];
        existing.reserve_yes = 450;
        existing.reserve_no = 430;
        existing.reserve_collateral = 1_020;
        existing.state_source = LmsrPoolStateSource::CanonicalScan;
        existing.last_transition_txid = Some(transition_txid.clone());
        existing.nostr_event_id = None;
        existing.nostr_event_json = None;

        let mut incoming = sample_lmsr_pool_ingest();
        incoming.market_id = "33".repeat(32);
        incoming.nostr_event_id = Some("evt-2".to_string());
        incoming.nostr_event_json = Some(r#"{"id":"evt-2"}"#.to_string());

        merge_lmsr_pool_ingest(&mut existing, &incoming);

        assert_eq!(existing.market_id, sample_lmsr_pool_ingest().market_id);
        assert_eq!(existing.current_s_index, 6);
        assert_eq!(existing.state_source, LmsrPoolStateSource::CanonicalScan);
        assert_eq!(
            existing.last_transition_txid.as_deref(),
            Some(transition_txid.as_str())
        );
        assert_eq!(existing.reserve_yes, 450);
        assert_eq!(existing.reserve_no, 430);
        assert_eq!(existing.reserve_collateral, 1_020);
        assert_eq!(existing.nostr_event_id.as_deref(), Some("evt-2"));
        assert_eq!(
            existing.nostr_event_json.as_deref(),
            Some(r#"{"id":"evt-2"}"#)
        );
    }

    #[test]
    fn merge_lmsr_pool_ingest_preserves_identity_for_conflicting_market_id() {
        let mut existing = sample_lmsr_pool_ingest();
        let original_market_id = existing.market_id.clone();
        let original_creation_txid = existing.creation_txid.clone();
        let original_witness_schema_version = existing.witness_schema_version.clone();

        let mut incoming = sample_lmsr_pool_ingest();
        incoming.market_id = "33".repeat(32);
        incoming.nostr_event_id = Some("evt-2".to_string());
        incoming.nostr_event_json = Some(r#"{"id":"evt-2"}"#.to_string());

        merge_lmsr_pool_ingest(&mut existing, &incoming);

        assert_eq!(existing.market_id, original_market_id);
        assert_eq!(existing.creation_txid, original_creation_txid);
        assert_eq!(
            existing.witness_schema_version,
            original_witness_schema_version
        );
        assert_eq!(existing.current_s_index, incoming.current_s_index);
        assert_eq!(existing.nostr_event_id.as_deref(), Some("evt-2"));
    }
}

// ---------------------------------------------------------------------------
// Shared contract / market / order helpers
// ---------------------------------------------------------------------------

/// Fixed contract params for compilation and ElementsEnv tests.
///
/// Oracle `[0xaa; 32]`, `collateral_per_token: 100_000`, `expiry_time: 1_000_000`.
pub fn test_contract_params() -> PredictionMarketParams {
    PredictionMarketParams {
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

pub fn test_contract_params_with_oracle_pubkey(
    oracle_public_key: [u8; 32],
) -> PredictionMarketParams {
    let mut params = test_contract_params();
    params.oracle_public_key = oracle_public_key;
    params
}

pub fn test_contract_params_with_defining_outpoints(
    oracle_public_key: [u8; 32],
    yes_defining_outpoint: OutPoint,
    no_defining_outpoint: OutPoint,
) -> PredictionMarketParams {
    let assets = compute_issuance_assets(
        yes_defining_outpoint,
        no_defining_outpoint,
        ContractHash::from_byte_array([0u8; 32]),
        false,
    );

    PredictionMarketParams {
        oracle_public_key,
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: assets.yes_token_asset,
        no_token_asset: assets.no_token_asset,
        yes_reissuance_token: assets.yes_reissuance_token,
        no_reissuance_token: assets.no_reissuance_token,
        collateral_per_token: 100_000,
        expiry_time: 1_000_000,
    }
}

pub fn test_issuance_entropy(
    yes_defining_outpoint: OutPoint,
    no_defining_outpoint: OutPoint,
    yes_blinding_nonce: [u8; 32],
    no_blinding_nonce: [u8; 32],
) -> crate::prediction_market::assembly::IssuanceEntropy {
    crate::prediction_market::assembly::IssuanceEntropy {
        yes_blinding_nonce,
        yes_entropy: AssetId::generate_asset_entropy(
            yes_defining_outpoint,
            ContractHash::from_byte_array([0u8; 32]),
        )
        .to_byte_array(),
        no_blinding_nonce,
        no_entropy: AssetId::generate_asset_entropy(
            no_defining_outpoint,
            ContractHash::from_byte_array([0u8; 32]),
        )
        .to_byte_array(),
    }
}

/// Parameterized contract params for discovery / node tests.
///
/// `collateral_per_token: 5000`, `expiry_time: 3_650_000`.
pub fn test_market_params(oracle_pubkey: [u8; 32]) -> PredictionMarketParams {
    PredictionMarketParams {
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

pub fn test_discovery_market_params(oracle_pubkey: [u8; 32], tag: u8) -> PredictionMarketParams {
    let yes_outpoint = OutPoint::new(Txid::from_byte_array([tag; 32]), 0);
    let no_outpoint = OutPoint::new(Txid::from_byte_array([tag.wrapping_add(1); 32]), 1);
    let assets = compute_issuance_assets(
        yes_outpoint,
        no_outpoint,
        ContractHash::from_byte_array([0u8; 32]),
        false,
    );

    PredictionMarketParams {
        oracle_public_key: oracle_pubkey,
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: assets.yes_token_asset,
        no_token_asset: assets.no_token_asset,
        yes_reissuance_token: assets.yes_reissuance_token,
        no_reissuance_token: assets.no_reissuance_token,
        collateral_per_token: 5000,
        expiry_time: 3_650_000,
    }
}

pub fn test_discovery_creation_tx(params: &PredictionMarketParams, tag: u8) -> Transaction {
    let contract = CompiledPredictionMarket::new(*params).unwrap();
    let yes_abf = [tag.wrapping_add(0x10); 32];
    let yes_vbf = [tag.wrapping_add(0x20); 32];
    let no_abf = [tag.wrapping_add(0x30); 32];
    let no_vbf = [tag.wrapping_add(0x40); 32];
    let inputs = vec![
        TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([tag; 32]), 0),
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
            previous_output: OutPoint::new(Txid::from_byte_array([tag.wrapping_add(1); 32]), 1),
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
    ];

    Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![
            confidential_dormant_creation_txout(
                &params.yes_reissuance_token,
                &yes_abf,
                &yes_vbf,
                &contract.script_pubkey(MarketSlot::DormantYesRt),
            ),
            confidential_dormant_creation_txout(
                &params.no_reissuance_token,
                &no_abf,
                &no_vbf,
                &contract.script_pubkey(MarketSlot::DormantNoRt),
            ),
        ],
    }
}

pub fn test_discovery_anchor(tag: u8, creation_txid: Txid) -> PredictionMarketAnchor {
    PredictionMarketAnchor::from_openings(
        creation_txid,
        [tag.wrapping_add(0x10); 32],
        [tag.wrapping_add(0x20); 32],
        [tag.wrapping_add(0x30); 32],
        [tag.wrapping_add(0x40); 32],
    )
}

pub fn test_market_announcement(
    oracle_pubkey: [u8; 32],
    tag: u8,
) -> (ContractAnnouncement, PredictionMarketParams) {
    let params = test_discovery_market_params(oracle_pubkey, tag);
    let tx = test_discovery_creation_tx(&params, tag);
    let anchor = test_discovery_anchor(tag, tx.txid());

    (
        ContractAnnouncement {
            version: CONTRACT_ANNOUNCEMENT_VERSION,
            contract_params: params,
            metadata: test_metadata(),
            anchor,
            creation_tx_hex: hex::encode(simplicityhl::elements::encode::serialize(&tx)),
        },
        params,
    )
}

pub fn test_lmsr_table_values() -> Vec<u64> {
    vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175]
}

pub fn test_lmsr_pool_params(tag: u8) -> LmsrPoolParams {
    let table_values = test_lmsr_table_values();
    LmsrPoolParams {
        yes_asset_id: [tag.wrapping_add(0x01); 32],
        no_asset_id: [tag.wrapping_add(0x02); 32],
        collateral_asset_id: [tag.wrapping_add(0x03); 32],
        lmsr_table_root: crate::lmsr_table_root(&table_values).expect("test lmsr table root"),
        table_depth: 3,
        q_step_lots: 10,
        s_bias: 4,
        s_max_index: 7,
        half_payout_sats: 100,
        fee_bps: 30,
        min_r_yes: 7,
        min_r_no: 8,
        min_r_collateral: 9,
        cosigner_pubkey: [tag.wrapping_add(0x04); 32],
    }
}

fn test_txid(tag: u8, offset: u8) -> Txid {
    let mut bytes = [0u8; 32];
    for (idx, byte) in bytes.iter_mut().enumerate() {
        *byte = tag
            .wrapping_add(offset)
            .wrapping_add((idx as u8).wrapping_mul(7));
    }
    Txid::from_byte_array(bytes)
}

pub fn test_lmsr_pool_snapshot(network: Network, tag: u8) -> (LmsrPoolSnapshot, Vec<u64>) {
    let table_values = test_lmsr_table_values();
    let params = test_lmsr_pool_params(tag);
    let creation_txid = test_txid(tag, 0x80);
    let creation_txid_bytes =
        txid_to_canonical_bytes(&creation_txid).expect("canonical test creation txid");
    let initial_reserve_outpoints = [
        LmsrInitialOutpoint {
            txid: creation_txid_bytes,
            vout: 0,
        },
        LmsrInitialOutpoint {
            txid: creation_txid_bytes,
            vout: 1,
        },
        LmsrInitialOutpoint {
            txid: creation_txid_bytes,
            vout: 2,
        },
    ];
    let locator = LmsrPoolLocator {
        market_id: derive_lmsr_market_id(params),
        pool_id: derive_lmsr_pool_id(
            network,
            params,
            creation_txid_bytes,
            initial_reserve_outpoints,
        )
        .expect("derive test lmsr pool id"),
        params,
        creation_txid,
        initial_reserve_outpoints,
        hinted_s_index: 4,
        witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".to_string(),
    };
    let reserve_txid = test_txid(tag, 0x81);
    let last_transition_txid = test_txid(tag, 0x82);
    (
        LmsrPoolSnapshot {
            locator,
            current_s_index: 4,
            reserves: PoolReserves {
                r_yes: 500,
                r_no: 400,
                r_lbtc: 1_000,
            },
            current_reserve_outpoints: [
                OutPoint::new(reserve_txid, 0),
                OutPoint::new(reserve_txid, 1),
                OutPoint::new(reserve_txid, 2),
            ],
            last_transition_txid: Some(last_transition_txid),
        },
        table_values,
    )
}

pub fn test_lmsr_pool_announcement(network: Network, tag: u8) -> PoolAnnouncement {
    let (snapshot, table_values) = test_lmsr_pool_snapshot(network, tag);
    build_pool_announcement_from_snapshot(&snapshot, table_values)
        .expect("build canonical test lmsr pool announcement")
}

pub fn test_lmsr_pool_ingest_input(network: Network, tag: u8) -> LmsrPoolIngestInput {
    let (snapshot, table_values) = test_lmsr_pool_snapshot(network, tag);
    let announcement = build_pool_announcement_from_snapshot(&snapshot, table_values)
        .expect("build canonical test lmsr pool announcement");
    LmsrPoolIngestInput {
        pool_id: announcement.lmsr_pool_id.clone(),
        market_id: announcement.market_id.clone(),
        yes_asset_id: announcement.params.yes_asset_id,
        no_asset_id: announcement.params.no_asset_id,
        collateral_asset_id: announcement.params.lbtc_asset_id,
        fee_bps: announcement.params.fee_bps,
        cosigner_pubkey: announcement.params.cosigner_pubkey,
        lmsr_table_root: hex::decode(&announcement.lmsr_table_root)
            .expect("test lmsr table root hex")
            .try_into()
            .expect("test lmsr table root length"),
        table_depth: announcement.table_depth,
        q_step_lots: announcement.q_step_lots,
        s_bias: announcement.s_bias,
        s_max_index: announcement.s_max_index,
        half_payout_sats: announcement.half_payout_sats,
        min_r_yes: announcement.params.min_r_yes,
        min_r_no: announcement.params.min_r_no,
        min_r_collateral: announcement.params.min_r_collateral,
        creation_txid: announcement.creation_txid.clone(),
        witness_schema_version: announcement.witness_schema_version.clone(),
        initial_reserve_outpoints: announcement
            .initial_reserve_outpoints
            .clone()
            .try_into()
            .expect("test announcement has 3 initial reserve outpoints"),
        current_s_index: announcement.current_s_index,
        reserve_outpoints: snapshot
            .current_reserve_outpoints
            .map(|outpoint| outpoint.to_string()),
        reserve_yes: announcement.reserves.r_yes,
        reserve_no: announcement.reserves.r_no,
        reserve_collateral: announcement.reserves.r_lbtc,
        state_source: LmsrPoolStateSource::Announcement,
        last_transition_txid: snapshot.last_transition_txid.map(|txid| txid.to_string()),
        lmsr_table_values: announcement.lmsr_table_values.clone(),
        nostr_event_id: None,
        nostr_event_json: None,
    }
}

/// Standard test metadata for discovery / node tests.
pub fn test_metadata() -> ContractMetadata {
    ContractMetadata {
        question: "Will BTC close above $120k by Dec 2026?".to_string(),
        description: "Resolved using median close basket.".to_string(),
        category: "Bitcoin".to_string(),
        resolution_source: "Exchange close basket".to_string(),
    }
}

/// Extract a 32-byte x-only public key from `nostr_sdk::Keys`.
pub fn oracle_pubkey_from_keys(keys: &Keys) -> [u8; 32] {
    let h = keys.public_key().to_hex();
    let b = hex::decode(&h).unwrap();
    <[u8; 32]>::try_from(b.as_slice()).unwrap()
}

/// Build a test order announcement for a given market ID.
pub fn test_order_announcement(market_id: &str) -> OrderAnnouncement {
    let (params, _) = MakerOrderParams::new(
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
    OrderAnnouncement {
        version: 1,
        params,
        market_id: market_id.to_string(),
        maker_base_pubkey: hex::encode([0xaa; 32]),
        order_nonce: hex::encode([0x11; 32]),
        covenant_address: "tex1qtest".to_string(),
        offered_amount: 100,
        direction_label: "sell-yes".to_string(),
    }
}
