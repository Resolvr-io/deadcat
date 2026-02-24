use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::Script;

use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;

use super::super::contract::CompiledAmmPool;
use super::super::math::SwapPair;

/// Parameters for a pool swap transaction.
pub struct SwapParams {
    /// Pool YES reserve UTXO (covenant input 0).
    pub pool_yes_utxo: UnblindedUtxo,
    /// Pool NO reserve UTXO (covenant input 1).
    pub pool_no_utxo: UnblindedUtxo,
    /// Pool L-BTC reserve UTXO (covenant input 2).
    pub pool_lbtc_utxo: UnblindedUtxo,
    /// Pool LP reissuance token UTXO (covenant input 3).
    pub pool_lp_rt_utxo: UnblindedUtxo,
    /// Current issued LP count (for state verification).
    pub issued_lp: u64,
    /// Trader's funding UTXOs.
    pub trader_utxos: Vec<UnblindedUtxo>,
    /// Which pair is being swapped.
    pub swap_pair: SwapPair,
    /// New YES reserve after swap.
    pub new_r_yes: u64,
    /// New NO reserve after swap.
    pub new_r_no: u64,
    /// New L-BTC reserve after swap.
    pub new_r_lbtc: u64,
    /// Asset ID the trader receives.
    pub trader_receive_asset: [u8; 32],
    /// Amount the trader receives.
    pub trader_receive_amount: u64,
    /// Destination for trader's received tokens.
    pub trader_receive_destination: Script,
    /// Trader change destination.
    pub trader_change_destination: Option<Script>,
    /// Fee UTXO.
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount.
    pub fee_amount: u64,
    /// Fee asset ID.
    pub fee_asset_id: [u8; 32],
}

/// Build a swap PSET.
///
/// Inputs 0-3: pool covenant UTXOs. Inputs 4+: trader funding, fee.
/// Outputs 0-3: new reserves at same address (state unchanged).
/// Output 4: trader receive. 5+: change, fee.
pub fn build_swap_pset(
    contract: &CompiledAmmPool,
    params: &SwapParams,
) -> Result<PartiallySignedTransaction> {
    use crate::pset::{add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset};

    if params.trader_utxos.is_empty() {
        return Err(Error::AmmPool("trader funding UTXOs must be non-empty".into()));
    }
    if params.new_r_yes == 0 || params.new_r_no == 0 || params.new_r_lbtc == 0 {
        return Err(Error::AmmPool("new reserves must be non-zero".into()));
    }
    if params.trader_receive_amount == 0 {
        return Err(Error::AmmPool("trader receive amount must be non-zero".into()));
    }

    // Swap path: state unchanged, same address
    let covenant_spk = contract.script_pubkey(params.issued_lp);
    let mut pset = new_pset();

    // Inputs 0-3: pool covenant UTXOs
    add_pset_input(&mut pset, &params.pool_yes_utxo);
    add_pset_input(&mut pset, &params.pool_no_utxo);
    add_pset_input(&mut pset, &params.pool_lbtc_utxo);
    add_pset_input(&mut pset, &params.pool_lp_rt_utxo);

    // Inputs 4+: trader funding UTXOs
    for utxo in &params.trader_utxos {
        add_pset_input(&mut pset, utxo);
    }
    // Fee input
    add_pset_input(&mut pset, &params.fee_utxo);

    // Outputs 0-3: new reserves at same covenant address
    let yes_out = explicit_txout(
        &contract.params().yes_asset_id,
        params.new_r_yes,
        &covenant_spk,
    );
    add_pset_output(&mut pset, yes_out);

    let no_out = explicit_txout(
        &contract.params().no_asset_id,
        params.new_r_no,
        &covenant_spk,
    );
    add_pset_output(&mut pset, no_out);

    let lbtc_out = explicit_txout(
        &contract.params().lbtc_asset_id,
        params.new_r_lbtc,
        &covenant_spk,
    );
    add_pset_output(&mut pset, lbtc_out);

    // RT passthrough (explicit, amount = 1)
    let rt_out = explicit_txout(
        &contract.params().lp_reissuance_token_id,
        1,
        &covenant_spk,
    );
    add_pset_output(&mut pset, rt_out);

    // Output 4: trader receive
    let trader_out = explicit_txout(
        &params.trader_receive_asset,
        params.trader_receive_amount,
        &params.trader_receive_destination,
    );
    add_pset_output(&mut pset, trader_out);

    // Trader change: determine the input asset and how much was deposited.
    // The trader sends asset B into the pool (reserve B increases) and receives asset A.
    // Identify which reserve increased to find the deposit amount and asset.
    let (trader_deposit_amount, trader_deposit_asset) = match params.swap_pair {
        SwapPair::YesNo => {
            if params.new_r_yes > params.pool_yes_utxo.value {
                (params.new_r_yes - params.pool_yes_utxo.value, contract.params().yes_asset_id)
            } else {
                (params.new_r_no - params.pool_no_utxo.value, contract.params().no_asset_id)
            }
        }
        SwapPair::YesLbtc => {
            if params.new_r_yes > params.pool_yes_utxo.value {
                (params.new_r_yes - params.pool_yes_utxo.value, contract.params().yes_asset_id)
            } else {
                (params.new_r_lbtc - params.pool_lbtc_utxo.value, contract.params().lbtc_asset_id)
            }
        }
        SwapPair::NoLbtc => {
            if params.new_r_no > params.pool_no_utxo.value {
                (params.new_r_no - params.pool_no_utxo.value, contract.params().no_asset_id)
            } else {
                (params.new_r_lbtc - params.pool_lbtc_utxo.value, contract.params().lbtc_asset_id)
            }
        }
    };

    let trader_total: u64 = params.trader_utxos.iter().map(|u| u.value).sum();
    if trader_total < trader_deposit_amount {
        return Err(Error::AmmPool(format!(
            "insufficient trader UTXOs: have {trader_total}, need {trader_deposit_amount}"
        )));
    }
    let change = trader_total - trader_deposit_amount;
    if change > 0 {
        if let Some(ref change_dest) = params.trader_change_destination {
            let change_out = explicit_txout(
                &trader_deposit_asset,
                change,
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
        if let Some(ref change_dest) = params.trader_change_destination {
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
    fn swap_pset_output_layout() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let swap = SwapParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            trader_utxos: vec![test_utxo(params.lbtc_asset_id, 1000, 4)],
            swap_pair: SwapPair::YesLbtc,
            new_r_yes: 9_800,
            new_r_no: 10_000,
            new_r_lbtc: 51_000,
            trader_receive_asset: params.yes_asset_id,
            trader_receive_amount: 200,
            trader_receive_destination: Script::new(),
            trader_change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        let pset = build_swap_pset(&contract, &swap).unwrap();
        // 6 inputs: 4 pool + 1 trader + 1 fee
        assert_eq!(pset.n_inputs(), 6);
        // 6 outputs: 4 reserves + trader receive + fee
        assert_eq!(pset.n_outputs(), 6);
    }

    #[test]
    fn swap_rejects_empty_trader_utxos() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let swap = SwapParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            trader_utxos: vec![],
            swap_pair: SwapPair::YesLbtc,
            new_r_yes: 9_800,
            new_r_no: 10_000,
            new_r_lbtc: 51_000,
            trader_receive_asset: params.yes_asset_id,
            trader_receive_amount: 200,
            trader_receive_destination: Script::new(),
            trader_change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_swap_pset(&contract, &swap).is_err());
    }

    #[test]
    fn swap_rejects_zero_reserves() {
        let params = test_params();
        let contract = CompiledAmmPool::new(params).unwrap();
        let swap = SwapParams {
            pool_yes_utxo: test_utxo(params.yes_asset_id, 10_000, 0),
            pool_no_utxo: test_utxo(params.no_asset_id, 10_000, 1),
            pool_lbtc_utxo: test_utxo(params.lbtc_asset_id, 50_000, 2),
            pool_lp_rt_utxo: test_utxo(params.lp_reissuance_token_id, 1, 3),
            issued_lp: 100,
            trader_utxos: vec![test_utxo(params.lbtc_asset_id, 1000, 4)],
            swap_pair: SwapPair::YesLbtc,
            new_r_yes: 0,
            new_r_no: 10_000,
            new_r_lbtc: 51_000,
            trader_receive_asset: params.yes_asset_id,
            trader_receive_amount: 200,
            trader_receive_destination: Script::new(),
            trader_change_destination: None,
            fee_utxo: test_utxo(params.lbtc_asset_id, 500, 5),
            fee_amount: 500,
            fee_asset_id: params.lbtc_asset_id,
        };
        assert!(build_swap_pset(&contract, &swap).is_err());
    }
}
