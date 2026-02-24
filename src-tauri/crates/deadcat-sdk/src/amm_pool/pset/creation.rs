use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::Script;

use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;

use super::super::contract::CompiledAmmPool;

/// Parameters for creating a new AMM pool (bootstrap transaction).
///
/// This is NOT covenant-validated — the SDK must validate before broadcast.
pub struct PoolCreationParams {
    /// YES token UTXOs from the creator's wallet.
    pub yes_utxos: Vec<UnblindedUtxo>,
    /// NO token UTXOs from the creator's wallet.
    pub no_utxos: Vec<UnblindedUtxo>,
    /// L-BTC UTXOs from the creator's wallet.
    pub lbtc_utxos: Vec<UnblindedUtxo>,
    /// LP reissuance token UTXO.
    pub lp_rt_utxo: UnblindedUtxo,
    /// Initial YES reserve amount.
    pub initial_r_yes: u64,
    /// Initial NO reserve amount.
    pub initial_r_no: u64,
    /// Initial L-BTC reserve amount.
    pub initial_r_lbtc: u64,
    /// Initial issued LP count (= LP tokens minted to creator).
    pub initial_issued_lp: u64,
    /// Destination for LP tokens.
    pub lp_token_destination: Script,
    /// Change destination for excess inputs.
    pub change_destination: Option<Script>,
    /// Fee UTXO.
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount in sats.
    pub fee_amount: u64,
    /// Fee asset ID.
    pub fee_asset_id: [u8; 32],
}

/// Build the pool creation PSET.
///
/// Outputs 0-3: reserves + RT at covenant address for `initial_issued_lp`.
/// Output 4: LP tokens to creator.
/// Outputs 5+: change, fee.
pub fn build_pool_creation_pset(
    contract: &CompiledAmmPool,
    params: &PoolCreationParams,
) -> Result<PartiallySignedTransaction> {
    use crate::pset::{add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset};

    if params.yes_utxos.is_empty() || params.no_utxos.is_empty() || params.lbtc_utxos.is_empty() {
        return Err(Error::AmmPool("funding UTXO vectors must be non-empty".into()));
    }
    if params.initial_r_yes == 0 || params.initial_r_no == 0 || params.initial_r_lbtc == 0 {
        return Err(Error::AmmPool("initial reserves must be non-zero".into()));
    }
    if params.initial_issued_lp == 0 {
        return Err(Error::ZeroIssuedLp);
    }

    let covenant_spk = contract.script_pubkey(params.initial_issued_lp);
    let mut pset = new_pset();

    // Add inputs: YES, NO, LBTC, LP RT, fee
    for utxo in &params.yes_utxos {
        add_pset_input(&mut pset, utxo);
    }
    for utxo in &params.no_utxos {
        add_pset_input(&mut pset, utxo);
    }
    for utxo in &params.lbtc_utxos {
        add_pset_input(&mut pset, utxo);
    }
    add_pset_input(&mut pset, &params.lp_rt_utxo);
    add_pset_input(&mut pset, &params.fee_utxo);

    // Output 0: YES reserve → covenant
    let yes_out = explicit_txout(
        &params.yes_utxos[0].asset_id,
        params.initial_r_yes,
        &covenant_spk,
    );
    add_pset_output(&mut pset, yes_out);

    // Output 1: NO reserve → covenant
    let no_out = explicit_txout(
        &params.no_utxos[0].asset_id,
        params.initial_r_no,
        &covenant_spk,
    );
    add_pset_output(&mut pset, no_out);

    // Output 2: L-BTC reserve → covenant
    let lbtc_out = explicit_txout(
        &params.lbtc_utxos[0].asset_id,
        params.initial_r_lbtc,
        &covenant_spk,
    );
    add_pset_output(&mut pset, lbtc_out);

    // Output 3: LP reissuance token → covenant (explicit, amount = 1)
    let rt_out = explicit_txout(
        &params.lp_rt_utxo.asset_id,
        1,
        &covenant_spk,
    );
    add_pset_output(&mut pset, rt_out);

    // Output 4: LP tokens → creator
    let lp_out = explicit_txout(
        &contract.params().lp_asset_id,
        params.initial_issued_lp,
        &params.lp_token_destination,
    );
    add_pset_output(&mut pset, lp_out);

    // Output 5+: change outputs
    // Compute excess for each asset type and add change if needed
    let yes_total: u64 = params.yes_utxos.iter().map(|u| u.value).sum();
    if yes_total < params.initial_r_yes {
        return Err(Error::AmmPool(format!(
            "insufficient YES UTXOs: have {yes_total}, need {}",
            params.initial_r_yes
        )));
    }
    let yes_change = yes_total - params.initial_r_yes;
    if yes_change > 0 {
        if let Some(ref change_dest) = params.change_destination {
            let change_out = explicit_txout(
                &params.yes_utxos[0].asset_id,
                yes_change,
                change_dest,
            );
            add_pset_output(&mut pset, change_out);
        } else {
            return Err(Error::MissingChangeDestination);
        }
    }

    let no_total: u64 = params.no_utxos.iter().map(|u| u.value).sum();
    if no_total < params.initial_r_no {
        return Err(Error::AmmPool(format!(
            "insufficient NO UTXOs: have {no_total}, need {}",
            params.initial_r_no
        )));
    }
    let no_change = no_total - params.initial_r_no;
    if no_change > 0 {
        if let Some(ref change_dest) = params.change_destination {
            let change_out = explicit_txout(
                &params.no_utxos[0].asset_id,
                no_change,
                change_dest,
            );
            add_pset_output(&mut pset, change_out);
        } else {
            return Err(Error::MissingChangeDestination);
        }
    }

    let lbtc_total: u64 = params.lbtc_utxos.iter().map(|u| u.value).sum();
    if lbtc_total < params.initial_r_lbtc {
        return Err(Error::AmmPool(format!(
            "insufficient LBTC UTXOs: have {lbtc_total}, need {}",
            params.initial_r_lbtc
        )));
    }
    let lbtc_change = lbtc_total - params.initial_r_lbtc;
    if lbtc_change > 0 {
        if let Some(ref change_dest) = params.change_destination {
            let change_out = explicit_txout(
                &params.lbtc_utxos[0].asset_id,
                lbtc_change,
                change_dest,
            );
            add_pset_output(&mut pset, change_out);
        } else {
            return Err(Error::MissingChangeDestination);
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

    // Fee output
    let fee_out = fee_txout(&params.fee_asset_id, params.fee_amount);
    add_pset_output(&mut pset, fee_out);

    Ok(pset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amm_pool::params::AmmPoolParams;
    use crate::taproot::NUMS_KEY_BYTES;
    use simplicityhl::elements::{OutPoint, Txid};
    use simplicityhl::elements::hashes::Hash;

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
    fn creation_pset_output_layout() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let creation = PoolCreationParams {
            yes_utxos: vec![test_utxo(params.yes_asset_id, 1000, 0)],
            no_utxos: vec![test_utxo(params.no_asset_id, 1000, 1)],
            lbtc_utxos: vec![test_utxo(params.lbtc_asset_id, 5000, 2)],
            lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            initial_r_yes: 1000,
            initial_r_no: 1000,
            initial_r_lbtc: 5000,
            initial_issued_lp: 100,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        let pset = build_pool_creation_pset(&contract, &creation).unwrap();
        // 5 inputs: YES, NO, LBTC, RT, fee
        assert_eq!(pset.n_inputs(), 5);
        // 6 outputs: YES reserve, NO reserve, LBTC reserve, RT, LP tokens, fee
        assert_eq!(pset.n_outputs(), 6);
    }

    #[test]
    fn creation_rejects_empty_utxos() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let creation = PoolCreationParams {
            yes_utxos: vec![],
            no_utxos: vec![test_utxo(params.no_asset_id, 1000, 1)],
            lbtc_utxos: vec![test_utxo(params.lbtc_asset_id, 5000, 2)],
            lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            initial_r_yes: 1000,
            initial_r_no: 1000,
            initial_r_lbtc: 5000,
            initial_issued_lp: 100,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_pool_creation_pset(&contract, &creation).is_err());
    }

    #[test]
    fn creation_rejects_zero_reserves() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let creation = PoolCreationParams {
            yes_utxos: vec![test_utxo(params.yes_asset_id, 1000, 0)],
            no_utxos: vec![test_utxo(params.no_asset_id, 1000, 1)],
            lbtc_utxos: vec![test_utxo(params.lbtc_asset_id, 5000, 2)],
            lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            initial_r_yes: 0,
            initial_r_no: 1000,
            initial_r_lbtc: 5000,
            initial_issued_lp: 100,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_pool_creation_pset(&contract, &creation).is_err());
    }

    #[test]
    fn creation_rejects_zero_issued_lp() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let creation = PoolCreationParams {
            yes_utxos: vec![test_utxo(params.yes_asset_id, 1000, 0)],
            no_utxos: vec![test_utxo(params.no_asset_id, 1000, 1)],
            lbtc_utxos: vec![test_utxo(params.lbtc_asset_id, 5000, 2)],
            lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            initial_r_yes: 1000,
            initial_r_no: 1000,
            initial_r_lbtc: 5000,
            initial_issued_lp: 0,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_pool_creation_pset(&contract, &creation).is_err());
    }

    #[test]
    fn creation_with_change() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let creation = PoolCreationParams {
            yes_utxos: vec![test_utxo(params.yes_asset_id, 2000, 0)],
            no_utxos: vec![test_utxo(params.no_asset_id, 1000, 1)],
            lbtc_utxos: vec![test_utxo(params.lbtc_asset_id, 5000, 2)],
            lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            initial_r_yes: 1000,
            initial_r_no: 1000,
            initial_r_lbtc: 5000,
            initial_issued_lp: 100,
            lp_token_destination: Script::new(),
            change_destination: Some(Script::new()),
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        let pset = build_pool_creation_pset(&contract, &creation).unwrap();
        // 6 base outputs + 1 YES change = 7
        assert_eq!(pset.n_outputs(), 7);
    }

    #[test]
    fn creation_insufficient_utxos() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let creation = PoolCreationParams {
            yes_utxos: vec![test_utxo(params.yes_asset_id, 500, 0)],
            no_utxos: vec![test_utxo(params.no_asset_id, 1000, 1)],
            lbtc_utxos: vec![test_utxo(params.lbtc_asset_id, 5000, 2)],
            lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            initial_r_yes: 1000,
            initial_r_no: 1000,
            initial_r_lbtc: 5000,
            initial_issued_lp: 100,
            lp_token_destination: Script::new(),
            change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 4),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_pool_creation_pset(&contract, &creation).is_err());
    }
}
