use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::secp256k1_zkp::Tweak;

use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;

use super::super::contract::CompiledAmmPool;

/// Parameters for an LP deposit transaction (reissuance of LP tokens).
pub struct LpDepositParams {
    /// Pool YES reserve UTXO (covenant input 0).
    pub pool_yes_utxo: UnblindedUtxo,
    /// Pool NO reserve UTXO (covenant input 1).
    pub pool_no_utxo: UnblindedUtxo,
    /// Pool L-BTC reserve UTXO (covenant input 2).
    pub pool_lbtc_utxo: UnblindedUtxo,
    /// Pool LP reissuance token UTXO (covenant input 3, triggers issuance).
    pub pool_lp_rt_utxo: UnblindedUtxo,
    /// Current issued LP count.
    pub issued_lp: u64,
    /// Depositor's funding UTXOs.
    pub deposit_utxos: Vec<UnblindedUtxo>,
    /// New YES reserve after deposit.
    pub new_r_yes: u64,
    /// New NO reserve after deposit.
    pub new_r_no: u64,
    /// New L-BTC reserve after deposit.
    pub new_r_lbtc: u64,
    /// Number of LP tokens to mint.
    pub lp_mint_amount: u64,
    /// Destination for minted LP tokens.
    pub lp_token_destination: Script,
    /// Change destination for excess deposit inputs.
    pub change_destination: Option<Script>,
    /// Fee UTXO.
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount.
    pub fee_amount: u64,
    /// Fee asset ID.
    pub fee_asset_id: [u8; 32],
    /// Asset blinding factor of the LP reissuance token UTXO being spent.
    pub lp_issuance_blinding_nonce: [u8; 32],
    /// Asset entropy for the LP token (from the original LP issuance transaction).
    pub lp_issuance_asset_entropy: [u8; 32],
}

/// Build an LP deposit PSET.
///
/// Input 3 triggers reissuance (`issuance_value_amount` set).
/// Outputs 0-3 at NEW address: `contract.script_pubkey(issued_lp + lp_mint_amount)`.
/// Output 4: minted LP tokens (issuance output).
pub fn build_lp_deposit_pset(
    contract: &CompiledAmmPool,
    params: &LpDepositParams,
) -> Result<PartiallySignedTransaction> {
    use crate::pset::{
        add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
        reissuance_token_output,
    };

    if params.lp_mint_amount == 0 {
        return Err(Error::AmmPool("lp_mint_amount must be non-zero".into()));
    }
    if params.new_r_yes == 0 || params.new_r_no == 0 || params.new_r_lbtc == 0 {
        return Err(Error::AmmPool("new reserves must be non-zero".into()));
    }

    let new_issued_lp = params
        .issued_lp
        .checked_add(params.lp_mint_amount)
        .ok_or_else(|| Error::AmmPool("issued_lp overflow".into()))?;
    let new_covenant_spk = contract.script_pubkey(new_issued_lp);

    let mut pset = new_pset();

    // Inputs 0-3: pool covenant UTXOs
    add_pset_input(&mut pset, &params.pool_yes_utxo);
    add_pset_input(&mut pset, &params.pool_no_utxo);
    add_pset_input(&mut pset, &params.pool_lbtc_utxo);
    add_pset_input(&mut pset, &params.pool_lp_rt_utxo);

    // Mark input 3 for reissuance (LP token minting).
    // Note: blinded_issuance is set during the blinding step in sdk.rs,
    // not here, matching the working issuance pattern.
    if let Some(input) = pset.inputs_mut().get_mut(3) {
        input.issuance_value_amount = Some(params.lp_mint_amount);
        input.issuance_blinding_nonce = Some(
            Tweak::from_slice(&params.lp_issuance_blinding_nonce)
                .expect("valid LP issuance blinding nonce"),
        );
        input.issuance_asset_entropy = Some(params.lp_issuance_asset_entropy);
    }

    // Inputs 4+: depositor funding UTXOs
    for utxo in &params.deposit_utxos {
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

    // RT cycles back to pool at new address (Null placeholder, filled by blinder).
    add_pset_output(&mut pset, reissuance_token_output(&new_covenant_spk));

    // Output 4: minted LP tokens → depositor
    let lp_out = explicit_txout(
        &contract.params().lp_asset_id,
        params.lp_mint_amount,
        &params.lp_token_destination,
    );
    add_pset_output(&mut pset, lp_out);

    // Change outputs for depositor — one per asset type.
    // Sum deposit UTXOs per asset, subtract what was deposited, emit single change output.
    let deposited_yes = params.new_r_yes.saturating_sub(params.pool_yes_utxo.value);
    let deposited_no = params.new_r_no.saturating_sub(params.pool_no_utxo.value);
    let deposited_lbtc = params
        .new_r_lbtc
        .saturating_sub(params.pool_lbtc_utxo.value);

    let mut total_yes: u64 = 0;
    let mut total_no: u64 = 0;
    let mut total_lbtc: u64 = 0;
    for utxo in &params.deposit_utxos {
        if utxo.asset_id == contract.params().yes_asset_id {
            total_yes += utxo.value;
        } else if utxo.asset_id == contract.params().no_asset_id {
            total_no += utxo.value;
        } else if utxo.asset_id == contract.params().lbtc_asset_id {
            total_lbtc += utxo.value;
        }
    }

    for (total, deposited, asset_id) in [
        (total_yes, deposited_yes, contract.params().yes_asset_id),
        (total_no, deposited_no, contract.params().no_asset_id),
        (total_lbtc, deposited_lbtc, contract.params().lbtc_asset_id),
    ] {
        if total < deposited {
            return Err(Error::AmmPool(format!(
                "insufficient deposit UTXOs: have {total}, need {deposited}"
            )));
        }
        let change = total - deposited;
        if change > 0 {
            if let Some(ref change_dest) = params.change_destination {
                let change_out = explicit_txout(&asset_id, change, change_dest);
                add_pset_output(&mut pset, change_out);
            } else {
                return Err(Error::MissingChangeDestination);
            }
        }
    }

    // Fee change
    let fee_total = params.fee_utxo.value;
    if fee_total < params.fee_amount {
        return Err(Error::InsufficientFee);
    }
    let fee_change = fee_total - params.fee_amount;
    if fee_change > 0 {
        if let Some(ref change_dest) = params.change_destination {
            let change_out = explicit_txout(&params.fee_asset_id, fee_change, change_dest);
            add_pset_output(&mut pset, change_out);
        } else {
            return Err(Error::MissingChangeDestination);
        }
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
    fn deposit_pset_output_layout() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let deposit = LpDepositParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            deposit_utxos: vec![
                test_utxo(params.yes_asset_id, 1000, 4),
                test_utxo(params.no_asset_id, 1000, 5),
                test_utxo(params.lbtc_asset_id, 5000, 6),
            ],
            new_r_yes: 11_000,
            new_r_no: 11_000,
            new_r_lbtc: 55_000,
            lp_mint_amount: 10,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 7),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
            lp_issuance_blinding_nonce: [0u8; 32],
            lp_issuance_asset_entropy: [0xaa; 32],
        };
        let pset = build_lp_deposit_pset(&contract, &deposit).unwrap();
        // 8 inputs: 4 pool + 3 deposit + 1 fee
        assert_eq!(pset.n_inputs(), 8);
        // 6 outputs: 4 reserves + LP tokens + fee
        assert_eq!(pset.n_outputs(), 6);

        // RT input (index 3) has reissuance fields set
        let rt_input = &pset.inputs()[3];
        assert_eq!(rt_input.issuance_value_amount, Some(10));
        assert!(rt_input.issuance_blinding_nonce.is_some());
        assert_eq!(rt_input.issuance_asset_entropy, Some([0xaa; 32]));
    }

    #[test]
    fn deposit_rejects_zero_mint() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let deposit = LpDepositParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            deposit_utxos: vec![],
            new_r_yes: 10_000,
            new_r_no: 10_000,
            new_r_lbtc: 50_000,
            lp_mint_amount: 0,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
            lp_issuance_blinding_nonce: [0u8; 32],
            lp_issuance_asset_entropy: [0xaa; 32],
        };
        assert!(build_lp_deposit_pset(&contract, &deposit).is_err());
    }

    #[test]
    fn deposit_rejects_zero_new_reserves() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let deposit = LpDepositParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            deposit_utxos: vec![],
            new_r_yes: 0,
            new_r_no: 11_000,
            new_r_lbtc: 55_000,
            lp_mint_amount: 10,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
            lp_issuance_blinding_nonce: [0u8; 32],
            lp_issuance_asset_entropy: [0xaa; 32],
        };
        assert!(build_lp_deposit_pset(&contract, &deposit).is_err());
    }
}
