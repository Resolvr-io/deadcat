use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::secp256k1_zkp::Tweak;
use simplicityhl::elements::{LockTime, Script};

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, covenant_spk, explicit_txout, fee_txout,
    new_pset, reissuance_token_output,
};

/// Parameters for constructing an initial issuance PSET (state 0 → 1).
///
/// First covenant-validated issuance: transitions from Dormant to Unresolved.
/// Unlike subsequent issuance, there is no old collateral from the covenant —
/// required collateral = collateral_for_pairs(pairs).
pub struct InitialIssuanceParams {
    pub yes_reissuance_utxo: UnblindedUtxo,
    pub no_reissuance_utxo: UnblindedUtxo,
    pub collateral_utxo: UnblindedUtxo,
    pub fee_utxo: UnblindedUtxo,
    pub pairs: u64,
    pub fee_amount: u64,
    pub yes_token_destination: Script,
    pub no_token_destination: Script,
    pub collateral_change_destination: Option<Script>,
    pub fee_change_destination: Option<Script>,
    pub yes_issuance_blinding_nonce: [u8; 32],
    pub yes_issuance_asset_entropy: [u8; 32],
    pub no_issuance_blinding_nonce: [u8; 32],
    pub no_issuance_asset_entropy: [u8; 32],
    pub lock_time: u32,
}

/// Build the initial issuance PSET (state 0 → 1).
///
/// Input 0: YES reissuance token (from Dormant covenant)
/// Input 1: NO reissuance token (from Dormant covenant)
/// Input 2: collateral (external — not from covenant)
/// Input 3: fee
///
/// Output 0: YES reissuance token → Unresolved covenant
/// Output 1: NO reissuance token → Unresolved covenant
/// Output 2: collateral → Unresolved covenant
/// Output 3: YES tokens → creator
/// Output 4: NO tokens → creator
/// Output 5: fee
/// Output 6+: optional change
pub fn build_initial_issuance_pset(
    contract: &CompiledContract,
    params: &InitialIssuanceParams,
) -> Result<PartiallySignedTransaction> {
    let cpt = contract.params().collateral_per_token;
    let required_collateral = params
        .pairs
        .checked_mul(2)
        .and_then(|v| v.checked_mul(cpt))
        .ok_or(Error::CollateralOverflow)?;

    if params.collateral_utxo.value < required_collateral {
        return Err(Error::InsufficientCollateral);
    }

    let mut pset = new_pset();
    let unresolved_spk = covenant_spk(contract, MarketState::Unresolved);

    // Input 0: YES reissuance token (from Dormant)
    add_pset_input(&mut pset, &params.yes_reissuance_utxo);
    // Input 1: NO reissuance token (from Dormant)
    add_pset_input(&mut pset, &params.no_reissuance_utxo);
    // Input 2: collateral (external)
    add_pset_input(&mut pset, &params.collateral_utxo);
    // Input 3: fee
    add_pset_input(&mut pset, &params.fee_utxo);

    // Mark inputs 0 and 1 as reissuance
    if let Some(input) = pset.inputs_mut().get_mut(0) {
        input.issuance_value_amount = Some(params.pairs);
        input.issuance_blinding_nonce = Some(
            Tweak::from_slice(&params.yes_issuance_blinding_nonce)
                .expect("valid yes issuance blinding nonce"),
        );
        input.issuance_asset_entropy = Some(params.yes_issuance_asset_entropy);
    }
    if let Some(input) = pset.inputs_mut().get_mut(1) {
        input.issuance_value_amount = Some(params.pairs);
        input.issuance_blinding_nonce = Some(
            Tweak::from_slice(&params.no_issuance_blinding_nonce)
                .expect("valid no issuance blinding nonce"),
        );
        input.issuance_asset_entropy = Some(params.no_issuance_asset_entropy);
    }

    // Output 0: YES reissuance token → Unresolved
    add_pset_output(&mut pset, reissuance_token_output(&unresolved_spk));
    // Output 1: NO reissuance token → Unresolved
    add_pset_output(&mut pset, reissuance_token_output(&unresolved_spk));
    // Output 2: collateral → Unresolved
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().collateral_asset_id,
            required_collateral,
            &unresolved_spk,
        ),
    );
    // Output 3: YES tokens → creator
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().yes_token_asset,
            params.pairs,
            &params.yes_token_destination,
        ),
    );
    // Output 4: NO tokens → creator
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().no_token_asset,
            params.pairs,
            &params.no_token_destination,
        ),
    );
    // Output 5: fee
    add_pset_output(
        &mut pset,
        fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
    );

    // Collateral change
    let change = params.collateral_utxo.value - required_collateral;
    if change > 0
        && let Some(ref change_spk) = params.collateral_change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(&contract.params().collateral_asset_id, change, change_spk),
        );
    }

    // Fee change
    let fee_change = params.fee_utxo.value.saturating_sub(params.fee_amount);
    if fee_change > 0
        && let Some(ref change_spk) = params.fee_change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(
                &contract.params().collateral_asset_id,
                fee_change,
                change_spk,
            ),
        );
    }

    pset.global.tx_data.fallback_locktime = Some(LockTime::from_consensus(params.lock_time));

    Ok(pset)
}
