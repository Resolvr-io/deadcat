//! Pool chain walk: reconstruct AMM pool state history from on-chain data.
//!
//! Starting from the pool creation txid, we walk the UTXO chain forward
//! to the current state, producing a snapshot at each state transition.

use std::collections::HashSet;

use simplicityhl::elements::{OutPoint, Transaction, Txid};

use crate::amm_pool::contract::CompiledAmmPool;
use crate::amm_pool::params::AmmPoolParams;
use crate::chain::ChainBackend;
use crate::error::{Error, Result};

/// A single on-chain pool state snapshot.
#[derive(Debug, Clone)]
pub struct PoolStateSnapshot {
    pub txid: Txid,
    pub r_yes: u64,
    pub r_no: u64,
    pub r_lbtc: u64,
    pub issued_lp: u64,
    pub block_height: Option<i32>,
}

/// Walk the UTXO chain from creation (or resume point) to current state.
///
/// Returns all intermediate snapshots (including the starting state).
///
/// If `resume_from` is provided, the walk begins from those outpoints instead
/// of the creation tx, allowing incremental sync.
pub fn walk_pool_chain(
    chain: &dyn ChainBackend,
    params: &AmmPoolParams,
    creation_txid: Txid,
    resume_from: Option<(Txid, u64)>,
) -> Result<Vec<PoolStateSnapshot>> {
    let contract = CompiledAmmPool::new(*params)?;
    let mut snapshots = Vec::new();

    // Determine starting state
    let (mut current_txid, mut current_issued_lp) = if let Some((txid, lp)) = resume_from {
        (txid, lp)
    } else {
        // Parse creation tx for initial state
        let creation_tx = chain.fetch_transaction(&creation_txid)?;
        let (r_yes, r_no, r_lbtc) = parse_reserves(&creation_tx, params)?;
        let initial_lp = parse_initial_issued_lp(&creation_tx)?;

        let height = get_tx_height(chain, &contract, initial_lp, &creation_txid);

        snapshots.push(PoolStateSnapshot {
            txid: creation_txid,
            r_yes,
            r_no,
            r_lbtc,
            issued_lp: initial_lp,
            block_height: height,
        });

        (creation_txid, initial_lp)
    };

    // Walk forward
    loop {
        let current_spk = contract.script_pubkey(current_issued_lp);
        let history = chain.get_script_history(&current_spk)?;

        // Build the set of current outpoints (outputs 0-3 of current tx)
        let current_outpoints: HashSet<OutPoint> = (0..4)
            .map(|vout| OutPoint::new(current_txid, vout))
            .collect();

        // Find the spending tx: any tx in history whose inputs spend one of our outpoints
        let mut spending_tx: Option<(Transaction, Option<i32>)> = None;
        for entry in &history {
            if entry.txid == current_txid {
                continue; // Skip the current tx itself
            }
            let tx = chain.fetch_transaction(&entry.txid)?;
            let spends_current = tx
                .input
                .iter()
                .any(|inp| current_outpoints.contains(&inp.previous_output));

            if spends_current {
                let height = if entry.height > 0 {
                    Some(entry.height)
                } else {
                    None
                };
                spending_tx = Some((tx, height));
                break;
            }
        }

        let Some((tx, block_height)) = spending_tx else {
            // No spending tx found — current state is the tip
            break;
        };

        let next_txid = tx.txid();

        // Parse new reserves from the spending tx
        let (r_yes, r_no, r_lbtc) = parse_reserves(&tx, params)?;

        // Determine new issued_lp
        let new_issued_lp = determine_issued_lp(&tx, params, current_issued_lp)?;

        snapshots.push(PoolStateSnapshot {
            txid: next_txid,
            r_yes,
            r_no,
            r_lbtc,
            issued_lp: new_issued_lp,
            block_height,
        });

        current_txid = next_txid;
        current_issued_lp = new_issued_lp;
    }

    Ok(snapshots)
}

/// Parse reserves from a pool transaction's outputs.
/// Output layout: 0=YES, 1=NO, 2=LBTC (consistent across all pool txs).
fn parse_reserves(tx: &Transaction, params: &AmmPoolParams) -> Result<(u64, u64, u64)> {
    if tx.output.len() < 3 {
        return Err(Error::AmmPool("pool tx has fewer than 3 outputs".into()));
    }

    let r_yes = extract_explicit_value(&tx.output[0], &params.yes_asset_id, "YES output")?;
    let r_no = extract_explicit_value(&tx.output[1], &params.no_asset_id, "NO output")?;
    let r_lbtc = extract_explicit_value(&tx.output[2], &params.lbtc_asset_id, "LBTC output")?;

    Ok((r_yes, r_no, r_lbtc))
}

/// Extract explicit (non-confidential) value from a transaction output,
/// validating the asset ID matches.
fn extract_explicit_value(
    txout: &simplicityhl::elements::TxOut,
    expected_asset: &[u8; 32],
    label: &str,
) -> Result<u64> {
    use simplicityhl::elements::confidential;

    let asset_id = match txout.asset {
        confidential::Asset::Explicit(id) => id,
        _ => return Err(Error::AmmPool(format!("{label}: confidential asset"))),
    };

    let asset_bytes = asset_id.into_inner().to_byte_array();
    if asset_bytes != *expected_asset {
        return Err(Error::AmmPool(format!("{label}: asset mismatch")));
    }

    match txout.value {
        confidential::Value::Explicit(v) => Ok(v),
        _ => Err(Error::AmmPool(format!("{label}: confidential value"))),
    }
}

/// Parse the initial issued LP from the creation tx.
/// The creation tx has an issuance input whose `amount` field gives the initial LP supply.
fn parse_initial_issued_lp(tx: &Transaction) -> Result<u64> {
    for txin in &tx.input {
        if txin.asset_issuance.is_null() {
            continue;
        }
        let issuance = &txin.asset_issuance;
        // The issuance amount (not inflation keys) is the LP mint
        if let Some(amount) = explicit_issuance_amount(issuance)
            && amount > 0
        {
            return Ok(amount);
        }
    }
    Err(Error::AmmPool("creation tx has no LP issuance".into()))
}

/// Determine new issued_lp after a state transition.
///
/// - If an input has LP reissuance (asset_issuance.amount > 0): deposit → issued_lp += amount
/// - If output 4 is a burn (empty script with LP asset): withdraw → issued_lp -= burn_amount
/// - Otherwise: swap → issued_lp unchanged
fn determine_issued_lp(
    tx: &Transaction,
    params: &AmmPoolParams,
    current_issued_lp: u64,
) -> Result<u64> {
    // Check for LP reissuance (deposit)
    for txin in &tx.input {
        if txin.asset_issuance.is_null() {
            continue;
        }
        if let Some(amount) = explicit_issuance_amount(&txin.asset_issuance)
            && amount > 0
        {
            return Ok(current_issued_lp + amount);
        }
    }

    // Check for LP burn on output 4 (withdraw)
    if tx.output.len() > 4 {
        let burn_out = &tx.output[4];
        if burn_out.script_pubkey.is_empty() {
            // Check if this is the LP asset
            if let simplicityhl::elements::confidential::Asset::Explicit(id) = burn_out.asset {
                let asset_bytes = id.into_inner().to_byte_array();
                if asset_bytes == params.lp_asset_id
                    && let simplicityhl::elements::confidential::Value::Explicit(burn_amount) =
                        burn_out.value
                {
                    return Ok(current_issued_lp.saturating_sub(burn_amount));
                }
            }
        }
    }

    // Swap: no LP change
    Ok(current_issued_lp)
}

/// Extract explicit issuance amount from an AssetIssuance, if present.
fn explicit_issuance_amount(issuance: &simplicityhl::elements::AssetIssuance) -> Option<u64> {
    use simplicityhl::elements::confidential;
    match issuance.amount {
        confidential::Value::Explicit(v) => Some(v),
        confidential::Value::Null => None,
        _ => None,
    }
}

/// Try to get the block height for a tx by checking the script history.
fn get_tx_height(
    chain: &dyn ChainBackend,
    contract: &CompiledAmmPool,
    issued_lp: u64,
    txid: &Txid,
) -> Option<i32> {
    let spk = contract.script_pubkey(issued_lp);
    if let Ok(history) = chain.get_script_history(&spk) {
        for entry in &history {
            if entry.txid == *txid && entry.height > 0 {
                return Some(entry.height);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::ScriptHistoryEntry;
    use simplicityhl::elements::confidential::{Asset, Value};
    use simplicityhl::elements::hashes::Hash;
    use simplicityhl::elements::secp256k1_zkp::Tweak;
    use simplicityhl::elements::{
        AssetId, AssetIssuance, Script, TxIn, TxInWitness, TxOut, TxOutWitness,
    };

    /// Mock chain backend for testing.
    struct MockChain {
        transactions: std::collections::HashMap<Txid, Transaction>,
        histories: std::collections::HashMap<Vec<u8>, Vec<ScriptHistoryEntry>>,
    }

    impl MockChain {
        fn new() -> Self {
            Self {
                transactions: std::collections::HashMap::new(),
                histories: std::collections::HashMap::new(),
            }
        }

        fn add_tx(&mut self, tx: Transaction) {
            self.transactions.insert(tx.txid(), tx);
        }

        fn add_history(&mut self, spk: &Script, entries: Vec<ScriptHistoryEntry>) {
            self.histories.insert(spk.as_bytes().to_vec(), entries);
        }
    }

    impl ChainBackend for MockChain {
        fn scan_script_utxos(&self, _script_pubkey: &Script) -> Result<Vec<(OutPoint, TxOut)>> {
            Ok(vec![])
        }

        fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction> {
            self.transactions
                .get(txid)
                .cloned()
                .ok_or_else(|| Error::Query(format!("tx not found: {txid}")))
        }

        fn broadcast(&self, _tx: &Transaction) -> Result<Txid> {
            unimplemented!()
        }

        fn get_script_history(&self, script_pubkey: &Script) -> Result<Vec<ScriptHistoryEntry>> {
            Ok(self
                .histories
                .get(script_pubkey.as_bytes())
                .cloned()
                .unwrap_or_default())
        }
    }

    fn test_params() -> AmmPoolParams {
        AmmPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        }
    }

    fn make_asset(id: &[u8; 32]) -> Asset {
        Asset::Explicit(AssetId::from_slice(id).expect("valid asset id"))
    }

    fn make_pool_output(asset_id: &[u8; 32], value: u64, script: Script) -> TxOut {
        TxOut {
            asset: make_asset(asset_id),
            value: Value::Explicit(value),
            nonce: simplicityhl::elements::confidential::Nonce::Null,
            script_pubkey: script,
            witness: TxOutWitness::default(),
        }
    }

    fn make_creation_tx(
        params: &AmmPoolParams,
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
        issued_lp: u64,
    ) -> Transaction {
        let contract = CompiledAmmPool::new(*params).unwrap();
        let spk = contract.script_pubkey(issued_lp);

        let issuance = AssetIssuance {
            asset_blinding_nonce: Tweak::from_slice(&[0u8; 32]).expect("valid nonce"),
            asset_entropy: [0u8; 32],
            amount: Value::Explicit(issued_lp),
            inflation_keys: Value::Null,
        };

        let input = TxIn {
            previous_output: OutPoint::new(Txid::all_zeros(), 0),
            is_pegin: false,
            script_sig: Script::new(),
            sequence: simplicityhl::elements::Sequence::MAX,
            asset_issuance: issuance,
            witness: TxInWitness::default(),
        };

        Transaction {
            version: 2,
            lock_time: simplicityhl::elements::LockTime::ZERO,
            input: vec![input],
            output: vec![
                make_pool_output(&params.yes_asset_id, r_yes, spk.clone()),
                make_pool_output(&params.no_asset_id, r_no, spk.clone()),
                make_pool_output(&params.lbtc_asset_id, r_lbtc, spk.clone()),
                make_pool_output(&params.lp_reissuance_token_id, 1, spk),
            ],
        }
    }

    fn make_swap_tx(
        params: &AmmPoolParams,
        prev_txid: Txid,
        issued_lp: u64,
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
    ) -> Transaction {
        let contract = CompiledAmmPool::new(*params).unwrap();
        let spk = contract.script_pubkey(issued_lp);

        let inputs: Vec<TxIn> = (0..4)
            .map(|vout| TxIn {
                previous_output: OutPoint::new(prev_txid, vout),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: simplicityhl::elements::Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            })
            .collect();

        Transaction {
            version: 2,
            lock_time: simplicityhl::elements::LockTime::ZERO,
            input: inputs,
            output: vec![
                make_pool_output(&params.yes_asset_id, r_yes, spk.clone()),
                make_pool_output(&params.no_asset_id, r_no, spk.clone()),
                make_pool_output(&params.lbtc_asset_id, r_lbtc, spk.clone()),
                make_pool_output(&params.lp_reissuance_token_id, 1, spk),
            ],
        }
    }

    #[test]
    fn walk_creation_only() {
        let params = test_params();
        let creation_tx = make_creation_tx(&params, 500_000, 500_000, 250_000, 1_000_000);
        let creation_txid = creation_tx.txid();

        let contract = CompiledAmmPool::new(params).unwrap();
        let spk = contract.script_pubkey(1_000_000);

        let mut chain = MockChain::new();
        chain.add_tx(creation_tx);
        // History for creation SPK: only the creation tx, no spending tx
        chain.add_history(
            &spk,
            vec![ScriptHistoryEntry {
                txid: creation_txid,
                height: 100,
            }],
        );

        let snapshots = walk_pool_chain(&chain, &params, creation_txid, None).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].r_yes, 500_000);
        assert_eq!(snapshots[0].r_no, 500_000);
        assert_eq!(snapshots[0].r_lbtc, 250_000);
        assert_eq!(snapshots[0].issued_lp, 1_000_000);
        assert_eq!(snapshots[0].block_height, Some(100));
    }

    #[test]
    fn walk_creation_then_swap() {
        let params = test_params();
        let creation_tx = make_creation_tx(&params, 500_000, 500_000, 250_000, 1_000_000);
        let creation_txid = creation_tx.txid();

        // Swap: YES goes up, NO goes down (sell NO for YES)
        let swap_tx = make_swap_tx(&params, creation_txid, 1_000_000, 480_000, 520_000, 250_000);
        let swap_txid = swap_tx.txid();

        let contract = CompiledAmmPool::new(params).unwrap();
        let spk = contract.script_pubkey(1_000_000);

        let mut chain = MockChain::new();
        chain.add_tx(creation_tx);
        chain.add_tx(swap_tx);
        // History for the SPK: both creation and swap tx
        chain.add_history(
            &spk,
            vec![
                ScriptHistoryEntry {
                    txid: creation_txid,
                    height: 100,
                },
                ScriptHistoryEntry {
                    txid: swap_txid,
                    height: 101,
                },
            ],
        );

        let snapshots = walk_pool_chain(&chain, &params, creation_txid, None).unwrap();
        assert_eq!(snapshots.len(), 2);

        // Creation snapshot
        assert_eq!(snapshots[0].r_yes, 500_000);
        assert_eq!(snapshots[0].issued_lp, 1_000_000);

        // Swap snapshot
        assert_eq!(snapshots[1].r_yes, 480_000);
        assert_eq!(snapshots[1].r_no, 520_000);
        assert_eq!(snapshots[1].r_lbtc, 250_000);
        assert_eq!(snapshots[1].issued_lp, 1_000_000); // unchanged for swap
    }

    #[test]
    fn walk_incremental_resume() {
        let params = test_params();
        let creation_tx = make_creation_tx(&params, 500_000, 500_000, 250_000, 1_000_000);
        let creation_txid = creation_tx.txid();

        let swap_tx = make_swap_tx(&params, creation_txid, 1_000_000, 480_000, 520_000, 250_000);
        let swap_txid = swap_tx.txid();

        let contract = CompiledAmmPool::new(params).unwrap();
        let spk = contract.script_pubkey(1_000_000);

        let mut chain = MockChain::new();
        chain.add_tx(creation_tx);
        chain.add_tx(swap_tx);
        chain.add_history(
            &spk,
            vec![
                ScriptHistoryEntry {
                    txid: creation_txid,
                    height: 100,
                },
                ScriptHistoryEntry {
                    txid: swap_txid,
                    height: 101,
                },
            ],
        );

        // Resume from creation tx (simulating we already have the creation snapshot)
        let snapshots = walk_pool_chain(
            &chain,
            &params,
            creation_txid,
            Some((creation_txid, 1_000_000)),
        )
        .unwrap();

        // Should only return the swap snapshot (new since resume point)
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].r_yes, 480_000);
    }

    /// Build a deposit tx: spends previous pool outputs, reissues LP tokens.
    fn make_deposit_tx(
        params: &AmmPoolParams,
        prev_txid: Txid,
        prev_issued_lp: u64,
        new_issued_lp: u64,
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
    ) -> Transaction {
        let contract = CompiledAmmPool::new(*params).unwrap();
        let spk = contract.script_pubkey(new_issued_lp);
        let mint_amount = new_issued_lp - prev_issued_lp;

        // First input: spends output 0, has LP reissuance
        let issuance = AssetIssuance {
            asset_blinding_nonce: Tweak::from_slice(&[0u8; 32]).expect("valid nonce"),
            asset_entropy: [0u8; 32],
            amount: Value::Explicit(mint_amount),
            inflation_keys: Value::Null,
        };

        let mut inputs: Vec<TxIn> = vec![TxIn {
            previous_output: OutPoint::new(prev_txid, 0),
            is_pegin: false,
            script_sig: Script::new(),
            sequence: simplicityhl::elements::Sequence::MAX,
            asset_issuance: issuance,
            witness: TxInWitness::default(),
        }];
        for vout in 1..4 {
            inputs.push(TxIn {
                previous_output: OutPoint::new(prev_txid, vout),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: simplicityhl::elements::Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            });
        }

        Transaction {
            version: 2,
            lock_time: simplicityhl::elements::LockTime::ZERO,
            input: inputs,
            output: vec![
                make_pool_output(&params.yes_asset_id, r_yes, spk.clone()),
                make_pool_output(&params.no_asset_id, r_no, spk.clone()),
                make_pool_output(&params.lbtc_asset_id, r_lbtc, spk.clone()),
                make_pool_output(&params.lp_reissuance_token_id, 1, spk),
            ],
        }
    }

    /// Build a withdraw tx: spends previous pool outputs, burns LP on output 4.
    fn make_withdraw_tx(
        params: &AmmPoolParams,
        prev_txid: Txid,
        issued_lp: u64,
        burn_amount: u64,
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
    ) -> Transaction {
        let new_lp = issued_lp - burn_amount;
        let contract = CompiledAmmPool::new(*params).unwrap();
        let spk = contract.script_pubkey(new_lp);

        let inputs: Vec<TxIn> = (0..4)
            .map(|vout| TxIn {
                previous_output: OutPoint::new(prev_txid, vout),
                is_pegin: false,
                script_sig: Script::new(),
                sequence: simplicityhl::elements::Sequence::MAX,
                asset_issuance: AssetIssuance::default(),
                witness: TxInWitness::default(),
            })
            .collect();

        // Output 4: LP burn (empty script, LP asset)
        let burn_output = TxOut {
            asset: make_asset(&params.lp_asset_id),
            value: Value::Explicit(burn_amount),
            nonce: simplicityhl::elements::confidential::Nonce::Null,
            script_pubkey: Script::new(), // empty script = burn
            witness: TxOutWitness::default(),
        };

        Transaction {
            version: 2,
            lock_time: simplicityhl::elements::LockTime::ZERO,
            input: inputs,
            output: vec![
                make_pool_output(&params.yes_asset_id, r_yes, spk.clone()),
                make_pool_output(&params.no_asset_id, r_no, spk.clone()),
                make_pool_output(&params.lbtc_asset_id, r_lbtc, spk.clone()),
                make_pool_output(&params.lp_reissuance_token_id, 1, spk),
                burn_output,
            ],
        }
    }

    #[test]
    fn walk_deposit_increases_lp() {
        let params = test_params();
        let initial_lp = 1_000_000u64;
        let creation_tx = make_creation_tx(&params, 500_000, 500_000, 250_000, initial_lp);
        let creation_txid = creation_tx.txid();

        let new_lp = 1_500_000u64;
        let deposit_tx = make_deposit_tx(
            &params,
            creation_txid,
            initial_lp,
            new_lp,
            750_000,
            750_000,
            375_000,
        );
        let deposit_txid = deposit_tx.txid();

        let contract = CompiledAmmPool::new(params).unwrap();
        let spk_initial = contract.script_pubkey(initial_lp);
        let spk_after = contract.script_pubkey(new_lp);

        let mut chain = MockChain::new();
        chain.add_tx(creation_tx);
        chain.add_tx(deposit_tx);
        // Creation SPK history: creation tx + deposit tx (deposit spends from this SPK)
        chain.add_history(
            &spk_initial,
            vec![
                ScriptHistoryEntry {
                    txid: creation_txid,
                    height: 100,
                },
                ScriptHistoryEntry {
                    txid: deposit_txid,
                    height: 101,
                },
            ],
        );
        // After-deposit SPK history: only the deposit tx (no further spending)
        chain.add_history(
            &spk_after,
            vec![ScriptHistoryEntry {
                txid: deposit_txid,
                height: 101,
            }],
        );

        let snapshots = walk_pool_chain(&chain, &params, creation_txid, None).unwrap();
        assert_eq!(snapshots.len(), 2);

        assert_eq!(snapshots[0].issued_lp, initial_lp);
        assert_eq!(snapshots[0].r_yes, 500_000);

        assert_eq!(snapshots[1].issued_lp, new_lp);
        assert_eq!(snapshots[1].r_yes, 750_000);
        assert_eq!(snapshots[1].r_lbtc, 375_000);
    }

    #[test]
    fn walk_withdraw_decreases_lp() {
        let params = test_params();
        let initial_lp = 1_000_000u64;
        let creation_tx = make_creation_tx(&params, 500_000, 500_000, 250_000, initial_lp);
        let creation_txid = creation_tx.txid();

        let burn = 200_000u64;
        let withdraw_tx = make_withdraw_tx(
            &params,
            creation_txid,
            initial_lp,
            burn,
            400_000,
            400_000,
            200_000,
        );
        let withdraw_txid = withdraw_tx.txid();

        let contract = CompiledAmmPool::new(params).unwrap();
        let spk_initial = contract.script_pubkey(initial_lp);
        let spk_after = contract.script_pubkey(initial_lp - burn);

        let mut chain = MockChain::new();
        chain.add_tx(creation_tx);
        chain.add_tx(withdraw_tx);
        chain.add_history(
            &spk_initial,
            vec![
                ScriptHistoryEntry {
                    txid: creation_txid,
                    height: 100,
                },
                ScriptHistoryEntry {
                    txid: withdraw_txid,
                    height: 101,
                },
            ],
        );
        chain.add_history(
            &spk_after,
            vec![ScriptHistoryEntry {
                txid: withdraw_txid,
                height: 101,
            }],
        );

        let snapshots = walk_pool_chain(&chain, &params, creation_txid, None).unwrap();
        assert_eq!(snapshots.len(), 2);

        assert_eq!(snapshots[0].issued_lp, initial_lp);
        assert_eq!(snapshots[1].issued_lp, initial_lp - burn);
        assert_eq!(snapshots[1].r_yes, 400_000);
    }
}
