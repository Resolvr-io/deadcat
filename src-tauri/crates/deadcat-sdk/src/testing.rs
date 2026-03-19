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
use crate::discovery::OrderAnnouncement;
use crate::discovery::store_trait::{
    DiscoveryStore, LmsrPoolIngestInput, LmsrPoolStateUpdateInput,
    PredictionMarketCandidateIngestInput,
};
use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::taproot::NUMS_KEY_BYTES;

/// Minimal in-memory store implementing `DiscoveryStore` for integration tests.
///
/// Deduplicates markets by `market_id` and stores orders as-is.
#[derive(Debug, Default)]
pub struct TestStore {
    pub markets: Vec<PredictionMarketCandidateIngestInput>,
    pub orders: Vec<(MakerOrderParams, Option<String>)>,
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

    fn ingest_lmsr_pool(
        &mut self,
        _input: &LmsrPoolIngestInput,
    ) -> std::result::Result<(), String> {
        Ok(())
    }

    fn upsert_lmsr_pool_state(
        &mut self,
        _input: &LmsrPoolStateUpdateInput,
    ) -> std::result::Result<(), String> {
        Ok(())
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
