use std::collections::HashSet;

use crate::lmsr_pool::contract::CompiledLmsrPool;
use crate::lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolId, LmsrPoolIdInput, LmsrPoolParams};
use crate::network::Network;
use crate::prediction_market::params::{MarketId, derive_market_id_from_assets};

pub(crate) fn derive_lmsr_pool_id(
    network: Network,
    params: LmsrPoolParams,
    creation_txid: [u8; 32],
    initial_reserve_outpoints: [LmsrInitialOutpoint; 3],
) -> Result<LmsrPoolId, String> {
    validate_initial_reserve_outpoints(initial_reserve_outpoints, creation_txid)?;
    let contract = CompiledLmsrPool::new(params).map_err(|e| e.to_string())?;
    LmsrPoolId::derive_v1(&LmsrPoolIdInput {
        chain_genesis_hash: network.genesis_hash(),
        params,
        covenant_cmr: contract.primary_cmr().to_byte_array(),
        creation_txid,
        initial_yes_outpoint: initial_reserve_outpoints[0],
        initial_no_outpoint: initial_reserve_outpoints[1],
        initial_collateral_outpoint: initial_reserve_outpoints[2],
    })
    .map_err(|e| e.to_string())
}

pub(crate) fn validate_initial_reserve_outpoints(
    outpoints: [LmsrInitialOutpoint; 3],
    creation_txid: [u8; 32],
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (idx, outpoint) in outpoints.iter().enumerate() {
        if outpoint.txid != creation_txid {
            return Err(format!(
                "initial_reserve_outpoints[{idx}] txid must match creation_txid"
            ));
        }
        if !seen.insert((outpoint.txid, outpoint.vout)) {
            return Err("duplicate LMSR initial reserve outpoint".into());
        }
    }
    Ok(())
}

pub(crate) fn derive_lmsr_market_id(params: LmsrPoolParams) -> MarketId {
    derive_market_id_from_assets(params.yes_asset_id, params.no_asset_id)
}
