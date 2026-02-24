use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;

use super::super::contract::CompiledAmmPool;

/// Parameters for an LP withdraw transaction (burn LP tokens).
pub struct LpWithdrawParams {
    /// Pool YES reserve UTXO (covenant input 0).
    pub pool_yes_utxo: UnblindedUtxo,
    /// Pool NO reserve UTXO (covenant input 1).
    pub pool_no_utxo: UnblindedUtxo,
    /// Pool L-BTC reserve UTXO (covenant input 2).
    pub pool_lbtc_utxo: UnblindedUtxo,
    /// Pool LP reissuance token UTXO (covenant input 3).
    pub pool_lp_rt_utxo: UnblindedUtxo,
    /// Current issued LP count.
    pub issued_lp: u64,
    /// LP token UTXO(s) to burn.
    pub lp_token_utxos: Vec<UnblindedUtxo>,
    /// Number of LP tokens to burn.
    pub lp_burn_amount: u64,
    /// New YES reserve after withdrawal.
    pub new_r_yes: u64,
    /// New NO reserve after withdrawal.
    pub new_r_no: u64,
    /// New L-BTC reserve after withdrawal.
    pub new_r_lbtc: u64,
    /// Destination for withdrawn reserves.
    pub withdraw_destination: Script,
    /// Fee UTXO.
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount.
    pub fee_amount: u64,
    /// Fee asset ID.
    pub fee_asset_id: [u8; 32],
}

/// Build an LP withdraw PSET.
///
/// Outputs 0-3 at NEW address: `contract.script_pubkey(issued_lp - lp_burn_amount)`.
/// Output 4: LP tokens to empty script (burned per D1).
/// Outputs 5-7: withdrawn YES, NO, L-BTC to LP's address.
pub fn build_lp_withdraw_pset(
    contract: &CompiledAmmPool,
    params: &LpWithdrawParams,
) -> Result<PartiallySignedTransaction> {
    use crate::pset::{
        add_pset_input, add_pset_output, burn_txout, explicit_txout, fee_txout, new_pset,
    };

    if params.lp_token_utxos.is_empty() {
        return Err(Error::AmmPool("LP token UTXOs must be non-empty".into()));
    }
    if params.lp_burn_amount == 0 {
        return Err(Error::AmmPool("lp_burn_amount must be non-zero".into()));
    }
    if params.lp_burn_amount >= params.issued_lp {
        return Err(Error::AmmPool(
            "cannot burn all LP tokens (minimum 1 must remain)".into(),
        ));
    }
    if params.new_r_yes == 0 || params.new_r_no == 0 || params.new_r_lbtc == 0 {
        return Err(Error::AmmPool("new reserves must be non-zero".into()));
    }

    let new_issued_lp = params
        .issued_lp
        .checked_sub(params.lp_burn_amount)
        .ok_or_else(|| Error::AmmPool("issued_lp underflow".into()))?;
    let new_covenant_spk = contract.script_pubkey(new_issued_lp);

    let mut pset = new_pset();

    // Inputs 0-3: pool covenant UTXOs
    add_pset_input(&mut pset, &params.pool_yes_utxo);
    add_pset_input(&mut pset, &params.pool_no_utxo);
    add_pset_input(&mut pset, &params.pool_lbtc_utxo);
    add_pset_input(&mut pset, &params.pool_lp_rt_utxo);

    // Input 4+: LP token UTXO(s) to burn
    for utxo in &params.lp_token_utxos {
        add_pset_input(&mut pset, utxo);
    }
    add_pset_input(&mut pset, &params.fee_utxo);

    // Outputs 0-3: reserves at NEW covenant address
    let yes_out = explicit_txout(
        &contract.params().yes_asset_id,
        params.new_r_yes,
        &new_covenant_spk,
    );
    add_pset_output(&mut pset, yes_out);

    let no_out = explicit_txout(
        &contract.params().no_asset_id,
        params.new_r_no,
        &new_covenant_spk,
    );
    add_pset_output(&mut pset, no_out);

    let lbtc_out = explicit_txout(
        &contract.params().lbtc_asset_id,
        params.new_r_lbtc,
        &new_covenant_spk,
    );
    add_pset_output(&mut pset, lbtc_out);

    // RT passthrough to new address
    let rt_out = explicit_txout(
        &contract.params().lp_reissuance_token_id,
        1,
        &new_covenant_spk,
    );
    add_pset_output(&mut pset, rt_out);

    // Output 4: LP tokens burned → OP_RETURN
    let burn_out = burn_txout(&contract.params().lp_asset_id, params.lp_burn_amount);
    add_pset_output(&mut pset, burn_out);

    // Outputs 5-7: withdrawn reserves to LP's address
    let withdrawn_yes = params.pool_yes_utxo.value.saturating_sub(params.new_r_yes);
    if withdrawn_yes > 0 {
        let yes_withdraw = explicit_txout(
            &contract.params().yes_asset_id,
            withdrawn_yes,
            &params.withdraw_destination,
        );
        add_pset_output(&mut pset, yes_withdraw);
    }

    let withdrawn_no = params.pool_no_utxo.value.saturating_sub(params.new_r_no);
    if withdrawn_no > 0 {
        let no_withdraw = explicit_txout(
            &contract.params().no_asset_id,
            withdrawn_no,
            &params.withdraw_destination,
        );
        add_pset_output(&mut pset, no_withdraw);
    }

    let withdrawn_lbtc = params
        .pool_lbtc_utxo
        .value
        .saturating_sub(params.new_r_lbtc);
    if withdrawn_lbtc > 0 {
        let lbtc_withdraw = explicit_txout(
            &contract.params().lbtc_asset_id,
            withdrawn_lbtc,
            &params.withdraw_destination,
        );
        add_pset_output(&mut pset, lbtc_withdraw);
    }

    // LP token change (if LP utxos have more than burn amount)
    let lp_total: u64 = params.lp_token_utxos.iter().map(|u| u.value).sum();
    let lp_change = lp_total.saturating_sub(params.lp_burn_amount);
    if lp_change > 0 {
        let lp_change_out = explicit_txout(
            &contract.params().lp_asset_id,
            lp_change,
            &params.withdraw_destination,
        );
        add_pset_output(&mut pset, lp_change_out);
    }

    // Fee change
    let fee_total = params.fee_utxo.value;
    if fee_total < params.fee_amount {
        return Err(Error::InsufficientFee);
    }
    let fee_change = fee_total - params.fee_amount;
    if fee_change > 0 {
        let change_out = explicit_txout(
            &params.fee_asset_id,
            fee_change,
            &params.withdraw_destination,
        );
        add_pset_output(&mut pset, change_out);
    }

    let fee_out = fee_txout(&params.fee_asset_id, params.fee_amount);
    add_pset_output(&mut pset, fee_out);

    Ok(pset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amm_pool::params::AmmPoolParams;
    use crate::taproot::NUMS_KEY_BYTES;
    use simplicityhl::elements::hashes::Hash;
    use simplicityhl::elements::{OutPoint, Txid};

    fn test_params() -> AmmPoolParams {
        AmmPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    fn test_utxo(asset_id: [u8; 32], value: u64, vout: u32) -> UnblindedUtxo {
        use crate::pset::explicit_txout;
        UnblindedUtxo {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            txout: explicit_txout(&asset_id, value, &Script::new()),
            asset_id,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    #[test]
    fn withdraw_pset_output_layout() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let withdraw = LpWithdrawParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            lp_token_utxos: vec![test_utxo(params.lp_asset_id, 10, 4)],
            lp_burn_amount: 10,
            new_r_yes: 9_000,
            new_r_no: 9_000,
            new_r_lbtc: 45_000,
            withdraw_destination: Script::new(),
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        let pset = build_lp_withdraw_pset(&contract, &withdraw).unwrap();
        // 6 inputs: 4 pool + 1 LP + 1 fee
        assert_eq!(pset.n_inputs(), 6);
        // 9 outputs: 4 reserves + burn + 3 withdrawals + fee
        assert_eq!(pset.n_outputs(), 9);
    }

    #[test]
    fn withdraw_burn_output_uses_empty_script() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let withdraw = LpWithdrawParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            lp_token_utxos: vec![test_utxo(params.lp_asset_id, 10, 4)],
            lp_burn_amount: 10,
            new_r_yes: 9_000,
            new_r_no: 9_000,
            new_r_lbtc: 45_000,
            withdraw_destination: Script::new(),
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        let pset = build_lp_withdraw_pset(&contract, &withdraw).unwrap();
        // Output 4 is the burn output — must have empty script (D1)
        assert!(pset.outputs()[4].script_pubkey.is_empty());
    }

    #[test]
    fn withdraw_rejects_empty_lp_utxos() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let withdraw = LpWithdrawParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            lp_token_utxos: vec![],
            lp_burn_amount: 10,
            new_r_yes: 9_000,
            new_r_no: 9_000,
            new_r_lbtc: 45_000,
            withdraw_destination: Script::new(),
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_lp_withdraw_pset(&contract, &withdraw).is_err());
    }

    #[test]
    fn withdraw_rejects_burning_all_lp() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let withdraw = LpWithdrawParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            lp_token_utxos: vec![test_utxo(params.lp_asset_id, 100, 4)],
            lp_burn_amount: 100,
            new_r_yes: 0,
            new_r_no: 0,
            new_r_lbtc: 0,
            withdraw_destination: Script::new(),
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_lp_withdraw_pset(&contract, &withdraw).is_err());
    }
}
