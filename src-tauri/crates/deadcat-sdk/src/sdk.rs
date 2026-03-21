use std::collections::HashMap;
use std::path::Path;

use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::elements::confidential::Asset;
use lwk_wollet::elements::hashes::Hash as _;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp::{self, Keypair};
use lwk_wollet::elements::{AssetId, OutPoint, Script, Transaction, TxOut, Txid};
use lwk_wollet::elements_miniscript::confidential::slip77::MasterBlindingKey;
use lwk_wollet::{
    ElectrumClient, ElectrumUrl, TxBuilder, WalletTx, WalletTxOut, Wollet, WolletDescriptor,
};
use rand::RngCore;
use rand::thread_rng;

use crate::assembly::{pset_to_pruning_transaction, txout_secrets_from_unblinded};
use crate::chain::{ChainBackend, ElectrumBackend};
use crate::error::{Error, Result};
use crate::lmsr_pool::api::{
    AdjustLmsrPoolRequest, AdjustLmsrPoolResult, CloseLmsrPoolRequest, CloseLmsrPoolResult,
    CreateLmsrPoolRequest, LmsrPoolLocator, LmsrPoolSnapshot, txid_to_canonical_bytes,
};
use crate::lmsr_pool::assembly::attach_lmsr_pool_witnesses;
use crate::lmsr_pool::chain_walk::{
    decode_primary_witness_payload_from_spend_tx, extract_reserve_window,
};
use crate::lmsr_pool::contract::CompiledLmsrPool;
use crate::lmsr_pool::identity::derive_lmsr_pool_id;
use crate::lmsr_pool::math::LmsrTradeKind;
use crate::lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolParams};
use crate::lmsr_pool::table::LmsrTableManifest;
use crate::maker_order::contract::CompiledMakerOrder;
use crate::maker_order::params::{
    MakerOrderParams, OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};
use crate::maker_order::pset::cancel_order::{CancelOrderParams, build_cancel_order_pset};
use crate::maker_order::pset::create_order::{CreateOrderParams, build_create_order_pset};
use crate::maker_order::pset::fill_order::{
    FillOrderParams, MakerOrderFill, TakerFill, build_fill_order_pset,
};
use crate::maker_order::witness::serialize_satisfied as serialize_maker_order_satisfied;
use crate::network::Network;
use crate::pool::PoolReserves;
use crate::prediction_market::anchor::{PredictionMarketAnchor, parse_prediction_market_anchor};
use crate::prediction_market::assembly::{
    CollateralSource, IssuanceAssemblyInputs, assemble_cancellation, assemble_expire_transition,
    assemble_expiry_redemption, assemble_issuance, assemble_oracle_resolve,
    assemble_post_resolution_redemption, compute_issuance_entropy,
};
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::params::PredictionMarketParams;
use crate::prediction_market::pset::cancellation::CancellationParams;
use crate::prediction_market::pset::creation::{CreationParams, build_creation_pset};
use crate::prediction_market::pset::expire_transition::ExpireTransitionParams;
use crate::prediction_market::pset::expiry_redemption::ExpiryRedemptionParams;
use crate::prediction_market::pset::oracle_resolve::OracleResolveParams;
use crate::prediction_market::pset::post_resolution_redemption::PostResolutionRedemptionParams;
use crate::prediction_market::state::MarketState;
use crate::prediction_market_scan::{
    PredictionMarketScanBackend, scan_prediction_market_canonical,
    validate_prediction_market_creation_tx,
};
use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
};
use crate::taproot::NUMS_KEY_BYTES;
use crate::trade::types::{LmsrPoolSwapLeg, LmsrPoolUtxos, LmsrPrimaryPath};

use crate::discovery::pool::LMSR_WITNESS_SCHEMA_V2;

/// Result of a successful token issuance.
#[derive(Debug, Clone)]
pub struct IssuanceResult {
    pub txid: Txid,
    pub previous_state: MarketState,
    pub new_state: MarketState,
    pub pairs_issued: u64,
}

/// Result of a successful token cancellation.
#[derive(Debug, Clone)]
pub struct CancellationResult {
    pub txid: Txid,
    pub previous_state: MarketState,
    pub new_state: MarketState,
    pub pairs_burned: u64,
    pub is_full_cancellation: bool,
}

/// Result of a successful oracle resolution.
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    pub txid: Txid,
    pub previous_state: MarketState,
    pub new_state: MarketState,
    pub outcome_yes: bool,
}

/// Result of a successful token redemption (post-resolution or expiry).
#[derive(Debug, Clone)]
pub struct RedemptionResult {
    pub txid: Txid,
    pub previous_state: MarketState,
    pub tokens_redeemed: u64,
    pub payout_sats: u64,
}

/// Result of a successful limit order creation.
#[derive(Debug, Clone)]
pub struct CreateOrderResult {
    pub txid: Txid,
    pub order_params: MakerOrderParams,
    pub maker_base_pubkey: [u8; 32],
    pub order_nonce: [u8; 32],
    pub covenant_address: String,
    pub order_amount: u64,
}

/// Result of a successful limit order fill.
#[derive(Debug, Clone)]
pub struct FillOrderResult {
    pub txid: Txid,
    pub lots_filled: u64,
    pub is_partial: bool,
}

/// Result of a successful limit order cancellation.
#[derive(Debug, Clone)]
pub struct CancelOrderResult {
    pub txid: Txid,
    pub refunded_amount: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct LmsrPoolScanResult {
    pub current_s_index: u64,
    pub pool_utxos: crate::trade::types::LmsrPoolUtxos,
    pub reserves: PoolReserves,
}

struct LmsrBootstrapPset {
    pset: PartiallySignedTransaction,
    wallet_inputs: Vec<UnblindedUtxo>,
    blind_output_indices: Vec<usize>,
}

pub struct DeadcatSdk {
    signer: SwSigner,
    wollet: Wollet,
    network: Network,
    chain: ElectrumBackend,
    /// Genesis hash for the Simplicity C runtime.
    ///
    /// For Liquid/Testnet, this is the hardcoded constant.
    /// For regtest, this must be set to the actual chain genesis hash
    /// via [`set_chain_genesis_hash`](Self::set_chain_genesis_hash).
    chain_genesis_override: Option<[u8; 32]>,
}

struct SdkPredictionMarketScanBackend<'a> {
    sdk: &'a DeadcatSdk,
}

impl PredictionMarketScanBackend for SdkPredictionMarketScanBackend<'_> {
    fn fetch_transaction(&self, txid: &Txid) -> std::result::Result<Transaction, String> {
        self.sdk
            .chain
            .fetch_transaction(txid)
            .map_err(|e| e.to_string())
    }

    fn spending_txid(
        &self,
        outpoint: &OutPoint,
        script_pubkey: &Script,
    ) -> std::result::Result<Option<Txid>, String> {
        let txids = self
            .sdk
            .chain
            .script_history_txids(script_pubkey)
            .map_err(|e| e.to_string())?;

        for txid in txids {
            let tx = self.fetch_transaction(&txid)?;
            if tx
                .input
                .iter()
                .any(|input| input.previous_output == *outpoint)
            {
                return Ok(Some(txid));
            }
        }

        Ok(None)
    }
}

impl DeadcatSdk {
    pub fn new(
        mnemonic: &str,
        network: Network,
        electrum_url: &str,
        datadir: &Path,
    ) -> Result<Self> {
        let signer = SwSigner::new(mnemonic, network.is_mainnet())
            .map_err(|e| Error::Signer(e.to_string()))?;

        let slip77_key = signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Signer(e.to_string()))?;
        let xpub = signer.xpub();
        let descriptor_str = format!("ct(slip77({}),elwpkh({}/*))", slip77_key, xpub);
        let descriptor: WolletDescriptor = descriptor_str
            .parse()
            .map_err(|e: lwk_wollet::Error| Error::Descriptor(e.to_string()))?;

        let persist_dir = datadir.join(network.as_str()).join("wallet_db");
        let wollet = Wollet::with_fs_persist(network.into_lwk(), descriptor, &persist_dir)
            .map_err(|e| Error::WalletInit(e.to_string()))?;

        Ok(Self {
            signer,
            wollet,
            network,
            chain: ElectrumBackend::new(electrum_url),
            chain_genesis_override: None,
        })
    }

    pub fn generate_mnemonic(is_mainnet: bool) -> Result<(String, SwSigner)> {
        let (signer, mnemonic) =
            SwSigner::random(is_mainnet).map_err(|e| Error::Signer(e.to_string()))?;
        Ok((mnemonic.to_string(), signer))
    }

    // ── Wallet queries ───────────────────────────────────────────────────

    pub fn sync(&mut self) -> Result<()> {
        let url: ElectrumUrl = self
            .chain
            .electrum_url()
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let mut client = ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
        lwk_wollet::full_scan_with_electrum_client(&mut self.wollet, &mut client)
            .map_err(|e| Error::Electrum(e.to_string()))?;
        Ok(())
    }

    /// Get the genesis block hash for Simplicity operations.
    ///
    /// Returns the override if set (required for regtest), otherwise
    /// falls back to the hardcoded network genesis hash.
    pub fn chain_genesis_hash(&self) -> Result<[u8; 32]> {
        Ok(self
            .chain_genesis_override
            .unwrap_or_else(|| self.network.genesis_hash()))
    }

    /// Set the chain genesis hash for Simplicity admin operations.
    ///
    /// Required for regtest where each `elementsd` instance has a unique
    /// genesis hash. For Liquid and Liquid Testnet, the hardcoded values
    /// are correct.
    pub fn set_chain_genesis_hash(&mut self, hash: [u8; 32]) {
        self.chain_genesis_override = Some(hash);
    }

    pub fn balance(&self) -> Result<HashMap<AssetId, u64>> {
        let balance = self
            .wollet
            .balance()
            .map_err(|e| Error::Query(e.to_string()))?;
        Ok(balance.iter().map(|(k, v)| (*k, *v)).collect())
    }

    pub fn address(&self, index: Option<u32>) -> Result<lwk_wollet::AddressResult> {
        self.wollet
            .address(index)
            .map_err(|e| Error::Query(e.to_string()))
    }

    pub fn utxos(&self) -> Result<Vec<WalletTxOut>> {
        self.wollet.utxos().map_err(|e| Error::Query(e.to_string()))
    }

    pub fn transactions(&self) -> Result<Vec<WalletTx>> {
        self.wollet
            .transactions()
            .map_err(|e| Error::Query(e.to_string()))
    }

    pub fn sign_pset(&self, mut pset: PartiallySignedTransaction) -> Result<Transaction> {
        self.wollet
            .add_details(&mut pset)
            .map_err(|e| Error::Signer(format!("add_details: {}", e)))?;
        self.signer
            .sign(&mut pset)
            .map_err(|e| Error::Signer(format!("{:?}", e)))?;
        self.wollet
            .finalize(&mut pset)
            .map_err(|e| Error::Finalize(e.to_string()))
    }

    pub fn send_lbtc(
        &mut self,
        address_str: &str,
        amount_sat: u64,
        fee_rate: Option<f32>,
    ) -> Result<(Txid, u64)> {
        let address: lwk_wollet::elements::Address = address_str
            .parse()
            .map_err(|e| Error::Query(format!("invalid address: {}", e)))?;

        let pset = TxBuilder::new(self.network.into_lwk())
            .add_lbtc_recipient(&address, amount_sat)
            .map_err(|e| Error::Query(format!("add_lbtc_recipient: {}", e)))?
            .fee_rate(fee_rate)
            .finish(&self.wollet)
            .map_err(|e| Error::Query(format!("TxBuilder finish: {}", e)))?;

        let tx = self.sign_pset(pset)?;

        let fee_sat: u64 = tx
            .output
            .iter()
            .filter(|o| o.script_pubkey.is_empty())
            .map(|o| o.value.explicit().unwrap_or(0))
            .sum();

        let txid = self.broadcast_and_sync(&tx)?;
        Ok((txid, fee_sat))
    }

    pub fn broadcast_and_sync(&mut self, tx: &Transaction) -> Result<Txid> {
        let txid = self.chain.broadcast(tx)?;
        // Re-sync wallet after broadcast, retrying briefly if the electrum
        // server hasn't indexed the mempool tx yet.
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            self.sync()?;
            if self.transactions()?.iter().any(|t| t.txid == txid) {
                break;
            }
        }
        Ok(txid)
    }

    pub fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction> {
        self.chain.fetch_transaction(txid)
    }

    #[cfg_attr(not(any(test, feature = "testing")), allow(dead_code))]
    pub fn network(&self) -> Network {
        self.network
    }

    pub fn electrum_url(&self) -> &str {
        self.chain.electrum_url()
    }

    pub fn policy_asset(&self) -> AssetId {
        self.network.into_lwk().policy_asset()
    }

    pub(crate) fn create_lmsr_pool_bootstrap(
        &mut self,
        request: &CreateLmsrPoolRequest,
    ) -> Result<LmsrPoolSnapshot> {
        self.sync()?;
        validate_create_lmsr_pool_request(request)?;

        let contract = CompiledLmsrPool::new(request.pool_params)?;
        let change_addr: lwk_wollet::elements::Address = self
            .address(None)?
            .address()
            .to_string()
            .parse()
            .map_err(|e| Error::Query(format!("bad change address: {e}")))?;

        let mut exclude = Vec::new();
        let reserve_yes_inputs = self.collect_wallet_utxos_for_asset(
            &request.pool_params.yes_asset_id,
            request.initial_reserves.r_yes,
            &exclude,
        )?;
        exclude.extend(reserve_yes_inputs.iter().map(|utxo| utxo.outpoint));

        let reserve_no_inputs = self.collect_wallet_utxos_for_asset(
            &request.pool_params.no_asset_id,
            request.initial_reserves.r_no,
            &exclude,
        )?;
        exclude.extend(reserve_no_inputs.iter().map(|utxo| utxo.outpoint));

        let policy_asset = self.policy_asset();
        let policy_asset_bytes = policy_asset.into_inner().to_byte_array();
        let collateral_is_policy_asset =
            request.pool_params.collateral_asset_id == policy_asset_bytes;
        let reserve_collateral_target = if collateral_is_policy_asset {
            request
                .initial_reserves
                .r_lbtc
                .checked_add(request.fee_amount)
                .ok_or(Error::CollateralOverflow)?
        } else {
            request.initial_reserves.r_lbtc
        };
        let reserve_collateral_inputs = self.collect_wallet_utxos_for_asset(
            &request.pool_params.collateral_asset_id,
            reserve_collateral_target,
            &exclude,
        )?;
        exclude.extend(reserve_collateral_inputs.iter().map(|utxo| utxo.outpoint));

        let fee_inputs = if collateral_is_policy_asset {
            Vec::new()
        } else {
            select_wallet_utxo_set(
                &self.utxos()?,
                policy_asset,
                request.fee_amount,
                &exclude,
                &policy_asset_bytes,
            )?
            .into_iter()
            .map(|wallet_utxo| {
                let tx = self.fetch_transaction(&wallet_utxo.outpoint.txid)?;
                let txout = tx
                    .output
                    .get(wallet_utxo.outpoint.vout as usize)
                    .ok_or_else(|| Error::Query("fee UTXO vout out of range".into()))?
                    .clone();
                Ok(wallet_txout_to_unblinded(&wallet_utxo, &txout))
            })
            .collect::<Result<Vec<_>>>()?
        };

        let mut built = build_lmsr_bootstrap_pset(
            &contract,
            request.initial_s_index,
            request.initial_reserves,
            &reserve_yes_inputs,
            &reserve_no_inputs,
            &reserve_collateral_inputs,
            &fee_inputs,
            request.fee_amount,
            &change_addr.script_pubkey(),
            &policy_asset.into_inner().to_byte_array(),
        )?;

        if !built.blind_output_indices.is_empty() {
            self.blind_order_pset(
                &mut built.pset,
                &built.wallet_inputs,
                &built.blind_output_indices,
                &change_addr,
            )?;
        }

        let tx = self.sign_pset(built.pset)?;
        let txid = self.broadcast_and_sync(&tx)?;
        let creation_txid_bytes = txid_to_canonical_bytes(&txid).map_err(Error::LmsrPool)?;
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
        let pool_id = derive_lmsr_pool_id(
            self.network,
            request.pool_params,
            creation_txid_bytes,
            initial_reserve_outpoints,
        )
        .map_err(Error::LmsrPool)?;

        Ok(LmsrPoolSnapshot {
            locator: LmsrPoolLocator {
                market_id: request.market_params.market_id(),
                pool_id,
                params: request.pool_params,
                creation_txid: txid,
                initial_reserve_outpoints,
                hinted_s_index: request.initial_s_index,
                witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
            },
            current_s_index: request.initial_s_index,
            reserves: request.initial_reserves,
            current_reserve_outpoints: [
                OutPoint::new(txid, 0),
                OutPoint::new(txid, 1),
                OutPoint::new(txid, 2),
            ],
            last_transition_txid: None,
        })
    }

    // ── LMSR pool admin operations ────────────────────────────────────

    pub(crate) fn adjust_lmsr_pool(
        &mut self,
        request: &AdjustLmsrPoolRequest,
    ) -> Result<AdjustLmsrPoolResult> {
        self.sync()?;

        // Fetch the actual chain genesis hash for admin signature computation.
        // The Simplicity C runtime in elementsd uses the chain's real genesis
        // hash (via `uint256::data()` in LE byte order). For regtest, each
        // instance has a unique genesis hash that differs from LWK's hardcoded
        // constant.
        let chain_genesis = self.chain_genesis_hash()?;

        let params = request.locator.params;
        if !params.has_admin_cosigner() {
            return Err(Error::LmsrPool(
                "adjust_lmsr_pool requires a non-NUMS admin cosigner".into(),
            ));
        }
        params
            .validate()
            .map_err(|e| Error::LmsrPool(e.to_string()))?;
        let admin_keypair = self.derive_pool_admin_keypair(request.pool_index)?;
        let (admin_xonly, _) = admin_keypair.x_only_public_key();
        if admin_xonly.serialize() != params.cosigner_pubkey {
            return Err(Error::LmsrPool(
                "derived admin pubkey does not match pool cosigner_pubkey".into(),
            ));
        }
        if request.new_reserves.r_yes < params.min_r_yes
            || request.new_reserves.r_no < params.min_r_no
            || request.new_reserves.r_lbtc < params.min_r_collateral
        {
            return Err(Error::LmsrPool("new reserves below pool minimums".into()));
        }
        if request.current_pool_utxos.yes.value != request.current_reserves.r_yes
            || request.current_pool_utxos.no.value != request.current_reserves.r_no
            || request.current_pool_utxos.collateral.value != request.current_reserves.r_lbtc
        {
            return Err(Error::LmsrPool(
                "current_reserves must match current_pool_utxos values".into(),
            ));
        }
        let manifest = LmsrTableManifest::new(params.table_depth, request.table_values.clone())?;
        manifest.verify_matches_pool_params(&params)?;

        let contract = CompiledLmsrPool::new(params)?;
        let s_index = request.current_s_index;
        let reserve_spk = contract.script_pubkey(s_index);

        let change_addr: lwk_wollet::elements::Address = self
            .address(None)?
            .address()
            .to_string()
            .parse()
            .map_err(|e| Error::Query(format!("bad change address: {e}")))?;

        let policy_asset = self.policy_asset();
        let policy_bytes = policy_asset.into_inner().to_byte_array();
        let collateral_is_policy = params.collateral_asset_id == policy_bytes;

        // Build PSET: 3 reserve inputs + optional wallet inputs → 3 reserve outputs + change + fee
        let mut pset = crate::pset::new_pset();

        // Add 3 reserve inputs (in_base = 0)
        crate::pset::add_pset_input(&mut pset, &request.current_pool_utxos.yes);
        crate::pset::add_pset_input(&mut pset, &request.current_pool_utxos.no);
        crate::pset::add_pset_input(&mut pset, &request.current_pool_utxos.collateral);
        let in_base: u32 = 0;
        let out_base: u32 = 0;

        // 3 reserve outputs at same s_index
        crate::pset::add_pset_output(
            &mut pset,
            crate::pset::explicit_txout(
                &params.yes_asset_id,
                request.new_reserves.r_yes,
                &reserve_spk,
            ),
        );
        crate::pset::add_pset_output(
            &mut pset,
            crate::pset::explicit_txout(
                &params.no_asset_id,
                request.new_reserves.r_no,
                &reserve_spk,
            ),
        );
        crate::pset::add_pset_output(
            &mut pset,
            crate::pset::explicit_txout(
                &params.collateral_asset_id,
                request.new_reserves.r_lbtc,
                &reserve_spk,
            ),
        );

        // Collect any additional wallet inputs needed for adding liquidity
        let mut exclude: Vec<OutPoint> = vec![
            request.current_pool_utxos.yes.outpoint,
            request.current_pool_utxos.no.outpoint,
            request.current_pool_utxos.collateral.outpoint,
        ];
        let mut wallet_inputs: Vec<UnblindedUtxo> = Vec::new();

        // YES delta
        if request.new_reserves.r_yes > request.current_reserves.r_yes {
            let extra = request.new_reserves.r_yes - request.current_reserves.r_yes;
            let inputs =
                self.collect_wallet_utxos_for_asset(&params.yes_asset_id, extra, &exclude)?;
            exclude.extend(inputs.iter().map(|u| u.outpoint));
            for u in &inputs {
                crate::pset::add_pset_input(&mut pset, u);
            }
            wallet_inputs.extend(inputs);
        }
        // NO delta
        if request.new_reserves.r_no > request.current_reserves.r_no {
            let extra = request.new_reserves.r_no - request.current_reserves.r_no;
            let inputs =
                self.collect_wallet_utxos_for_asset(&params.no_asset_id, extra, &exclude)?;
            exclude.extend(inputs.iter().map(|u| u.outpoint));
            for u in &inputs {
                crate::pset::add_pset_input(&mut pset, u);
            }
            wallet_inputs.extend(inputs);
        }
        // Collateral delta.
        //
        // When collateral IS the policy asset, the fee can be absorbed from the
        // explicit reserve flow:
        //   - increase: wallet must provide (extra + fee) worth of policy asset.
        //   - decrease: the explicit surplus covers the fee, so we reduce the
        //     collateral change output by fee_amount and skip the wallet input.
        //   - neutral: wallet provides fee_amount.
        //
        // This avoids mixing confidential wallet inputs with an explicit fee
        // output which would cause `value in != value out` on Elements.
        let collateral_decrease = request
            .current_reserves
            .r_lbtc
            .saturating_sub(request.new_reserves.r_lbtc);
        let fee_absorbed_by_collateral_surplus =
            collateral_is_policy && collateral_decrease >= request.fee_amount;

        let collateral_extra_needed =
            if request.new_reserves.r_lbtc > request.current_reserves.r_lbtc {
                let extra = request.new_reserves.r_lbtc - request.current_reserves.r_lbtc;
                if collateral_is_policy {
                    extra + request.fee_amount
                } else {
                    extra
                }
            } else if collateral_is_policy && !fee_absorbed_by_collateral_surplus {
                // Neutral case or decrease too small to absorb fee
                request.fee_amount - collateral_decrease
            } else {
                0
            };
        if collateral_extra_needed > 0 {
            let inputs = self.collect_wallet_utxos_for_asset(
                &params.collateral_asset_id,
                collateral_extra_needed,
                &exclude,
            )?;
            exclude.extend(inputs.iter().map(|u| u.outpoint));
            for u in &inputs {
                crate::pset::add_pset_input(&mut pset, u);
            }
            wallet_inputs.extend(inputs);
        }
        // Fee inputs if collateral != policy asset
        if !collateral_is_policy {
            let fee_inputs =
                self.collect_wallet_utxos_for_asset(&policy_bytes, request.fee_amount, &exclude)?;
            for u in &fee_inputs {
                crate::pset::add_pset_input(&mut pset, u);
            }
            wallet_inputs.extend(fee_inputs);
        }

        // Fee output
        crate::pset::add_pset_output(
            &mut pset,
            crate::pset::explicit_txout(&policy_bytes, request.fee_amount, &Script::new()),
        );

        // Change outputs for surplus tokens returned to wallet.
        //
        // All change outputs are explicit (not blinded). The reserve inputs are
        // explicit on-chain, so reserve amounts are already public. Blinding the
        // change would require surjection proofs referencing confidential inputs,
        // but the reserve covenant inputs are explicit.
        let change_spk = change_addr.script_pubkey();

        // Reserve-surplus change (decreasing reserves → tokens flow to wallet)
        if request.current_reserves.r_yes > request.new_reserves.r_yes {
            let surplus = request.current_reserves.r_yes - request.new_reserves.r_yes;
            crate::pset::add_pset_output(
                &mut pset,
                crate::pset::explicit_txout(&params.yes_asset_id, surplus, &change_spk),
            );
        }
        if request.current_reserves.r_no > request.new_reserves.r_no {
            let surplus = request.current_reserves.r_no - request.new_reserves.r_no;
            crate::pset::add_pset_output(
                &mut pset,
                crate::pset::explicit_txout(&params.no_asset_id, surplus, &change_spk),
            );
        }
        if request.current_reserves.r_lbtc > request.new_reserves.r_lbtc {
            let surplus = request.current_reserves.r_lbtc - request.new_reserves.r_lbtc;
            // When the fee is absorbed from this explicit surplus, reduce by fee_amount.
            let net_surplus = if fee_absorbed_by_collateral_surplus {
                surplus - request.fee_amount
            } else {
                surplus
            };
            if net_surplus > 0 {
                crate::pset::add_pset_output(
                    &mut pset,
                    crate::pset::explicit_txout(
                        &params.collateral_asset_id,
                        net_surplus,
                        &change_spk,
                    ),
                );
            }
        }

        // Wallet-input surplus change (wallet provided more than needed).
        // These outputs are blinded because the wallet inputs are confidential
        // and Elements requires the value balance to hold with commitments.
        let mut blind_indices = Vec::new();
        {
            let mut wallet_totals: HashMap<[u8; 32], u64> = HashMap::new();
            for u in &wallet_inputs {
                *wallet_totals.entry(u.asset_id).or_insert(0) += u.value;
            }
            let mut wallet_needed_per_asset: HashMap<[u8; 32], u64> = HashMap::new();
            if request.new_reserves.r_yes > request.current_reserves.r_yes {
                let extra = request.new_reserves.r_yes - request.current_reserves.r_yes;
                *wallet_needed_per_asset
                    .entry(params.yes_asset_id)
                    .or_insert(0) += extra;
            }
            if request.new_reserves.r_no > request.current_reserves.r_no {
                let extra = request.new_reserves.r_no - request.current_reserves.r_no;
                *wallet_needed_per_asset
                    .entry(params.no_asset_id)
                    .or_insert(0) += extra;
            }
            if collateral_extra_needed > 0 {
                *wallet_needed_per_asset
                    .entry(params.collateral_asset_id)
                    .or_insert(0) += collateral_extra_needed;
            }
            if !collateral_is_policy {
                *wallet_needed_per_asset.entry(policy_bytes).or_insert(0) += request.fee_amount;
            }
            for (asset, total) in &wallet_totals {
                let needed = wallet_needed_per_asset.get(asset).copied().unwrap_or(0);
                if *total > needed {
                    let surplus = *total - needed;
                    let idx = pset.n_outputs();
                    crate::pset::add_pset_output(
                        &mut pset,
                        crate::pset::explicit_txout(asset, surplus, &change_spk),
                    );
                    blind_indices.push(idx);
                }
            }
        }

        // Blind wallet-surplus change outputs BEFORE computing the admin
        // signature, since sig_all_hash covers blinded output commitments.
        //
        // We can't use blind_order_pset here because it indexes wallet_inputs
        // from 0, but in the PSET, wallet inputs start at index 3 (after the
        // 3 reserve inputs). We build the secrets map manually with correct
        // PSET indices, including both reserve and wallet inputs.
        if !blind_indices.is_empty() {
            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| Error::Blinding("change address has no blinding key".into()))?;
            let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
                inner: blinding_pk,
                compressed: true,
            };
            for &idx in &blind_indices {
                pset.outputs_mut()[idx].blinding_key = Some(pset_blinding_key);
                pset.outputs_mut()[idx].blinder_index = Some(0);
            }

            let mut inp_txout_sec = HashMap::new();
            // Reserve inputs (explicit, at PSET indices 0-2)
            for (i, utxo) in [
                &request.current_pool_utxos.yes,
                &request.current_pool_utxos.no,
                &request.current_pool_utxos.collateral,
            ]
            .iter()
            .enumerate()
            {
                let asset_id = AssetId::from_slice(&utxo.asset_id)
                    .map_err(|e| Error::Blinding(format!("reserve input {i} asset: {e}")))?;
                inp_txout_sec.insert(i, txout_secrets_from_unblinded(utxo, asset_id)?);
            }
            // Wallet inputs (confidential, at PSET indices 3+)
            for (i, utxo) in wallet_inputs.iter().enumerate() {
                let pset_idx = 3 + i;
                let asset_id = AssetId::from_slice(&utxo.asset_id)
                    .map_err(|e| Error::Blinding(format!("wallet input {i} asset: {e}")))?;
                inp_txout_sec.insert(pset_idx, txout_secrets_from_unblinded(utxo, asset_id)?);
            }

            let secp = secp256k1_zkp::Secp256k1::new();
            let mut rng = rand::thread_rng();
            pset.blind_last(&mut rng, &secp, &inp_txout_sec)
                .map_err(|e| Error::Blinding(format!("{e:?}")))?;
        }

        // Compute admin signature
        let old_proof = manifest.proof_at(s_index)?;
        let admin_signature = compute_lmsr_admin_signature(
            &pset,
            &contract,
            &params,
            &request.current_pool_utxos,
            s_index,
            s_index,
            request.current_reserves,
            request.new_reserves,
            in_base,
            out_base,
            &admin_keypair,
            chain_genesis,
        )?;

        // Build swap leg for witness attachment
        let leg = LmsrPoolSwapLeg {
            primary_path: LmsrPrimaryPath::AdminAdjust,
            pool_params: params,
            pool_id: request.locator.pool_id.to_hex(),
            old_s_index: s_index,
            new_s_index: s_index,
            old_path_bits: old_proof.path_bits,
            new_path_bits: old_proof.path_bits,
            old_siblings: old_proof.siblings.clone(),
            new_siblings: old_proof.siblings,
            in_base,
            out_base,
            pool_utxos: request.current_pool_utxos.clone(),
            trade_kind: LmsrTradeKind::BuyYes, // unused for AdminAdjust
            old_f: manifest.value_at(s_index)?,
            new_f: manifest.value_at(s_index)?,
            delta_in: 0,
            delta_out: 0,
            admin_signature,
        };

        attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3, chain_genesis)?;

        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        let mut new_locator = request.locator.clone();
        new_locator.hinted_s_index = s_index;

        Ok(AdjustLmsrPoolResult {
            txid,
            new_snapshot: LmsrPoolSnapshot {
                locator: new_locator,
                current_s_index: s_index,
                reserves: request.new_reserves,
                current_reserve_outpoints: [
                    OutPoint::new(txid, 0),
                    OutPoint::new(txid, 1),
                    OutPoint::new(txid, 2),
                ],
                last_transition_txid: Some(txid),
            },
        })
    }

    pub(crate) fn close_lmsr_pool(
        &mut self,
        request: &CloseLmsrPoolRequest,
    ) -> Result<CloseLmsrPoolResult> {
        let params = request.locator.params;
        let min_reserves = PoolReserves {
            r_yes: params.min_r_yes,
            r_no: params.min_r_no,
            r_lbtc: params.min_r_collateral,
        };
        let adjust_request = AdjustLmsrPoolRequest {
            locator: request.locator.clone(),
            current_pool_utxos: request.current_pool_utxos.clone(),
            current_s_index: request.current_s_index,
            current_reserves: request.current_reserves,
            new_reserves: min_reserves,
            table_values: request.table_values.clone(),
            fee_amount: request.fee_amount,
            pool_index: request.pool_index,
        };
        let result = self.adjust_lmsr_pool(&adjust_request)?;
        Ok(CloseLmsrPoolResult {
            txid: result.txid,
            reclaimed_yes: request
                .current_reserves
                .r_yes
                .saturating_sub(min_reserves.r_yes),
            reclaimed_no: request
                .current_reserves
                .r_no
                .saturating_sub(min_reserves.r_no),
            reclaimed_collateral: request
                .current_reserves
                .r_lbtc
                .saturating_sub(min_reserves.r_lbtc),
        })
    }

    // ── Boltz key derivation ─────────────────────────────────────────────

    pub fn boltz_submarine_refund_pubkey_hex(&self) -> Result<String> {
        let network_path = if self.network.is_mainnet() { 1776 } else { 1 };
        self.derive_boltz_pubkey_hex(format!("m/49'/{network_path}'/21'/0/0"))
    }

    pub fn boltz_reverse_claim_pubkey_hex(&self) -> Result<String> {
        let network_path = if self.network.is_mainnet() { 1776 } else { 1 };
        self.derive_boltz_pubkey_hex(format!("m/84'/{network_path}'/42'/0/0"))
    }

    fn derive_boltz_pubkey_hex(&self, path_str: String) -> Result<String> {
        let path: lwk_wollet::bitcoin::bip32::DerivationPath = path_str
            .parse()
            .map_err(|e| Error::Signer(format!("{}", e)))?;
        let derived = self
            .signer
            .derive_xprv(&path)
            .map_err(|e| Error::Signer(format!("{:?}", e)))?;
        let secp = secp256k1_zkp::Secp256k1::new();
        let secret = secp256k1_zkp::SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|e| Error::Signer(format!("{}", e)))?;
        let keypair = Keypair::from_secret_key(&secp, &secret);
        Ok(keypair.public_key().to_string())
    }

    // ── On-chain contract creation ───────────────────────────────────────

    pub fn create_contract_onchain(
        &mut self,
        oracle_public_key: [u8; 32],
        collateral_per_token: u64,
        expiry_time: u32,
        min_utxo_value: u64,
        fee_amount: u64,
    ) -> Result<(PredictionMarketAnchor, PredictionMarketParams)> {
        self.sync()?;

        let raw_utxos = self.utxos()?;
        let policy_asset = self.policy_asset();
        let policy_bytes: [u8; 32] = policy_asset.into_inner().to_byte_array();

        let (yes_utxo, no_utxo) = select_defining_utxos(&raw_utxos, policy_asset, min_utxo_value)?;

        let yes_tx = self.fetch_transaction(&yes_utxo.outpoint.txid)?;
        let no_tx = self.fetch_transaction(&no_utxo.outpoint.txid)?;

        let yes_txout = yes_tx
            .output
            .get(yes_utxo.outpoint.vout as usize)
            .ok_or_else(|| Error::Query("YES UTXO vout out of range".to_string()))?
            .clone();
        let no_txout = no_tx
            .output
            .get(no_utxo.outpoint.vout as usize)
            .ok_or_else(|| Error::Query("NO UTXO vout out of range".to_string()))?
            .clone();

        let addr_result = self.address(None)?;
        let change_addr: lwk_wollet::elements::Address = addr_result
            .address()
            .to_string()
            .parse()
            .map_err(|e| Error::Query(format!("bad change address: {}", e)))?;

        // Compile contract — types are identical, no bridging needed
        let contract = CompiledPredictionMarket::create(
            oracle_public_key,
            policy_bytes,
            collateral_per_token,
            expiry_time,
            yes_utxo.outpoint,
            no_utxo.outpoint,
        )?;

        let yes_unblinded = wallet_txout_to_unblinded(&yes_utxo, &yes_txout);
        let no_unblinded = wallet_txout_to_unblinded(&no_utxo, &no_txout);

        let mut sdk_pset = build_creation_pset(
            &contract,
            &CreationParams {
                yes_defining_utxo: yes_unblinded,
                no_defining_utxo: no_unblinded,
                fee_amount,
                change_destination: Some(change_addr.script_pubkey()),
                lock_time: 0,
            },
        )?;

        // Blind the dormant RT outputs and any wallet change output.
        {
            let outputs = sdk_pset.outputs_mut();
            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;
            let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
                inner: blinding_pk,
                compressed: true,
            };
            let yes_rt_id = AssetId::from_slice(&contract.params().yes_reissuance_token)
                .map_err(|e| Error::Blinding(format!("bad YES reissuance asset: {e}")))?;
            let no_rt_id = AssetId::from_slice(&contract.params().no_reissuance_token)
                .map_err(|e| Error::Blinding(format!("bad NO reissuance asset: {e}")))?;

            outputs[0].amount = Some(1);
            outputs[0].asset = Some(yes_rt_id);
            outputs[1].amount = Some(1);
            outputs[1].asset = Some(no_rt_id);

            for idx in [0usize, 1] {
                outputs[idx].blinding_key = Some(pset_blinding_key);
                outputs[idx].blinder_index = Some(0);
            }
            for output in outputs.iter_mut().skip(3) {
                output.blinding_key = Some(pset_blinding_key);
                output.blinder_index = Some(0);
            }

            let inputs = sdk_pset.inputs_mut();
            inputs[0].blinded_issuance = Some(0x00);
            inputs[1].blinded_issuance = Some(0x00);

            let mut inp_txout_sec = HashMap::new();
            inp_txout_sec.insert(0usize, yes_utxo.unblinded);
            inp_txout_sec.insert(1usize, no_utxo.unblinded);

            let secp = lwk_wollet::elements::secp256k1_zkp::Secp256k1::new();
            let mut rng = thread_rng();
            sdk_pset
                .blind_last(&mut rng, &secp, &inp_txout_sec)
                .map_err(|e| Error::Blinding(format!("{e:?}")))?;
        }

        let tx = self.sign_pset(sdk_pset)?;
        let txid = self.broadcast_and_sync(&tx)?;
        let params = *contract.params();
        let master_blinding_key = self
            .signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Blinding(format!("slip77 key: {e}")))?;
        let anchor = recover_creation_anchor(&tx, txid, &master_blinding_key, &change_addr)?;

        Ok((anchor, params))
    }

    // ── Token issuance ──────────────────────────────────────────────────

    /// Issue prediction market token pairs.
    ///
    /// Detects whether the market is in Dormant (initial issuance) or Unresolved
    /// (subsequent issuance) state and builds the appropriate transaction.
    pub fn issue_tokens(
        &mut self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<IssuanceResult> {
        let contract = CompiledPredictionMarket::new(*params)?;

        // A. Scan market state
        let (current_state, covenant_utxos) = self.scan_market_state(&contract, anchor)?;

        // B. Classify and unblind covenant UTXOs
        let (yes_rt, no_rt, collateral_covenant_utxo) =
            self.classify_covenant_utxos(&covenant_utxos, params, current_state)?;

        // C. Compute issuance entropy
        let parsed_anchor = parse_prediction_market_anchor(anchor).map_err(Error::Query)?;
        let creation_tx = self.fetch_transaction(&parsed_anchor.creation_txid)?;
        let issuance_entropy = compute_issuance_entropy(
            &creation_tx,
            &yes_rt.asset_blinding_factor,
            &no_rt.asset_blinding_factor,
        )?;

        // D. Select wallet UTXOs for collateral + fee
        let (collateral_unblinded, fee_unblinded, change_addr) =
            self.select_wallet_utxos(params, pairs, fee_amount)?;

        let change_spk = change_addr.script_pubkey();

        let collateral_source = match current_state {
            MarketState::Dormant => CollateralSource::Initial {
                wallet_utxo: collateral_unblinded,
            },
            MarketState::Unresolved => {
                let cov_collateral = collateral_covenant_utxo.ok_or_else(|| {
                    Error::CovenantScan("collateral UTXO not found at covenant".into())
                })?;
                CollateralSource::Subsequent {
                    covenant_collateral: cov_collateral,
                    new_wallet_utxo: collateral_unblinded,
                }
            }
            other => return Err(Error::NotIssuable(other)),
        };
        // E-H. Assemble issuance (build PSET → blind → recover factors → attach witnesses)
        let blinding_pk = change_addr
            .blinding_pubkey
            .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;

        let master_blinding_key = self
            .signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Blinding(format!("slip77 key: {e}")))?;

        let assembled = assemble_issuance(
            IssuanceAssemblyInputs {
                contract,
                current_state,
                yes_reissuance_utxo: yes_rt,
                no_reissuance_utxo: no_rt,
                collateral_source,
                fee_utxo: fee_unblinded,
                pairs,
                fee_amount,
                token_destination: change_spk.clone(),
                change_destination: Some(change_spk.clone()),
                issuance_entropy,
                lock_time: 0,
            },
            &master_blinding_key,
            blinding_pk,
            &change_spk,
        )?;

        // I. Sign and finalize
        let tx = self.sign_pset(assembled)?;

        // J. Broadcast
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(IssuanceResult {
            txid,
            previous_state: current_state,
            new_state: MarketState::Unresolved,
            pairs_issued: pairs,
        })
    }

    /// Scan the canonical prediction-market lineage from the proof-carrying dormant anchor to determine the
    /// current on-chain lifecycle state and live canonical covenant UTXOs.
    pub(crate) fn scan_market_state(
        &self,
        contract: &CompiledPredictionMarket,
        anchor: &PredictionMarketAnchor,
    ) -> Result<(MarketState, Vec<(OutPoint, TxOut)>)> {
        let backend = SdkPredictionMarketScanBackend { sdk: self };
        let scan = scan_prediction_market_canonical(&backend, contract.params(), anchor)
            .map_err(Error::CovenantScan)?;
        Ok((
            scan.state,
            scan.utxos
                .into_iter()
                .map(|utxo| (utxo.outpoint, utxo.txout))
                .collect(),
        ))
    }

    /// Classify and unblind covenant UTXOs into YES RT, NO RT, and optional collateral.
    fn classify_covenant_utxos(
        &self,
        covenant_utxos: &[(OutPoint, TxOut)],
        params: &PredictionMarketParams,
        _current_state: MarketState,
    ) -> Result<(UnblindedUtxo, UnblindedUtxo, Option<UnblindedUtxo>)> {
        let yes_rt_id = AssetId::from_slice(&params.yes_reissuance_token)
            .map_err(|e| Error::Unblind(format!("bad YES reissuance asset: {e}")))?;
        let no_rt_id = AssetId::from_slice(&params.no_reissuance_token)
            .map_err(|e| Error::Unblind(format!("bad NO reissuance asset: {e}")))?;
        let collateral_id = AssetId::from_slice(&params.collateral_asset_id)
            .map_err(|e| Error::Unblind(format!("bad collateral asset: {e}")))?;

        let mut yes_rt_utxo: Option<UnblindedUtxo> = None;
        let mut no_rt_utxo: Option<UnblindedUtxo> = None;
        let mut collateral_covenant_utxo: Option<UnblindedUtxo> = None;

        for (outpoint, txout) in covenant_utxos {
            match txout.asset {
                Asset::Explicit(asset) => {
                    let value = txout.value.explicit().unwrap_or(0);
                    let utxo = UnblindedUtxo {
                        outpoint: *outpoint,
                        txout: txout.clone(),
                        asset_id: asset.into_inner().to_byte_array(),
                        value,
                        asset_blinding_factor: [0u8; 32],
                        value_blinding_factor: [0u8; 32],
                    };

                    if asset == collateral_id {
                        collateral_covenant_utxo = Some(utxo);
                    } else if asset == yes_rt_id {
                        yes_rt_utxo = Some(utxo);
                    } else if asset == no_rt_id {
                        no_rt_utxo = Some(utxo);
                    }
                }
                Asset::Confidential(_) => {
                    let (asset, value, abf, vbf) = self.unblind_covenant_utxo(txout)?;
                    let utxo = UnblindedUtxo {
                        outpoint: *outpoint,
                        txout: txout.clone(),
                        asset_id: asset.into_inner().to_byte_array(),
                        value,
                        asset_blinding_factor: abf,
                        value_blinding_factor: vbf,
                    };
                    if asset == yes_rt_id {
                        yes_rt_utxo = Some(utxo);
                    } else if asset == no_rt_id {
                        no_rt_utxo = Some(utxo);
                    } else if asset == collateral_id {
                        collateral_covenant_utxo = Some(utxo);
                    }
                }
                _ => {}
            }
        }

        let yes_rt = yes_rt_utxo
            .ok_or_else(|| Error::CovenantScan("YES reissuance token not found".into()))?;
        let no_rt = no_rt_utxo
            .ok_or_else(|| Error::CovenantScan("NO reissuance token not found".into()))?;

        Ok((yes_rt, no_rt, collateral_covenant_utxo))
    }

    /// Select wallet UTXOs for collateral and fee, returning unblinded UTXOs and change address.
    fn select_wallet_utxos(
        &mut self,
        params: &PredictionMarketParams,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<(UnblindedUtxo, UnblindedUtxo, lwk_wollet::elements::Address)> {
        self.sync()?;
        let cpt = params.collateral_per_token;
        let required_collateral = pairs
            .checked_mul(2)
            .and_then(|v| v.checked_mul(cpt))
            .ok_or(Error::CollateralOverflow)?;

        let policy_asset = self.policy_asset();
        let raw_utxos = self.utxos()?;

        let collateral_wallet_utxo = raw_utxos
            .iter()
            .filter(|u| {
                !u.is_spent
                    && u.unblinded.asset == policy_asset
                    && u.unblinded.value >= required_collateral
            })
            .max_by_key(|u| u.unblinded.value)
            .ok_or_else(|| {
                Error::InsufficientUtxos(format!(
                    "need L-BTC UTXO with >= {} sats for collateral",
                    required_collateral
                ))
            })?
            .clone();

        let fee_wallet_utxo = raw_utxos
            .iter()
            .filter(|u| {
                !u.is_spent
                    && u.unblinded.asset == policy_asset
                    && u.unblinded.value >= fee_amount
                    && u.outpoint != collateral_wallet_utxo.outpoint
            })
            .min_by_key(|u| u.unblinded.value)
            .ok_or_else(|| {
                Error::InsufficientUtxos(format!(
                    "need a second L-BTC UTXO with >= {} sats for the fee \
                     (send yourself a small amount first to create another UTXO)",
                    fee_amount
                ))
            })?
            .clone();

        let collateral_tx = self.fetch_transaction(&collateral_wallet_utxo.outpoint.txid)?;
        let collateral_txout = collateral_tx
            .output
            .get(collateral_wallet_utxo.outpoint.vout as usize)
            .ok_or_else(|| Error::Query("collateral UTXO vout out of range".into()))?
            .clone();
        let fee_tx = self.fetch_transaction(&fee_wallet_utxo.outpoint.txid)?;
        let fee_txout = fee_tx
            .output
            .get(fee_wallet_utxo.outpoint.vout as usize)
            .ok_or_else(|| Error::Query("fee UTXO vout out of range".into()))?
            .clone();

        let collateral_unblinded =
            wallet_txout_to_unblinded(&collateral_wallet_utxo, &collateral_txout);
        let fee_unblinded = wallet_txout_to_unblinded(&fee_wallet_utxo, &fee_txout);

        let addr_result = self.address(None)?;
        let change_addr: lwk_wollet::elements::Address = addr_result
            .address()
            .to_string()
            .parse()
            .map_err(|e| Error::Query(format!("bad change address: {}", e)))?;

        Ok((collateral_unblinded, fee_unblinded, change_addr))
    }

    // ── Token cancellation ───────────────────────────────────────────────

    /// Cancel token pairs by burning equal YES and NO tokens to reclaim collateral.
    ///
    /// If all collateral is burned (full cancellation), the market transitions
    /// back to Dormant. Otherwise it stays Unresolved (partial cancellation).
    pub fn cancel_tokens(
        &mut self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
        pairs_to_burn: u64,
        fee_amount: u64,
    ) -> Result<CancellationResult> {
        self.sync()?;
        let contract = CompiledPredictionMarket::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract, anchor)?;
        if current_state != MarketState::Unresolved {
            return Err(Error::NotCancellable(current_state));
        }

        let (yes_rt, no_rt, collateral_covenant_utxo) =
            self.classify_covenant_utxos(&covenant_utxos, params, current_state)?;

        let collateral = collateral_covenant_utxo
            .ok_or_else(|| Error::CovenantScan("collateral UTXO not found at covenant".into()))?;

        let cpt = params.collateral_per_token;
        let refund = pairs_to_burn
            .checked_mul(2)
            .and_then(|v| v.checked_mul(cpt))
            .ok_or(Error::CollateralOverflow)?;
        let is_full = collateral.value == refund;

        // Find YES and NO token UTXOs in wallet
        let (yes_token_utxos, no_token_utxos) = self.find_token_utxos_for_burn(
            &params.yes_token_asset,
            &params.no_token_asset,
            pairs_to_burn,
        )?;

        let (fee_unblinded, change_addr) = self.select_fee_utxo(fee_amount)?;
        let change_spk = change_addr.script_pubkey();

        let cancellation_params = CancellationParams {
            collateral_utxo: collateral,
            yes_reissuance_utxo: if is_full { Some(yes_rt.clone()) } else { None },
            no_reissuance_utxo: if is_full { Some(no_rt.clone()) } else { None },
            yes_token_utxos,
            no_token_utxos,
            fee_utxo: fee_unblinded,
            pairs_burned: pairs_to_burn,
            fee_amount,
            refund_destination: change_spk.clone(),
            fee_change_destination: Some(change_spk.clone()),
            token_change_destination: Some(change_spk.clone()),
        };

        let blinding_pk = change_addr
            .blinding_pubkey
            .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;

        let master_blinding_key = self
            .signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Blinding(format!("slip77 key: {e}")))?;

        let assembled = assemble_cancellation(
            &contract,
            &cancellation_params,
            &master_blinding_key,
            blinding_pk,
            &change_spk,
        )?;

        let tx = self.sign_pset(assembled)?;
        let txid = self.broadcast_and_sync(&tx)?;

        let new_state = if is_full {
            MarketState::Dormant
        } else {
            MarketState::Unresolved
        };

        Ok(CancellationResult {
            txid,
            previous_state: current_state,
            new_state,
            pairs_burned: pairs_to_burn,
            is_full_cancellation: is_full,
        })
    }

    // ── Oracle resolution ────────────────────────────────────────────────

    /// Resolve a market with an oracle signature.
    ///
    /// Transitions the market from Unresolved to ResolvedYes or ResolvedNo.
    pub fn resolve_market(
        &mut self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
        outcome_yes: bool,
        oracle_signature: [u8; 64],
        fee_amount: u64,
    ) -> Result<ResolutionResult> {
        self.sync()?;
        let contract = CompiledPredictionMarket::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract, anchor)?;
        if current_state != MarketState::Unresolved {
            return Err(Error::NotResolvable(current_state));
        }

        let (yes_rt, no_rt, collateral_covenant_utxo) =
            self.classify_covenant_utxos(&covenant_utxos, params, current_state)?;

        let collateral = collateral_covenant_utxo
            .ok_or_else(|| Error::CovenantScan("collateral UTXO not found at covenant".into()))?;

        let (fee_unblinded, change_addr) = self.select_fee_utxo(fee_amount)?;
        let change_spk = change_addr.script_pubkey();

        let resolve_params = OracleResolveParams {
            yes_reissuance_utxo: yes_rt.clone(),
            no_reissuance_utxo: no_rt.clone(),
            collateral_utxo: collateral,
            fee_amount,
            fee_change_destination: Some(change_spk.clone()),
            fee_utxo: fee_unblinded,
            outcome_yes,
            lock_time: 0,
        };

        let blinding_pk = change_addr
            .blinding_pubkey
            .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;

        let master_blinding_key = self
            .signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Blinding(format!("slip77 key: {e}")))?;

        let assembled = assemble_oracle_resolve(
            &contract,
            &resolve_params,
            oracle_signature,
            &master_blinding_key,
            blinding_pk,
            &change_spk,
            &yes_rt,
            &no_rt,
        )?;

        let txid = self.broadcast_and_sync(&self.sign_pset(assembled)?)?;

        let new_state = if outcome_yes {
            MarketState::ResolvedYes
        } else {
            MarketState::ResolvedNo
        };

        Ok(ResolutionResult {
            txid,
            previous_state: current_state,
            new_state,
            outcome_yes,
        })
    }

    // ── Post-resolution redemption ───────────────────────────────────────

    /// Redeem winning tokens after oracle resolution.
    ///
    /// Burns winning tokens and reclaims 2x collateral_per_token per token.
    pub fn redeem_tokens(
        &mut self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
        tokens_to_burn: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult> {
        self.sync()?;
        let contract = CompiledPredictionMarket::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract, anchor)?;
        if !current_state.is_resolved() {
            return Err(Error::NotRedeemable(current_state));
        }

        let winning_asset = current_state
            .winning_token_asset(params)
            .ok_or(Error::InvalidState)?;

        let collateral = Self::find_collateral_utxo(&covenant_utxos, params)?;

        let cpt = params.collateral_per_token;
        let payout = tokens_to_burn
            .checked_mul(2)
            .and_then(|v| v.checked_mul(cpt))
            .ok_or(Error::CollateralOverflow)?;

        // Find winning token UTXOs in wallet
        let token_utxos = self.find_single_token_utxos(&winning_asset, tokens_to_burn)?;

        let (fee_unblinded, change_addr) = self.select_fee_utxo(fee_amount)?;
        let change_spk = change_addr.script_pubkey();

        let redemption_params = PostResolutionRedemptionParams {
            collateral_utxo: collateral,
            token_utxos,
            fee_utxo: fee_unblinded,
            tokens_burned: tokens_to_burn,
            resolved_state: current_state,
            fee_amount,
            payout_destination: change_spk.clone(),
            fee_change_destination: Some(change_spk.clone()),
            token_change_destination: Some(change_spk),
        };

        let blinding_pk = change_addr
            .blinding_pubkey
            .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;

        let assembled =
            assemble_post_resolution_redemption(&contract, &redemption_params, blinding_pk)?;

        let tx = self.sign_pset(assembled)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(RedemptionResult {
            txid,
            previous_state: current_state,
            tokens_redeemed: tokens_to_burn,
            payout_sats: payout,
        })
    }

    // ── Expiry redemption ────────────────────────────────────────────────

    /// Permissionlessly finalize an unresolved market into the explicit Expired state.
    fn expire_market(
        &mut self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
        fee_amount: u64,
    ) -> Result<Txid> {
        self.sync()?;
        let contract = CompiledPredictionMarket::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract, anchor)?;
        if current_state != MarketState::Unresolved {
            return Err(Error::NotRedeemable(current_state));
        }

        let (yes_rt, no_rt, collateral_covenant_utxo) =
            self.classify_covenant_utxos(&covenant_utxos, params, current_state)?;

        let collateral = collateral_covenant_utxo
            .ok_or_else(|| Error::CovenantScan("collateral UTXO not found at covenant".into()))?;

        let (fee_unblinded, change_addr) = self.select_fee_utxo(fee_amount)?;
        let change_spk = change_addr.script_pubkey();

        let expire_params = ExpireTransitionParams {
            yes_reissuance_utxo: yes_rt.clone(),
            no_reissuance_utxo: no_rt.clone(),
            collateral_utxo: collateral,
            fee_amount,
            fee_change_destination: Some(change_spk.clone()),
            fee_utxo: fee_unblinded,
            lock_time: params.expiry_time,
        };

        let blinding_pk = change_addr
            .blinding_pubkey
            .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;

        let master_blinding_key = self
            .signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Blinding(format!("slip77 key: {e}")))?;

        let assembled = assemble_expire_transition(
            &contract,
            &expire_params,
            &master_blinding_key,
            blinding_pk,
            &change_spk,
            &yes_rt,
            &no_rt,
        )?;

        self.broadcast_and_sync(&self.sign_pset(assembled)?)
    }

    /// Redeem tokens after market expiry (no oracle resolution).
    ///
    /// Burns tokens and reclaims 1x collateral_per_token per token. If the market
    /// is still Unresolved, this auto-finalizes Unresolved -> Expired first.
    pub fn redeem_expired(
        &mut self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
        token_asset: [u8; 32],
        tokens_to_burn: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult> {
        self.sync()?;
        let contract = CompiledPredictionMarket::new(*params)?;

        let (mut current_state, mut covenant_utxos) = self.scan_market_state(&contract, anchor)?;
        let mut finalize_txid: Option<Txid> = None;

        if current_state == MarketState::Unresolved {
            let txid = self.expire_market(params, anchor, fee_amount)?;
            finalize_txid = Some(txid);
            self.sync()?;
            let (rescanned_state, rescanned_utxos) = self.scan_market_state(&contract, anchor)?;
            current_state = rescanned_state;
            covenant_utxos = rescanned_utxos;
        }

        if current_state != MarketState::Expired {
            return Err(Error::NotRedeemable(current_state));
        }

        let redemption = (|| -> Result<RedemptionResult> {
            let collateral = Self::find_collateral_utxo(&covenant_utxos, params)?;

            let cpt = params.collateral_per_token;
            let payout = tokens_to_burn
                .checked_mul(cpt)
                .ok_or(Error::CollateralOverflow)?;

            let token_utxos = self.find_single_token_utxos(&token_asset, tokens_to_burn)?;

            let (fee_unblinded, change_addr) = self.select_fee_utxo(fee_amount)?;
            let change_spk = change_addr.script_pubkey();

            let expiry_params = ExpiryRedemptionParams {
                collateral_utxo: collateral,
                token_utxos,
                fee_utxo: fee_unblinded,
                tokens_burned: tokens_to_burn,
                burn_token_asset: token_asset,
                fee_amount,
                payout_destination: change_spk.clone(),
                fee_change_destination: Some(change_spk.clone()),
                token_change_destination: Some(change_spk),
                lock_time: params.expiry_time,
            };

            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;

            let assembled = assemble_expiry_redemption(&contract, &expiry_params, blinding_pk)?;

            let tx = self.sign_pset(assembled)?;
            let txid = self.broadcast_and_sync(&tx)?;

            Ok(RedemptionResult {
                txid,
                previous_state: current_state,
                tokens_redeemed: tokens_to_burn,
                payout_sats: payout,
            })
        })();

        match redemption {
            Ok(result) => Ok(result),
            Err(e) => {
                if let Some(finalized) = finalize_txid {
                    Err(Error::ExpiryFinalizeThenRedeemFailed {
                        finalize_txid: finalized.to_string(),
                        reason: e.to_string(),
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    // ── Maker order key derivation ─────────────────────────────────────

    /// Derive a secp256k1 keypair for maker orders at the given index.
    ///
    /// Path: `m/86'/{network}'/1'/0/{order_index}` where `1'` = maker order account.
    fn derive_maker_keypair(&self, order_index: u32) -> Result<Keypair> {
        let network_path = if self.network.is_mainnet() { 1776 } else { 1 };
        let path_str = format!("m/86'/{network_path}'/1'/0/{order_index}");
        let path: lwk_wollet::bitcoin::bip32::DerivationPath = path_str
            .parse()
            .map_err(|e| Error::Signer(format!("{}", e)))?;
        let derived = self
            .signer
            .derive_xprv(&path)
            .map_err(|e| Error::Signer(format!("{:?}", e)))?;
        let secp = secp256k1_zkp::Secp256k1::new();
        let secret = secp256k1_zkp::SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|e| Error::Signer(format!("{}", e)))?;
        Ok(Keypair::from_secret_key(&secp, &secret))
    }

    // ── Pool admin key derivation ────────────────────────────────────

    /// Derive a secp256k1 keypair for LMSR pool admin at the given index.
    ///
    /// Path: `m/86'/{network}'/2'/0/{pool_index}` where `2'` = LMSR pool admin account.
    fn derive_pool_admin_keypair(&self, pool_index: u32) -> Result<Keypair> {
        let network_path = if self.network.is_mainnet() { 1776 } else { 1 };
        let path_str = format!("m/86'/{network_path}'/2'/0/{pool_index}");
        let path: lwk_wollet::bitcoin::bip32::DerivationPath = path_str
            .parse()
            .map_err(|e| Error::Signer(format!("{}", e)))?;
        let derived = self
            .signer
            .derive_xprv(&path)
            .map_err(|e| Error::Signer(format!("{:?}", e)))?;
        let secp = secp256k1_zkp::Secp256k1::new();
        let secret = secp256k1_zkp::SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|e| Error::Signer(format!("{}", e)))?;
        Ok(Keypair::from_secret_key(&secp, &secret))
    }

    /// Get the x-only public key for LMSR pool admin at the given index.
    pub fn pool_admin_pubkey(&self, pool_index: u32) -> Result<[u8; 32]> {
        let keypair = self.derive_pool_admin_keypair(pool_index)?;
        let (xonly, _parity) = keypair.x_only_public_key();
        Ok(xonly.serialize())
    }

    fn collect_wallet_utxos_for_asset(
        &self,
        asset_id: &[u8; 32],
        required_amount: u64,
        exclude: &[OutPoint],
    ) -> Result<Vec<UnblindedUtxo>> {
        let target_asset = AssetId::from_slice(asset_id)
            .map_err(|e| Error::Query(format!("bad asset id: {e}")))?;
        let raw_utxos = self.utxos()?;
        let selected =
            select_wallet_utxo_set(&raw_utxos, target_asset, required_amount, exclude, asset_id)?;

        selected
            .into_iter()
            .map(|wallet_utxo| {
                let tx = self.fetch_transaction(&wallet_utxo.outpoint.txid)?;
                let txout = tx
                    .output
                    .get(wallet_utxo.outpoint.vout as usize)
                    .ok_or_else(|| Error::Query("funding UTXO vout out of range".into()))?
                    .clone();
                Ok(wallet_txout_to_unblinded(&wallet_utxo, &txout))
            })
            .collect()
    }

    /// Select a wallet UTXO for a specific asset with enough value, excluding certain outpoints.
    fn select_funding_utxo(
        &self,
        asset_id: &[u8; 32],
        required_amount: u64,
        exclude: &[OutPoint],
    ) -> Result<UnblindedUtxo> {
        let target_asset = AssetId::from_slice(asset_id)
            .map_err(|e| Error::Query(format!("bad asset id: {e}")))?;
        let raw_utxos = self.utxos()?;

        let wallet_utxo = raw_utxos
            .iter()
            .filter(|u| {
                !u.is_spent
                    && u.unblinded.asset == target_asset
                    && u.unblinded.value >= required_amount
                    && !exclude.contains(&u.outpoint)
            })
            .min_by_key(|u| u.unblinded.value)
            .ok_or_else(|| {
                Error::InsufficientUtxos(format!(
                    "need UTXO of asset {} with >= {} (excluding {} outpoints)",
                    hex::encode(asset_id),
                    required_amount,
                    exclude.len()
                ))
            })?
            .clone();

        let tx = self.fetch_transaction(&wallet_utxo.outpoint.txid)?;
        let txout = tx
            .output
            .get(wallet_utxo.outpoint.vout as usize)
            .ok_or_else(|| Error::Query("funding UTXO vout out of range".into()))?
            .clone();

        Ok(wallet_txout_to_unblinded(&wallet_utxo, &txout))
    }

    /// Select a fee UTXO excluding certain outpoints, returning it with a change address.
    fn select_fee_utxo_excluding(
        &mut self,
        fee_amount: u64,
        exclude: &[OutPoint],
    ) -> Result<(UnblindedUtxo, lwk_wollet::elements::Address)> {
        let policy_asset = self.policy_asset();
        let raw_utxos = self.utxos()?;

        let fee_wallet_utxo = raw_utxos
            .iter()
            .filter(|u| {
                !u.is_spent
                    && u.unblinded.asset == policy_asset
                    && u.unblinded.value >= fee_amount
                    && !exclude.contains(&u.outpoint)
            })
            .min_by_key(|u| u.unblinded.value)
            .ok_or_else(|| {
                Error::InsufficientUtxos(format!(
                    "need an L-BTC UTXO with >= {} sats for the fee (excluding {} outpoints)",
                    fee_amount,
                    exclude.len()
                ))
            })?
            .clone();

        let fee_tx = self.fetch_transaction(&fee_wallet_utxo.outpoint.txid)?;
        let fee_txout = fee_tx
            .output
            .get(fee_wallet_utxo.outpoint.vout as usize)
            .ok_or_else(|| Error::Query("fee UTXO vout out of range".into()))?
            .clone();

        let fee_unblinded = wallet_txout_to_unblinded(&fee_wallet_utxo, &fee_txout);

        let addr_result = self.address(None)?;
        let change_addr: lwk_wollet::elements::Address = addr_result
            .address()
            .to_string()
            .parse()
            .map_err(|e| Error::Query(format!("bad change address: {}", e)))?;

        Ok((fee_unblinded, change_addr))
    }

    // ── Limit order blinding helper ────────────────────────────────────

    /// Blind wallet-destination outputs in a maker order PSET.
    ///
    /// `wallet_inputs`: the wallet UTXOs used as PSET inputs (in order).
    /// `first_wallet_output`: index of the first output to blind (all subsequent
    ///   non-fee outputs are also blinded).
    fn blind_order_pset(
        &self,
        pset: &mut PartiallySignedTransaction,
        wallet_inputs: &[UnblindedUtxo],
        blind_output_indices: &[usize],
        change_addr: &lwk_wollet::elements::Address,
    ) -> Result<()> {
        let blinding_pk = change_addr
            .blinding_pubkey
            .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;
        let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
            inner: blinding_pk,
            compressed: true,
        };

        // Mark only the specified wallet-destination outputs for blinding
        let outputs = pset.outputs_mut();
        for &idx in blind_output_indices {
            outputs[idx].blinding_key = Some(pset_blinding_key);
            outputs[idx].blinder_index = Some(0);
        }

        // Provide input txout secrets for ALL inputs (both confidential and explicit).
        // blind_last needs secrets for every input to compute surjection proofs.
        let mut inp_txout_sec = HashMap::new();
        for (idx, utxo) in wallet_inputs.iter().enumerate() {
            let asset_id = AssetId::from_slice(&utxo.asset_id)
                .map_err(|e| Error::Blinding(format!("input {idx} asset: {e}")))?;
            inp_txout_sec.insert(idx, txout_secrets_from_unblinded(utxo, asset_id)?);
        }

        let secp = secp256k1_zkp::Secp256k1::new();
        let mut rng = thread_rng();
        pset.blind_last(&mut rng, &secp, &inp_txout_sec)
            .map_err(|e| Error::Blinding(format!("{e:?}")))?;

        Ok(())
    }

    // ── Limit order methods ─────────────────────────────────────────────

    /// Create a limit order by locking the offered asset in a maker order covenant.
    #[allow(clippy::too_many_arguments)]
    pub fn create_limit_order(
        &mut self,
        base_asset_id: [u8; 32],
        quote_asset_id: [u8; 32],
        price: u64,
        order_amount: u64,
        direction: OrderDirection,
        min_fill_lots: u64,
        min_remainder_lots: u64,
        order_index: u32,
        fee_amount: u64,
    ) -> Result<CreateOrderResult> {
        self.sync()?;

        // 1. Derive maker keypair
        let maker_keypair = self.derive_maker_keypair(order_index)?;
        let (maker_xonly, _parity) = maker_keypair.x_only_public_key();
        let maker_base_pubkey: [u8; 32] = maker_xonly.serialize();

        // 2. Generate random order nonce
        let mut order_nonce = [0u8; 32];
        thread_rng().fill_bytes(&mut order_nonce);

        // 3. Build MakerOrderParams
        let (params, _p_order) = MakerOrderParams::new(
            base_asset_id,
            quote_asset_id,
            price,
            min_fill_lots,
            min_remainder_lots,
            direction,
            NUMS_KEY_BYTES,
            &maker_base_pubkey,
            &order_nonce,
        );

        // 4. Compile the contract
        let contract = CompiledMakerOrder::new(params)?;

        // 5. Determine offered asset and select funding UTXO
        let offered_asset = match direction {
            OrderDirection::SellBase => &base_asset_id,
            OrderDirection::SellQuote => &quote_asset_id,
        };

        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();
        let same_asset = *offered_asset == policy_bytes;

        let funding_utxo = if same_asset {
            // Buy order: single UTXO covers order + fee
            self.select_funding_utxo(offered_asset, order_amount + fee_amount, &[])?
        } else {
            self.select_funding_utxo(offered_asset, order_amount, &[])?
        };

        // 6. Select fee UTXO (or reuse funding UTXO for same-asset orders)
        let (fee_utxo, change_addr) = if same_asset {
            let addr_result = self.address(None)?;
            let change_addr: lwk_wollet::elements::Address = addr_result
                .address()
                .to_string()
                .parse()
                .map_err(|e| Error::Query(format!("bad change address: {}", e)))?;
            (funding_utxo.clone(), change_addr)
        } else {
            self.select_fee_utxo_excluding(fee_amount, &[funding_utxo.outpoint])?
        };
        let change_spk = change_addr.script_pubkey();

        // 7. Build PSET
        let input_utxos: Vec<UnblindedUtxo> = if same_asset {
            vec![funding_utxo.clone()]
        } else {
            vec![funding_utxo.clone(), fee_utxo.clone()]
        };

        let create_params = CreateOrderParams {
            funding_utxo,
            fee_utxo,
            order_amount,
            fee_amount,
            fee_asset_id: policy_bytes,
            change_destination: Some(change_spk.clone()),
            fee_change_destination: Some(change_spk),
            maker_base_pubkey,
        };

        let mut pset = build_create_order_pset(&contract, &create_params)?;

        // 7b. Blind change outputs (inputs are confidential wallet UTXOs).
        // Output 0 = covenant (explicit), Output 1 = fee (skip).
        // Outputs 2+ are optional funding/fee change — blind all of them.
        let num_outputs = pset.n_outputs();
        let blind_indices: Vec<usize> = (2..num_outputs).collect();
        self.blind_order_pset(&mut pset, &input_utxos, &blind_indices, &change_addr)?;

        // 8. Sign and broadcast
        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        let covenant_address = contract
            .address(&maker_base_pubkey, self.network.address_params())
            .to_string();

        Ok(CreateOrderResult {
            txid,
            order_params: params,
            maker_base_pubkey,
            order_nonce,
            covenant_address,
            order_amount,
        })
    }

    /// Cancel a limit order by script-path spending the covenant UTXO.
    ///
    /// Uses the Simplicity cancel path (Right branch) with a BIP-340 signature
    /// over SHA256(prev_outpoint) to authorize reclaiming funds.
    pub fn cancel_limit_order(
        &mut self,
        params: &MakerOrderParams,
        maker_base_pubkey: [u8; 32],
        order_index: u32,
        fee_amount: u64,
    ) -> Result<CancelOrderResult> {
        self.sync()?;

        // 1. Derive maker keypair
        let maker_keypair = self.derive_maker_keypair(order_index)?;

        // 2. Compile the contract
        let contract = CompiledMakerOrder::new(*params)?;
        let cmr = *contract.cmr();
        let cb_bytes = contract.control_block(&maker_base_pubkey);

        // 3. Compute covenant SPK and scan for order UTXO
        let covenant_spk = contract.script_pubkey(&maker_base_pubkey);
        let covenant_utxos = self.scan_covenant_utxos(&covenant_spk)?;
        let (order_outpoint, order_txout) = covenant_utxos
            .into_iter()
            .next()
            .ok_or_else(|| Error::MakerOrder("no UTXO found at order covenant address".into()))?;

        // 4. Convert to UnblindedUtxo (explicit asset, zeroed blinding factors)
        let order_value = order_txout.value.explicit().unwrap_or(0);
        let order_asset = match params.direction {
            OrderDirection::SellBase => params.base_asset_id,
            OrderDirection::SellQuote => params.quote_asset_id,
        };
        let order_utxo = UnblindedUtxo {
            outpoint: order_outpoint,
            txout: order_txout,
            asset_id: order_asset,
            value: order_value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        };

        // 5. Select fee UTXO
        let (fee_utxo, change_addr) =
            self.select_fee_utxo_excluding(fee_amount, &[order_outpoint])?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        // 6. Build cancel PSET
        let input_utxos = [order_utxo.clone(), fee_utxo.clone()];

        let cancel_params = CancelOrderParams {
            order_utxo,
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
            order_asset_id: order_asset,
            refund_destination: change_spk.clone(),
            fee_change_destination: Some(change_spk),
        };

        let mut pset = build_cancel_order_pset(&cancel_params)?;

        // 6b. Blind wallet-destination outputs.
        // Output 0 = refund (blind), Output 1 = fee (skip), Output 2 = fee change (blind).
        let mut blind_indices = vec![0usize]; // refund
        let num_outputs = pset.n_outputs();
        if num_outputs > 2 {
            blind_indices.push(2); // fee change
        }
        self.blind_order_pset(&mut pset, &input_utxos, &blind_indices, &change_addr)?;

        // 7. Compute maker cancel signature: SHA256(txid || vout) of the order outpoint
        let secp = secp256k1_zkp::Secp256k1::new();
        let sighash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(order_outpoint.txid.to_byte_array());
            hasher.update(order_outpoint.vout.to_be_bytes());
            let hash: [u8; 32] = hasher.finalize().into();
            hash
        };
        let msg = secp256k1_zkp::Message::from_digest(sighash);
        let sig = secp.sign_schnorr_no_aux_rand(&msg, &maker_keypair);
        let sig_bytes: [u8; 64] = sig.serialize();

        // 8. Attach Simplicity cancel witness to covenant input (input 0)
        {
            use simplicityhl::elements::taproot::ControlBlock;
            use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

            let tx = std::sync::Arc::new(pset_to_pruning_transaction(&pset)?);
            let utxos: Vec<ElementsUtxo> = pset
                .inputs()
                .iter()
                .enumerate()
                .map(|(i, inp)| {
                    inp.witness_utxo
                        .as_ref()
                        .map(|u| ElementsUtxo::from(u.clone()))
                        .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
                })
                .collect::<Result<Vec<_>>>()?;

            let control_block = ControlBlock::from_slice(&cb_bytes)
                .map_err(|e| Error::Witness(format!("control block: {e}")))?;

            let env = ElementsEnv::new(
                std::sync::Arc::clone(&tx),
                utxos,
                0, // covenant is input 0 for cancel
                cmr,
                control_block,
                None,
                self.wollet.network().genesis_block_hash(),
            );

            let witness_values =
                crate::maker_order::witness::build_maker_order_cancel_witness(&sig_bytes);
            let satisfied = CompiledMakerOrder::new(*params)?
                .program()
                .satisfy_with_env(witness_values, Some(&env))
                .map_err(|e| {
                    Error::Compilation(format!("maker order cancel witness satisfaction: {e}"))
                })?;
            let (program_bytes, witness_bytes) = serialize_maker_order_satisfied(&satisfied);
            let cmr_bytes = cmr.to_byte_array().to_vec();

            pset.inputs_mut()[0].final_script_witness =
                Some(vec![witness_bytes, program_bytes, cmr_bytes, cb_bytes]);
        }

        // 9. Sign fee input via normal signer
        self.wollet
            .add_details(&mut pset)
            .map_err(|e| Error::Signer(format!("add_details: {}", e)))?;
        self.signer
            .sign(&mut pset)
            .map_err(|e| Error::Signer(format!("{:?}", e)))?;
        let tx = self
            .wollet
            .finalize(&mut pset)
            .map_err(|e| Error::Finalize(e.to_string()))?;

        let txid = self.broadcast_and_sync(&tx)?;

        Ok(CancelOrderResult {
            txid,
            refunded_amount: order_value,
        })
    }

    /// Fill a limit order by spending the covenant UTXO via Simplicity script-path.
    pub fn fill_limit_order(
        &mut self,
        params: &MakerOrderParams,
        maker_base_pubkey: [u8; 32],
        order_nonce: [u8; 32],
        lots_to_fill: u64,
        fee_amount: u64,
    ) -> Result<FillOrderResult> {
        self.sync()?;

        // 1. Compile the contract
        let contract = CompiledMakerOrder::new(*params)?;

        // 2. Scan for order UTXO
        let covenant_spk = contract.script_pubkey(&maker_base_pubkey);
        let covenant_utxos = self.scan_covenant_utxos(&covenant_spk)?;
        let (order_outpoint, order_txout) = covenant_utxos
            .into_iter()
            .next()
            .ok_or_else(|| Error::MakerOrder("no UTXO found at order covenant address".into()))?;

        let order_value = order_txout.value.explicit().unwrap_or(0);
        let order_asset = match params.direction {
            OrderDirection::SellBase => params.base_asset_id,
            OrderDirection::SellQuote => params.quote_asset_id,
        };
        let order_utxo = UnblindedUtxo {
            outpoint: order_outpoint,
            txout: order_txout,
            asset_id: order_asset,
            value: order_value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        };

        // 3. Compute fill amounts based on direction
        let (
            taker_pays_amount,
            taker_pays_asset,
            taker_receives_amount,
            taker_receives_asset,
            maker_receive_amount,
            is_partial,
            remainder_amount,
        ) = match params.direction {
            OrderDirection::SellBase => {
                // Maker sells BASE lots, taker pays QUOTE
                let taker_payment = lots_to_fill
                    .checked_mul(params.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                let is_partial = lots_to_fill < order_value;
                let remainder = if is_partial {
                    order_value - lots_to_fill
                } else {
                    0
                };
                (
                    taker_payment,
                    params.quote_asset_id,
                    lots_to_fill,
                    params.base_asset_id,
                    taker_payment,
                    is_partial,
                    remainder,
                )
            }
            OrderDirection::SellQuote => {
                // Maker sells QUOTE, taker pays BASE lots
                let quote_consumed = lots_to_fill
                    .checked_mul(params.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                let is_partial = quote_consumed < order_value;
                let remainder = if is_partial {
                    order_value - quote_consumed
                } else {
                    0
                };
                (
                    lots_to_fill,
                    params.base_asset_id,
                    quote_consumed,
                    params.quote_asset_id,
                    lots_to_fill,
                    is_partial,
                    remainder,
                )
            }
        };

        // 4. Select taker funding UTXO
        let taker_funding =
            self.select_funding_utxo(&taker_pays_asset, taker_pays_amount, &[order_outpoint])?;

        // Compute taker change (excess from overfunded UTXO)
        let taker_change_amount = taker_funding.value - taker_pays_amount;

        // 5. Select fee UTXO
        let (fee_utxo, change_addr) =
            self.select_fee_utxo_excluding(fee_amount, &[order_outpoint, taker_funding.outpoint])?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        // 6. Compute maker receive script
        let (p_order, _spk_hash) = derive_maker_receive(&maker_base_pubkey, &order_nonce, params);
        let maker_receive_spk_bytes = maker_receive_script_pubkey(&p_order);
        let maker_receive_script = Script::from(maker_receive_spk_bytes);

        // 7. Build TakerFill and MakerOrderFill
        // Save wallet inputs for blinding (in PSET input order: taker, order, fee)
        let input_utxos = [taker_funding.clone(), order_utxo.clone(), fee_utxo.clone()];

        let taker_fill = TakerFill {
            funding_utxo: taker_funding,
            receive_destination: change_spk.clone(),
            receive_amount: taker_receives_amount,
            receive_asset_id: taker_receives_asset,
            change_destination: if taker_change_amount > 0 {
                Some(change_spk.clone())
            } else {
                None
            },
            change_amount: taker_change_amount,
            change_asset_id: taker_pays_asset,
        };

        // 7b. Save contract data needed for witness before moving into MakerOrderFill
        let cmr = *contract.cmr();
        let cb_bytes = contract.control_block(&maker_base_pubkey);

        let maker_fill = MakerOrderFill {
            contract,
            order_utxo,
            maker_base_pubkey,
            maker_receive_amount,
            maker_receive_script,
            is_partial,
            remainder_amount,
        };

        let fill_params = FillOrderParams {
            takers: vec![taker_fill],
            orders: vec![maker_fill],
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
            fee_change_destination: Some(change_spk),
        };

        let mut pset = build_fill_order_pset(&fill_params)?;

        // 7c. Blind wallet-destination outputs.
        // Fill output layout:
        //   [taker_receive, maker_receive, (remainder), (taker_change), fee, (fee_change)]
        // Blind: output 0 (taker receive) + any taker change + fee change (all ours).
        // Do NOT blind: maker_receive, remainder (explicit covenant outputs).
        let num_outputs = pset.n_outputs();
        let mut blind_indices = vec![0usize]; // taker receive
        for idx in 0..num_outputs {
            let out = &pset.outputs()[idx];
            // Skip output 0 (already added), fee outputs (empty script), and outputs
            // at covenant-controlled positions (1 = maker_receive, 2 = remainder if partial).
            if idx == 0 || out.script_pubkey.is_empty() {
                continue;
            }
            // Outputs 1 and (if partial fill) 2 are covenant outputs — skip them.
            if idx == 1 {
                continue;
            }
            if is_partial && idx == 2 {
                continue;
            }
            // Everything else is a wallet output — blind it.
            blind_indices.push(idx);
        }
        self.blind_order_pset(&mut pset, &input_utxos, &blind_indices, &change_addr)?;

        // 8. Attach Simplicity witness with pruning to covenant input (input 1)
        let covenant_input_idx = 1; // takers-first: input 0 = taker, input 1 = maker order
        {
            use simplicityhl::elements::taproot::ControlBlock;
            use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

            let tx = std::sync::Arc::new(pset_to_pruning_transaction(&pset)?);
            let utxos: Vec<ElementsUtxo> = pset
                .inputs()
                .iter()
                .enumerate()
                .map(|(i, inp)| {
                    inp.witness_utxo
                        .as_ref()
                        .map(|u| ElementsUtxo::from(u.clone()))
                        .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
                })
                .collect::<Result<Vec<_>>>()?;

            let control_block = ControlBlock::from_slice(&cb_bytes)
                .map_err(|e| Error::Witness(format!("control block: {e}")))?;

            let env = ElementsEnv::new(
                std::sync::Arc::clone(&tx),
                utxos,
                covenant_input_idx as u32,
                cmr,
                control_block,
                None,
                self.wollet.network().genesis_block_hash(),
            );

            let witness_values =
                crate::maker_order::witness::build_maker_order_fill_witness(&[0u8; 64]);
            let satisfied = CompiledMakerOrder::new(*params)?
                .program()
                .satisfy_with_env(witness_values, Some(&env))
                .map_err(|e| {
                    Error::Compilation(format!("maker order witness satisfaction: {e}"))
                })?;
            let (program_bytes, witness_bytes) = serialize_maker_order_satisfied(&satisfied);
            let cmr_bytes = cmr.to_byte_array().to_vec();

            pset.inputs_mut()[covenant_input_idx].final_script_witness =
                Some(vec![witness_bytes, program_bytes, cmr_bytes, cb_bytes]);
        }

        // 9. Sign taker + fee inputs via normal signer
        self.wollet
            .add_details(&mut pset)
            .map_err(|e| Error::Signer(format!("add_details: {}", e)))?;
        self.signer
            .sign(&mut pset)
            .map_err(|e| Error::Signer(format!("{:?}", e)))?;
        let tx = self
            .wollet
            .finalize(&mut pset)
            .map_err(|e| Error::Finalize(e.to_string()))?;

        let txid = self.broadcast_and_sync(&tx)?;

        Ok(FillOrderResult {
            txid,
            lots_filled: lots_to_fill,
            is_partial,
        })
    }

    // ── Trade routing: combined LMSR + limit order execution ─────────────

    /// Execute a routed trade plan: build combined PSET, blind, attach
    /// Simplicity witnesses, sign, and broadcast.
    ///
    /// The `plan` must have been produced by the trade router
    /// ([`trade::router::build_execution_plan`]). The caller (typically
    /// [`DeadcatNode::execute_trade`](crate::node::DeadcatNode)) is
    /// responsible for obtaining a quote first.
    pub(crate) fn execute_trade_plan(
        &mut self,
        plan: &crate::trade::types::ExecutionPlan,
        fee_amount: u64,
    ) -> Result<crate::trade::types::TradeResult> {
        use crate::trade::pset::{TradePsetParams, build_trade_pset};

        self.sync()?;

        if let Some(ref lmsr_leg) = plan.lmsr_pool_leg {
            self.validate_live_lmsr_reserve_anchors(lmsr_leg)?;
        }

        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        // 1. Compile contracts
        let order_contracts: Vec<CompiledMakerOrder> = plan
            .order_legs
            .iter()
            .map(|leg| CompiledMakerOrder::new(leg.params))
            .collect::<Result<Vec<_>>>()?;

        // 2. Collect outpoints to exclude from wallet UTXO selection
        let mut exclude: Vec<OutPoint> = Vec::new();
        if let Some(ref lmsr_leg) = plan.lmsr_pool_leg {
            exclude.push(lmsr_leg.pool_utxos.yes.outpoint);
            exclude.push(lmsr_leg.pool_utxos.no.outpoint);
            exclude.push(lmsr_leg.pool_utxos.collateral.outpoint);
        }
        for leg in &plan.order_legs {
            exclude.push(leg.order_utxo.outpoint);
        }

        // 3. Select taker funding UTXO
        let taker_funding =
            self.select_funding_utxo(&plan.taker_send_asset, plan.total_taker_input, &exclude)?;
        exclude.push(taker_funding.outpoint);

        // 4. Select fee UTXO + change address
        let (fee_utxo, change_addr) = self.select_fee_utxo_excluding(fee_amount, &exclude)?;
        let change_spk = change_addr.script_pubkey();

        let taker_change = if taker_funding.value > plan.total_taker_input {
            Some(change_spk.clone())
        } else {
            None
        };

        // 5. Build combined PSET
        let pset_result = build_trade_pset(&TradePsetParams {
            plan,
            order_contracts: &order_contracts,
            taker_funding_utxos: vec![taker_funding],
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
            taker_receive_destination: change_spk,
            taker_change_destination: taker_change,
        })?;
        let mut pset = pset_result.pset;

        // 6. Blind wallet-destination outputs.
        let blind_indices = pset_result.blind_output_indices.clone();
        self.blind_order_pset(
            &mut pset,
            &pset_result.all_input_utxos,
            &blind_indices,
            &change_addr,
        )?;

        // 7. Attach LMSR pool witnesses (if LMSR leg present)
        if let Some(ref lmsr_leg) = plan.lmsr_pool_leg {
            let lmsr_input_range = pset_result.lmsr_input_range.clone().ok_or_else(|| {
                Error::TradeRouting(
                    "internal error: missing lmsr_input_range for LMSR pool leg".into(),
                )
            })?;
            crate::lmsr_pool::assembly::attach_lmsr_pool_witnesses(
                &mut pset,
                lmsr_leg,
                lmsr_input_range,
                self.network.genesis_hash_simplicity(),
            )?;
        }

        // 8. Attach maker order witnesses for each order input
        {
            use simplicityhl::elements::taproot::ControlBlock;
            use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

            let tx = std::sync::Arc::new(pset_to_pruning_transaction(&pset)?);
            let utxos: Vec<ElementsUtxo> = pset
                .inputs()
                .iter()
                .enumerate()
                .map(|(i, inp)| {
                    inp.witness_utxo
                        .as_ref()
                        .map(|u| ElementsUtxo::from(u.clone()))
                        .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
                })
                .collect::<Result<Vec<_>>>()?;

            for (i, (leg, contract)) in plan
                .order_legs
                .iter()
                .zip(order_contracts.iter())
                .enumerate()
            {
                let input_idx = pset_result.order_input_indices[i];
                let cmr = *contract.cmr();
                let cb_bytes = contract.control_block(&leg.maker_base_pubkey);

                let control_block = ControlBlock::from_slice(&cb_bytes)
                    .map_err(|e| Error::Witness(format!("order control block: {e}")))?;

                let env = ElementsEnv::new(
                    std::sync::Arc::clone(&tx),
                    utxos.clone(),
                    input_idx as u32,
                    cmr,
                    control_block,
                    None,
                    self.wollet.network().genesis_block_hash(),
                );

                let witness_values =
                    crate::maker_order::witness::build_maker_order_fill_witness(&[0u8; 64]);
                let satisfied = CompiledMakerOrder::new(leg.params)?
                    .program()
                    .satisfy_with_env(witness_values, Some(&env))
                    .map_err(|e| {
                        Error::Compilation(format!("maker order witness satisfaction: {e}"))
                    })?;
                let (program_bytes, witness_bytes) = serialize_maker_order_satisfied(&satisfied);
                let cmr_bytes = cmr.to_byte_array().to_vec();

                pset.inputs_mut()[input_idx].final_script_witness =
                    Some(vec![witness_bytes, program_bytes, cmr_bytes, cb_bytes]);
            }
        }

        // 9. Sign wallet inputs and broadcast
        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(crate::trade::types::TradeResult {
            txid,
            total_input: plan.total_taker_input,
            total_output: plan.total_taker_output,
            num_orders_filled: plan.order_legs.len(),
            pool_used: plan.lmsr_pool_leg.is_some(),
            new_reserves: None,
        })
    }

    fn validate_live_lmsr_reserve_anchors(
        &self,
        leg: &crate::trade::types::LmsrPoolSwapLeg,
    ) -> Result<()> {
        let contract = CompiledLmsrPool::new(leg.pool_params)?;
        let old_spk = contract.script_pubkey(leg.old_s_index);
        let live = self.scan_covenant_utxos(&old_spk)?;
        let expected = [
            (&leg.pool_utxos.yes, "YES"),
            (&leg.pool_utxos.no, "NO"),
            (&leg.pool_utxos.collateral, "collateral"),
        ];
        for (utxo, role) in expected {
            let Some((_, live_txout)) =
                live.iter().find(|(outpoint, _)| *outpoint == utxo.outpoint)
            else {
                return Err(Error::TradeRouting(format!(
                    "stale/non-canonical LMSR reserve anchors: {role} outpoint {} is no longer live",
                    utxo.outpoint
                )));
            };
            if live_txout.asset != utxo.txout.asset
                || live_txout.value != utxo.txout.value
                || live_txout.script_pubkey != utxo.txout.script_pubkey
            {
                return Err(Error::TradeRouting(format!(
                    "stale/non-canonical LMSR reserve anchors: {role} outpoint {} changed on chain",
                    utxo.outpoint
                )));
            }
        }
        Ok(())
    }

    /// Find the explicit collateral UTXO from a set of covenant UTXOs.
    fn find_collateral_utxo(
        covenant_utxos: &[(OutPoint, TxOut)],
        params: &PredictionMarketParams,
    ) -> Result<UnblindedUtxo> {
        let collateral_id = AssetId::from_slice(&params.collateral_asset_id)
            .map_err(|e| Error::Unblind(format!("bad collateral asset: {e}")))?;

        for (outpoint, txout) in covenant_utxos {
            if let Asset::Explicit(asset) = txout.asset
                && asset == collateral_id
            {
                let value = txout.value.explicit().unwrap_or(0);
                return Ok(UnblindedUtxo {
                    outpoint: *outpoint,
                    txout: txout.clone(),
                    asset_id: params.collateral_asset_id,
                    value,
                    asset_blinding_factor: [0u8; 32],
                    value_blinding_factor: [0u8; 32],
                });
            }
        }

        Err(Error::CovenantScan(
            "collateral UTXO not found at covenant".into(),
        ))
    }

    // ── Token/fee UTXO helpers ───────────────────────────────────────────

    /// Find YES and NO token UTXOs in the wallet for burning `pairs` of each.
    fn find_token_utxos_for_burn(
        &self,
        yes_asset: &[u8; 32],
        no_asset: &[u8; 32],
        pairs: u64,
    ) -> Result<(Vec<UnblindedUtxo>, Vec<UnblindedUtxo>)> {
        let yes_id = AssetId::from_slice(yes_asset)
            .map_err(|e| Error::Query(format!("bad YES asset: {e}")))?;
        let no_id = AssetId::from_slice(no_asset)
            .map_err(|e| Error::Query(format!("bad NO asset: {e}")))?;

        let raw_utxos = self.utxos()?;

        let collect_tokens = |asset_id: AssetId,
                              asset_bytes: &[u8; 32],
                              needed: u64|
         -> Result<Vec<UnblindedUtxo>> {
            let mut collected = Vec::new();
            let mut total = 0u64;
            for u in raw_utxos
                .iter()
                .filter(|u| !u.is_spent && u.unblinded.asset == asset_id)
            {
                let tx = self.fetch_transaction(&u.outpoint.txid)?;
                let txout = tx
                    .output
                    .get(u.outpoint.vout as usize)
                    .ok_or_else(|| Error::Query("token UTXO vout out of range".into()))?
                    .clone();
                collected.push(wallet_txout_to_unblinded(u, &txout));
                total = total.saturating_add(u.unblinded.value);
                if total >= needed {
                    break;
                }
            }
            if total < needed {
                return Err(Error::InsufficientUtxos(format!(
                    "need {} tokens of asset {:?}, found {}",
                    needed,
                    hex::encode(asset_bytes),
                    total
                )));
            }
            Ok(collected)
        };

        let yes_utxos = collect_tokens(yes_id, yes_asset, pairs)?;
        let no_utxos = collect_tokens(no_id, no_asset, pairs)?;

        Ok((yes_utxos, no_utxos))
    }

    /// Find token UTXOs of a single asset type for burning.
    fn find_single_token_utxos(
        &self,
        token_asset: &[u8; 32],
        needed: u64,
    ) -> Result<Vec<UnblindedUtxo>> {
        let asset_id = AssetId::from_slice(token_asset)
            .map_err(|e| Error::Query(format!("bad token asset: {e}")))?;

        let raw_utxos = self.utxos()?;
        let mut collected = Vec::new();
        let mut total = 0u64;
        for u in raw_utxos
            .iter()
            .filter(|u| !u.is_spent && u.unblinded.asset == asset_id)
        {
            let tx = self.fetch_transaction(&u.outpoint.txid)?;
            let txout = tx
                .output
                .get(u.outpoint.vout as usize)
                .ok_or_else(|| Error::Query("token UTXO vout out of range".into()))?
                .clone();
            collected.push(wallet_txout_to_unblinded(u, &txout));
            total = total.saturating_add(u.unblinded.value);
            if total >= needed {
                break;
            }
        }
        if total < needed {
            return Err(Error::InsufficientUtxos(format!(
                "need {} tokens of asset {}, found {}",
                needed,
                hex::encode(token_asset),
                total
            )));
        }
        Ok(collected)
    }

    /// Select a fee UTXO and return it with a change address.
    ///
    /// Callers must ensure the wallet is synced before calling this method.
    fn select_fee_utxo(
        &mut self,
        fee_amount: u64,
    ) -> Result<(UnblindedUtxo, lwk_wollet::elements::Address)> {
        self.select_fee_utxo_excluding(fee_amount, &[])
    }

    // ── Covenant scanning helpers ───────────────────────────────────────

    pub(crate) fn scan_covenant_utxos(
        &self,
        script_pubkey: &Script,
    ) -> Result<Vec<(OutPoint, TxOut)>> {
        self.chain.scan_script_utxos(script_pubkey)
    }

    /// Scan LMSR pool reserves using canonical outpoint lineage.
    ///
    /// The tracker starts from the three canonical reserve anchors and advances
    /// by following transactions that spend the full reserve bundle.
    pub(crate) fn scan_lmsr_pool_state(
        &self,
        params: LmsrPoolParams,
        creation_txid: [u8; 32],
        initial_reserve_outpoints: [LmsrInitialOutpoint; 3],
        hinted_s_index: u64,
        witness_schema_version: &str,
    ) -> Result<LmsrPoolScanResult> {
        let parse_txid = |label: &str, bytes: [u8; 32]| -> Result<Txid> {
            let txid_hex = hex::encode(bytes);
            txid_hex
                .parse::<Txid>()
                .map_err(|e| Error::TradeRouting(format!("invalid {label} txid '{txid_hex}': {e}")))
        };

        let contract = CompiledLmsrPool::new(params)?;
        let mut hinted_s_index = hinted_s_index;
        let creation_txid_bytes = creation_txid;
        for (idx, outpoint) in initial_reserve_outpoints.iter().enumerate() {
            if outpoint.txid != creation_txid_bytes {
                return Err(Error::TradeRouting(format!(
                    "initial_reserve_outpoints[{idx}] txid must match creation_txid"
                )));
            }
        }
        let creation_txid = parse_txid("creation_txid", creation_txid_bytes)?;
        let creation_tx = self.fetch_transaction(&creation_txid)?;
        let expected_assets = [
            params.yes_asset_id,
            params.no_asset_id,
            params.collateral_asset_id,
        ];
        let first_vout = initial_reserve_outpoints[0].vout;
        let first_script = creation_tx
            .output
            .get(first_vout as usize)
            .ok_or_else(|| {
                Error::TradeRouting(format!(
                    "creation_txid output {first_vout} missing for initial_reserve_outpoints[0]"
                ))
            })?
            .script_pubkey
            .clone();
        for (idx, outpoint) in initial_reserve_outpoints.iter().enumerate() {
            let out = creation_tx
                .output
                .get(outpoint.vout as usize)
                .ok_or_else(|| {
                    Error::TradeRouting(format!(
                        "creation_txid output {} missing for initial_reserve_outpoints[{idx}]",
                        outpoint.vout
                    ))
                })?;
            let asset = match out.asset {
                Asset::Explicit(asset) => asset.into_inner().to_byte_array(),
                _ => {
                    return Err(Error::TradeRouting(format!(
                        "creation_txid output {} asset must be explicit for LMSR anchors",
                        outpoint.vout
                    )));
                }
            };
            if asset != expected_assets[idx] {
                return Err(Error::TradeRouting(format!(
                    "creation_txid output {} asset mismatch for LMSR anchor slot {idx}",
                    outpoint.vout
                )));
            }
            if out.script_pubkey != first_script {
                return Err(Error::TradeRouting(
                    "creation_txid LMSR anchor outputs must share the same script".into(),
                ));
            }
        }

        let yes_txid = parse_txid(
            "initial_reserve_outpoints[0]",
            initial_reserve_outpoints[0].txid,
        )?;
        let no_txid = parse_txid(
            "initial_reserve_outpoints[1]",
            initial_reserve_outpoints[1].txid,
        )?;
        let collateral_txid = parse_txid(
            "initial_reserve_outpoints[2]",
            initial_reserve_outpoints[2].txid,
        )?;

        let mut bundle = [
            OutPoint::new(yes_txid, initial_reserve_outpoints[0].vout),
            OutPoint::new(no_txid, initial_reserve_outpoints[1].vout),
            OutPoint::new(collateral_txid, initial_reserve_outpoints[2].vout),
        ];

        let mut safety_steps: u32 = 0;
        loop {
            safety_steps = safety_steps.saturating_add(1);
            if safety_steps > 512 {
                return Err(Error::TradeRouting(
                    "LMSR canonical bundle walk exceeded step limit".into(),
                ));
            }

            let yes_tx = self.fetch_transaction(&bundle[0].txid)?;
            let no_tx = self.fetch_transaction(&bundle[1].txid)?;
            let collateral_tx = self.fetch_transaction(&bundle[2].txid)?;

            let yes_txout = yes_tx
                .output
                .get(bundle[0].vout as usize)
                .ok_or_else(|| {
                    Error::TradeRouting("YES reserve outpoint vout out of range".into())
                })?
                .clone();
            let no_txout = no_tx
                .output
                .get(bundle[1].vout as usize)
                .ok_or_else(|| Error::TradeRouting("NO reserve outpoint vout out of range".into()))?
                .clone();
            let collateral_txout = collateral_tx
                .output
                .get(bundle[2].vout as usize)
                .ok_or_else(|| {
                    Error::TradeRouting("collateral reserve outpoint vout out of range".into())
                })?
                .clone();

            if yes_txout.script_pubkey != no_txout.script_pubkey
                || yes_txout.script_pubkey != collateral_txout.script_pubkey
            {
                return Err(Error::TradeRouting(
                    "canonical LMSR reserve scripts do not match".into(),
                ));
            }
            let current_script = yes_txout.script_pubkey.clone();
            let current_s_index =
                find_lmsr_state_index_by_script(&contract, hinted_s_index, &current_script)?;

            let history_txids = self.chain.script_history_txids(&current_script)?;
            let mut spenders = Vec::new();
            for txid in history_txids {
                let tx = self.fetch_transaction(&txid)?;
                let spends_yes = tx.input.iter().any(|i| i.previous_output == bundle[0]);
                let spends_no = tx.input.iter().any(|i| i.previous_output == bundle[1]);
                let spends_collateral = tx.input.iter().any(|i| i.previous_output == bundle[2]);

                if spends_yes || spends_no || spends_collateral {
                    if !(spends_yes && spends_no && spends_collateral) {
                        return Err(Error::TradeRouting(
                            "partial spend of canonical LMSR reserve bundle detected".into(),
                        ));
                    }
                    spenders.push(tx);
                }
            }

            if spenders.is_empty() {
                let (pool_utxos, reserves) = make_live_reserve_bundle(
                    bundle,
                    yes_txout,
                    no_txout,
                    collateral_txout,
                    contract.params(),
                )?;
                return Ok(LmsrPoolScanResult {
                    current_s_index,
                    pool_utxos,
                    reserves,
                });
            }

            if spenders.len() > 1 {
                return Err(Error::TradeRouting(
                    "ambiguous LMSR transition: multiple transactions spend canonical reserve bundle"
                        .into(),
                ));
            }

            let spend_tx = spenders.remove(0);
            let payload = decode_primary_witness_payload_from_spend_tx(
                &spend_tx,
                bundle[0],
                &contract,
                witness_schema_version,
            )?;
            if payload.old_s_index != current_s_index {
                return Err(Error::TradeRouting(format!(
                    "LMSR witness OLD_S_INDEX {} does not match canonical script-derived state {}",
                    payload.old_s_index, current_s_index
                )));
            }
            if payload.path_tag == 0 && payload.new_s_index == payload.old_s_index {
                return Err(Error::TradeRouting(
                    "invalid LMSR swap payload: NEW_S_INDEX must differ from OLD_S_INDEX".into(),
                ));
            }
            if payload.path_tag == 1 && payload.new_s_index != payload.old_s_index {
                return Err(Error::TradeRouting(
                    "invalid LMSR admin payload: NEW_S_INDEX must equal OLD_S_INDEX".into(),
                ));
            }
            let (next_bundle_utxos, _, next_script) =
                extract_reserve_window(&spend_tx, payload.out_base, contract.params())?;
            let expected_next_script = contract.script_pubkey(payload.new_s_index);
            if next_script != expected_next_script {
                return Err(Error::TradeRouting(
                    "LMSR transition output script does not match witness NEW_S_INDEX".into(),
                ));
            }
            bundle = [
                next_bundle_utxos.yes.outpoint,
                next_bundle_utxos.no.outpoint,
                next_bundle_utxos.collateral.outpoint,
            ];
            hinted_s_index = payload.new_s_index;
        }
    }

    /// Unblind a confidential covenant UTXO by trying wallet blinding keys.
    ///
    /// During creation/issuance, reissuance token outputs are blinded using the
    /// change address blinding pubkey. The matching private key is derived via
    /// SLIP77 from the address's script_pubkey.
    fn unblind_covenant_utxo(&self, txout: &TxOut) -> Result<(AssetId, u64, [u8; 32], [u8; 32])> {
        let master_blinding_key = self
            .signer
            .slip77_master_blinding_key()
            .map_err(|e| Error::Unblind(format!("slip77 key: {e}")))?;

        let secp = secp256k1_zkp::Secp256k1::new();

        // Try wallet addresses 0..100 — the blinding key was derived from one of them
        for i in 0..100u32 {
            let addr = match self.wollet.address(Some(i)) {
                Ok(a) => a,
                Err(_) => continue,
            };

            let addr_spk = addr.address().script_pubkey();
            let blinding_sk = master_blinding_key.blinding_private_key(&addr_spk);

            if let Ok(secrets) = txout.unblind(&secp, blinding_sk) {
                let _asset_bytes: [u8; 32] = secrets.asset.into_inner().to_byte_array();
                let mut abf = [0u8; 32];
                abf.copy_from_slice(secrets.asset_bf.into_inner().as_ref());
                let mut vbf = [0u8; 32];
                vbf.copy_from_slice(secrets.value_bf.into_inner().as_ref());
                return Ok((secrets.asset, secrets.value, abf, vbf));
            }
        }

        Err(Error::Unblind(
            "no wallet address blinding key could unblind this UTXO".into(),
        ))
    }

    // ── Market Validation ──────────────────────────────────────────────

    /// Validate that a market's creation tx matches the canonical dormant
    /// bootstrap described by the proof-carrying anchor.
    ///
    /// The anchor includes the creation txid plus dormant YES/NO output
    /// openings. The validator recomputes the confidential asset generators and
    /// value commitments from the published openings and requires an exact
    /// match for both dormant outputs.
    pub(crate) fn validate_market_creation(
        &self,
        params: &PredictionMarketParams,
        anchor: &PredictionMarketAnchor,
    ) -> Result<bool> {
        let parsed_anchor = parse_prediction_market_anchor(anchor).map_err(Error::Query)?;
        let tx = self.chain.fetch_transaction(&parsed_anchor.creation_txid)?;
        validate_prediction_market_creation_tx(params, &tx, anchor).map_err(Error::Query)
    }
}

// ── Private helpers ──────────────────────────────────────────────────────

fn validate_create_lmsr_pool_request(request: &CreateLmsrPoolRequest) -> Result<()> {
    request
        .pool_params
        .validate()
        .map_err(|e| Error::LmsrPool(e.to_string()))?;

    if request.pool_params.yes_asset_id != request.market_params.yes_token_asset {
        return Err(Error::LmsrPool(
            "pool yes_asset_id must match market yes_token_asset".into(),
        ));
    }
    if request.pool_params.no_asset_id != request.market_params.no_token_asset {
        return Err(Error::LmsrPool(
            "pool no_asset_id must match market no_token_asset".into(),
        ));
    }
    if request.pool_params.collateral_asset_id != request.market_params.collateral_asset_id {
        return Err(Error::LmsrPool(
            "pool collateral_asset_id must match market collateral_asset_id".into(),
        ));
    }
    if request.pool_params.half_payout_sats != request.market_params.collateral_per_token {
        return Err(Error::LmsrPool(
            "pool half_payout_sats must match market collateral_per_token".into(),
        ));
    }
    if request.initial_s_index > request.pool_params.s_max_index {
        return Err(Error::LmsrPool(format!(
            "initial_s_index {} exceeds s_max_index {}",
            request.initial_s_index, request.pool_params.s_max_index
        )));
    }

    let manifest = LmsrTableManifest::new(
        request.pool_params.table_depth,
        request.table_values.clone(),
    )?;
    manifest.verify_matches_pool_params(&request.pool_params)?;
    Ok(())
}

/// Compute the BIP340 admin signature for an LMSR AdminAdjust transition.
///
/// The message hash matches the contract's `verify_admin_signature()`:
/// SHA256(domain || genesis_hash || table_root || asset_ids || sig_all_hash ||
///        input_prevouts || indices || s_indices || reserve_amounts || output_spk_hashes)
#[allow(clippy::too_many_arguments)]
fn compute_lmsr_admin_signature(
    pset: &PartiallySignedTransaction,
    contract: &CompiledLmsrPool,
    params: &LmsrPoolParams,
    pool_utxos: &LmsrPoolUtxos,
    old_s_index: u64,
    new_s_index: u64,
    current_reserves: PoolReserves,
    new_reserves: PoolReserves,
    in_base: u32,
    out_base: u32,
    admin_keypair: &Keypair,
    genesis_hash: [u8; 32],
) -> Result<[u8; 64]> {
    use sha2::{Digest, Sha256};
    use simplicityhl::elements::taproot::ControlBlock;
    use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

    let tx = std::sync::Arc::new(pset_to_pruning_transaction(pset)?);
    let utxos: Vec<ElementsUtxo> = pset
        .inputs()
        .iter()
        .enumerate()
        .map(|(i, inp)| {
            inp.witness_utxo
                .as_ref()
                .map(|u| ElementsUtxo::from(u.clone()))
                .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
        })
        .collect::<Result<Vec<_>>>()?;

    let primary_cmr = *contract.primary_cmr();
    let cb_bytes = contract.primary_control_block(old_s_index);
    let control_block = ControlBlock::from_slice(&cb_bytes)
        .map_err(|e| Error::Witness(format!("admin CB: {e}")))?;

    let env = ElementsEnv::new(
        tx,
        utxos,
        in_base,
        primary_cmr,
        control_block,
        None,
        lwk_wollet::elements::BlockHash::from_byte_array(genesis_hash),
    );
    let sig_all: [u8; 32] = env.c_tx_env().sighash_all().to_byte_array();

    // Build the admin message hash matching the contract's verify_admin_signature()
    let mut hasher = Sha256::new();
    // Domain tag: "DEADCAT/LMSR_LIQUIDITY_ADJUST_V1"
    hasher.update(b"DEADCAT/LMSR_LIQUIDITY_ADJUST_V1");
    hasher.update(genesis_hash);
    hasher.update(params.lmsr_table_root);
    hasher.update(params.yes_asset_id);
    hasher.update(params.no_asset_id);
    hasher.update(params.collateral_asset_id);
    hasher.update(sig_all);
    // Input prevouts
    hasher.update(pool_utxos.yes.outpoint.txid.to_byte_array());
    hasher.update(pool_utxos.yes.outpoint.vout.to_be_bytes());
    hasher.update(pool_utxos.no.outpoint.txid.to_byte_array());
    hasher.update(pool_utxos.no.outpoint.vout.to_be_bytes());
    hasher.update(pool_utxos.collateral.outpoint.txid.to_byte_array());
    hasher.update(pool_utxos.collateral.outpoint.vout.to_be_bytes());
    // Indices and state
    hasher.update(in_base.to_be_bytes());
    hasher.update(out_base.to_be_bytes());
    hasher.update(old_s_index.to_be_bytes());
    hasher.update(new_s_index.to_be_bytes());
    // Reserve amounts
    hasher.update(current_reserves.r_yes.to_be_bytes());
    hasher.update(current_reserves.r_no.to_be_bytes());
    hasher.update(current_reserves.r_lbtc.to_be_bytes());
    hasher.update(new_reserves.r_yes.to_be_bytes());
    hasher.update(new_reserves.r_no.to_be_bytes());
    hasher.update(new_reserves.r_lbtc.to_be_bytes());
    // Output script pubkey hashes
    let spk_hash = contract.script_hash(new_s_index);
    hasher.update(spk_hash);
    hasher.update(spk_hash);
    hasher.update(spk_hash);

    let msg_hash: [u8; 32] = hasher.finalize().into();
    let secp = secp256k1_zkp::Secp256k1::new();
    let msg = secp256k1_zkp::Message::from_digest(msg_hash);
    let sig = secp.sign_schnorr_no_aux_rand(&msg, admin_keypair);
    Ok(sig.serialize())
}

fn find_lmsr_state_index_by_script(
    contract: &CompiledLmsrPool,
    hinted_s_index: u64,
    script: &Script,
) -> Result<u64> {
    if hinted_s_index <= contract.params().s_max_index
        && contract.script_pubkey(hinted_s_index) == *script
    {
        return Ok(hinted_s_index);
    }

    let mut found: Option<u64> = None;
    for idx in 0..=contract.params().s_max_index {
        if contract.script_pubkey(idx) == *script {
            if found.is_some() {
                return Err(Error::TradeRouting(
                    "ambiguous LMSR script match across multiple S_INDEX values".into(),
                ));
            }
            found = Some(idx);
        }
    }

    found.ok_or_else(|| Error::TradeRouting("unable to derive LMSR S_INDEX from script".into()))
}

#[allow(clippy::too_many_arguments)]
fn build_lmsr_bootstrap_pset(
    contract: &CompiledLmsrPool,
    initial_s_index: u64,
    initial_reserves: PoolReserves,
    reserve_yes_inputs: &[UnblindedUtxo],
    reserve_no_inputs: &[UnblindedUtxo],
    reserve_collateral_inputs: &[UnblindedUtxo],
    fee_inputs: &[UnblindedUtxo],
    fee_amount: u64,
    change_destination: &Script,
    fee_asset_id: &[u8; 32],
) -> Result<LmsrBootstrapPset> {
    let yes_total = sum_unblinded_values(reserve_yes_inputs)?;
    let no_total = sum_unblinded_values(reserve_no_inputs)?;
    let collateral_total = sum_unblinded_values(reserve_collateral_inputs)?;
    let fee_total = sum_unblinded_values(fee_inputs)?;
    let collateral_is_fee_asset = contract.params().collateral_asset_id == *fee_asset_id;

    if yes_total < initial_reserves.r_yes || no_total < initial_reserves.r_no {
        return Err(Error::InsufficientCollateral);
    }
    if collateral_is_fee_asset {
        if collateral_total < initial_reserves.r_lbtc {
            return Err(Error::InsufficientCollateral);
        }
        if collateral_total - initial_reserves.r_lbtc < fee_amount {
            return Err(Error::InsufficientFee);
        }
    } else {
        if collateral_total < initial_reserves.r_lbtc {
            return Err(Error::InsufficientCollateral);
        }
        if fee_total < fee_amount {
            return Err(Error::InsufficientFee);
        }
    }

    let covenant_spk = contract.script_pubkey(initial_s_index);
    let mut pset = new_pset();
    let mut wallet_inputs = Vec::with_capacity(
        reserve_yes_inputs.len()
            + reserve_no_inputs.len()
            + reserve_collateral_inputs.len()
            + fee_inputs.len(),
    );
    for utxo in reserve_yes_inputs
        .iter()
        .chain(reserve_no_inputs)
        .chain(reserve_collateral_inputs)
        .chain(fee_inputs)
    {
        add_pset_input(&mut pset, utxo);
        wallet_inputs.push(utxo.clone());
    }

    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().yes_asset_id,
            initial_reserves.r_yes,
            &covenant_spk,
        ),
    );
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().no_asset_id,
            initial_reserves.r_no,
            &covenant_spk,
        ),
    );
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().collateral_asset_id,
            initial_reserves.r_lbtc,
            &covenant_spk,
        ),
    );
    if fee_amount > 0 {
        add_pset_output(&mut pset, fee_txout(fee_asset_id, fee_amount));
    }

    let mut blind_output_indices = Vec::new();
    let mut changes = vec![
        (
            yes_total - initial_reserves.r_yes,
            contract.params().yes_asset_id,
        ),
        (
            no_total - initial_reserves.r_no,
            contract.params().no_asset_id,
        ),
    ];
    if collateral_is_fee_asset {
        changes.push((
            collateral_total - initial_reserves.r_lbtc - fee_amount,
            contract.params().collateral_asset_id,
        ));
    } else {
        changes.push((
            collateral_total - initial_reserves.r_lbtc,
            contract.params().collateral_asset_id,
        ));
        changes.push((fee_total - fee_amount, *fee_asset_id));
    }
    for (amount, asset_id) in changes {
        if amount == 0 {
            continue;
        }
        let output_index = pset.outputs().len();
        add_pset_output(
            &mut pset,
            explicit_txout(&asset_id, amount, change_destination),
        );
        blind_output_indices.push(output_index);
    }

    Ok(LmsrBootstrapPset {
        pset,
        wallet_inputs,
        blind_output_indices,
    })
}

fn recover_creation_anchor(
    tx: &Transaction,
    creation_txid: Txid,
    master_blinding_key: &MasterBlindingKey,
    change_addr: &lwk_wollet::elements::Address,
) -> Result<PredictionMarketAnchor> {
    let yes_output = tx
        .output
        .first()
        .ok_or_else(|| Error::Blinding("creation tx missing dormant YES output".to_string()))?;
    let no_output = tx
        .output
        .get(1)
        .ok_or_else(|| Error::Blinding("creation tx missing dormant NO output".to_string()))?;

    let secp = secp256k1_zkp::Secp256k1::new();
    let blinding_sk = master_blinding_key.blinding_private_key(&change_addr.script_pubkey());

    let yes_secrets = yes_output
        .unblind(&secp, blinding_sk)
        .map_err(|e| Error::Blinding(format!("unblind YES dormant output: {e}")))?;
    let no_secrets = no_output
        .unblind(&secp, blinding_sk)
        .map_err(|e| Error::Blinding(format!("unblind NO dormant output: {e}")))?;

    let mut yes_abf = [0u8; 32];
    yes_abf.copy_from_slice(yes_secrets.asset_bf.into_inner().as_ref());
    let mut yes_vbf = [0u8; 32];
    yes_vbf.copy_from_slice(yes_secrets.value_bf.into_inner().as_ref());
    let mut no_abf = [0u8; 32];
    no_abf.copy_from_slice(no_secrets.asset_bf.into_inner().as_ref());
    let mut no_vbf = [0u8; 32];
    no_vbf.copy_from_slice(no_secrets.value_bf.into_inner().as_ref());

    Ok(PredictionMarketAnchor::from_openings(
        creation_txid,
        yes_abf,
        yes_vbf,
        no_abf,
        no_vbf,
    ))
}

fn make_live_reserve_bundle(
    bundle: [OutPoint; 3],
    yes_txout: TxOut,
    no_txout: TxOut,
    collateral_txout: TxOut,
    params: &LmsrPoolParams,
) -> Result<(crate::trade::types::LmsrPoolUtxos, PoolReserves)> {
    use lwk_wollet::elements::confidential::{Asset, Value as ConfValue};

    let build = |name: &str,
                 outpoint: OutPoint,
                 txout: TxOut,
                 expected_asset: [u8; 32]|
     -> Result<UnblindedUtxo> {
        let asset = match txout.asset {
            Asset::Explicit(asset) => asset,
            _ => {
                return Err(Error::TradeRouting(format!(
                    "{name} reserve asset must be explicit"
                )));
            }
        };
        let value = match txout.value {
            ConfValue::Explicit(v) => v,
            _ => {
                return Err(Error::TradeRouting(format!(
                    "{name} reserve value must be explicit"
                )));
            }
        };
        let asset_id = asset.into_inner().to_byte_array();
        if asset_id != expected_asset {
            return Err(Error::TradeRouting(format!(
                "{name} reserve asset mismatch"
            )));
        }
        Ok(UnblindedUtxo {
            outpoint,
            txout,
            asset_id,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        })
    };

    let yes = build("YES", bundle[0], yes_txout, params.yes_asset_id)?;
    let no = build("NO", bundle[1], no_txout, params.no_asset_id)?;
    let collateral = build(
        "collateral",
        bundle[2],
        collateral_txout,
        params.collateral_asset_id,
    )?;

    let reserves = PoolReserves {
        r_yes: yes.value,
        r_no: no.value,
        r_lbtc: collateral.value,
    };

    Ok((
        crate::trade::types::LmsrPoolUtxos {
            yes,
            no,
            collateral,
        },
        reserves,
    ))
}

/// Convert a LWK `WalletTxOut` + full `TxOut` into the SDK's `UnblindedUtxo`.
///
/// Since both `lwk_wollet::elements` and `deadcat_sdk::elements` resolve to the
/// same `elements 0.25.2` crate, the types are directly compatible — no consensus
/// encode/decode bridging needed.
fn wallet_txout_to_unblinded(
    utxo: &WalletTxOut,
    txout: &lwk_wollet::elements::TxOut,
) -> UnblindedUtxo {
    let asset_bytes: [u8; 32] = utxo.unblinded.asset.into_inner().to_byte_array();

    let mut abf = [0u8; 32];
    abf.copy_from_slice(utxo.unblinded.asset_bf.into_inner().as_ref());

    let mut vbf = [0u8; 32];
    vbf.copy_from_slice(utxo.unblinded.value_bf.into_inner().as_ref());

    UnblindedUtxo {
        outpoint: utxo.outpoint,
        txout: txout.clone(),
        asset_id: asset_bytes,
        value: utxo.unblinded.value,
        asset_blinding_factor: abf,
        value_blinding_factor: vbf,
    }
}

fn sum_unblinded_values(utxos: &[UnblindedUtxo]) -> Result<u64> {
    utxos.iter().try_fold(0u64, |acc, utxo| {
        acc.checked_add(utxo.value).ok_or(Error::CollateralOverflow)
    })
}

fn select_wallet_utxo_set(
    raw_utxos: &[WalletTxOut],
    target_asset: AssetId,
    required_amount: u64,
    exclude: &[OutPoint],
    asset_bytes: &[u8; 32],
) -> Result<Vec<WalletTxOut>> {
    if required_amount == 0 {
        return Ok(Vec::new());
    }

    let mut candidates: Vec<_> = raw_utxos
        .iter()
        .filter(|u| {
            !u.is_spent && u.unblinded.asset == target_asset && !exclude.contains(&u.outpoint)
        })
        .cloned()
        .collect();
    candidates.sort_by(|a, b| {
        b.unblinded
            .value
            .cmp(&a.unblinded.value)
            .then_with(|| {
                a.outpoint
                    .txid
                    .to_string()
                    .cmp(&b.outpoint.txid.to_string())
            })
            .then_with(|| a.outpoint.vout.cmp(&b.outpoint.vout))
    });

    let mut selected = Vec::new();
    let mut total = 0u64;
    for utxo in candidates {
        total = total
            .checked_add(utxo.unblinded.value)
            .ok_or(Error::CollateralOverflow)?;
        selected.push(utxo);
        if total >= required_amount {
            return Ok(selected);
        }
    }

    Err(Error::InsufficientUtxos(format!(
        "need {} units of asset {}, found {} (excluding {} outpoints)",
        required_amount,
        hex::encode(asset_bytes),
        total,
        exclude.len()
    )))
}

/// Select 2 unspent L-BTC UTXOs suitable as defining outpoints.
fn select_defining_utxos(
    raw_utxos: &[WalletTxOut],
    policy_asset: AssetId,
    min_value_per_utxo: u64,
) -> Result<(WalletTxOut, WalletTxOut)> {
    let mut candidates: Vec<_> = raw_utxos
        .iter()
        .filter(|u| {
            !u.is_spent
                && u.unblinded.asset == policy_asset
                && u.unblinded.value >= min_value_per_utxo
        })
        .cloned()
        .collect();

    candidates.sort_by(|a, b| b.unblinded.value.cmp(&a.unblinded.value));

    if candidates.len() < 2 {
        return Err(Error::InsufficientUtxos(format!(
            "need at least 2 L-BTC UTXOs with >= {} sats each (found {}). \
             Fund the wallet and try again.",
            min_value_per_utxo,
            candidates.len()
        )));
    }

    Ok((candidates[0].clone(), candidates[1].clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prediction_market::anchor::PredictionMarketAnchor;
    use crate::prediction_market::state::MarketSlot;
    use crate::prediction_market_scan::validate_prediction_market_creation_tx;
    use crate::testing::{confidential_dormant_creation_txout, test_explicit_utxo};
    use lwk_wollet::Chain;
    use lwk_wollet::elements::bitcoin::hashes::Hash;
    use lwk_wollet::elements::confidential::{AssetBlindingFactor, ValueBlindingFactor};
    use lwk_wollet::elements::{AddressParams, OutPoint, Script, TxOutSecrets};

    fn make_utxo(value: u64, asset: AssetId, vout: u32, spent: bool) -> WalletTxOut {
        let addr = lwk_wollet::elements::Address::p2sh(
            &Script::new(),
            None,
            &AddressParams::LIQUID_TESTNET,
        );
        WalletTxOut {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            script_pubkey: Script::new(),
            height: Some(100),
            unblinded: TxOutSecrets {
                asset,
                asset_bf: AssetBlindingFactor::zero(),
                value,
                value_bf: ValueBlindingFactor::zero(),
            },
            wildcard_index: 0,
            ext_int: Chain::External,
            is_spent: spent,
            address: addr,
        }
    }

    fn policy_asset() -> AssetId {
        "0000000000000000000000000000000000000000000000000000000000000001"
            .parse()
            .unwrap()
    }

    fn other_asset() -> AssetId {
        "0000000000000000000000000000000000000000000000000000000000000002"
            .parse()
            .unwrap()
    }

    fn third_asset() -> AssetId {
        "0000000000000000000000000000000000000000000000000000000000000003"
            .parse()
            .unwrap()
    }

    fn sample_lmsr_create_request() -> CreateLmsrPoolRequest {
        let yes_asset = [0x11; 32];
        let no_asset = [0x22; 32];
        let collateral_asset = policy_asset().into_inner().to_byte_array();
        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let table_root = crate::lmsr_table_root(&table_values).unwrap();
        let pool_params = LmsrPoolParams {
            yes_asset_id: yes_asset,
            no_asset_id: no_asset,
            collateral_asset_id: collateral_asset,
            lmsr_table_root: table_root,
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: NUMS_KEY_BYTES,
        };
        CreateLmsrPoolRequest {
            market_params: PredictionMarketParams {
                oracle_public_key: [0x44; 32],
                collateral_asset_id: collateral_asset,
                yes_token_asset: yes_asset,
                no_token_asset: no_asset,
                yes_reissuance_token: [0x55; 32],
                no_reissuance_token: [0x66; 32],
                collateral_per_token: 100,
                expiry_time: 123,
            },
            pool_params,
            initial_s_index: 4,
            initial_reserves: PoolReserves {
                r_yes: 700,
                r_no: 600,
                r_lbtc: 800,
            },
            table_values,
            fee_amount: 25,
        }
    }

    #[test]
    fn select_defining_utxos_happy_path() {
        let pa = policy_asset();
        let utxos = vec![
            make_utxo(500, pa, 0, false),
            make_utxo(1000, pa, 1, false),
            make_utxo(800, pa, 2, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 1000);
        assert_eq!(b.unblinded.value, 800);
    }

    #[test]
    fn select_defining_utxos_excludes_below_min() {
        let pa = policy_asset();
        let utxos = vec![
            make_utxo(100, pa, 0, false),
            make_utxo(500, pa, 1, false),
            make_utxo(200, pa, 2, false),
            make_utxo(600, pa, 3, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 600);
        assert_eq!(b.unblinded.value, 500);
    }

    #[test]
    fn select_defining_utxos_excludes_spent() {
        let pa = policy_asset();
        let utxos = vec![
            make_utxo(1000, pa, 0, true),
            make_utxo(500, pa, 1, false),
            make_utxo(600, pa, 2, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 600);
        assert_eq!(b.unblinded.value, 500);
    }

    #[test]
    fn select_defining_utxos_excludes_wrong_asset() {
        let pa = policy_asset();
        let other = other_asset();
        let utxos = vec![
            make_utxo(1000, other, 0, false),
            make_utxo(500, pa, 1, false),
            make_utxo(600, pa, 2, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 600);
        assert_eq!(b.unblinded.value, 500);
    }

    #[test]
    fn select_defining_utxos_fewer_than_two() {
        let pa = policy_asset();
        let utxos = vec![make_utxo(500, pa, 0, false)];
        let result = select_defining_utxos(&utxos, pa, 300);
        assert!(result.is_err());
    }

    #[test]
    fn select_defining_utxos_empty() {
        let pa = policy_asset();
        let result = select_defining_utxos(&[], pa, 300);
        assert!(result.is_err());
    }

    #[test]
    fn validate_create_lmsr_pool_request_rejects_market_asset_mismatch() {
        let mut request = sample_lmsr_create_request();
        request.market_params.yes_token_asset = [0x99; 32];
        let err = validate_create_lmsr_pool_request(&request).unwrap_err();
        assert!(err.to_string().contains("yes_asset_id"));
    }

    #[test]
    fn validate_create_lmsr_pool_request_rejects_half_payout_mismatch() {
        let mut request = sample_lmsr_create_request();
        request.market_params.collateral_per_token = 101;
        let err = validate_create_lmsr_pool_request(&request).unwrap_err();
        assert!(err.to_string().contains("half_payout_sats"));
    }

    #[test]
    fn validate_create_lmsr_pool_request_rejects_invalid_initial_s_index() {
        let mut request = sample_lmsr_create_request();
        request.initial_s_index = request.pool_params.s_max_index + 1;
        let err = validate_create_lmsr_pool_request(&request).unwrap_err();
        assert!(err.to_string().contains("initial_s_index"));
    }

    #[test]
    fn validate_create_lmsr_pool_request_rejects_invalid_table_values() {
        let mut request = sample_lmsr_create_request();
        request.table_values[0] += 1;
        let err = validate_create_lmsr_pool_request(&request).unwrap_err();
        assert!(err.to_string().contains("manifest root"));
    }

    #[test]
    fn select_wallet_utxo_set_aggregates_across_multiple_utxos() {
        let asset = policy_asset();
        let utxos = vec![
            make_utxo(400, asset, 0, false),
            make_utxo(350, asset, 1, false),
            make_utxo(200, asset, 2, false),
            make_utxo(1_000, third_asset(), 3, false),
        ];
        let selected =
            select_wallet_utxo_set(&utxos, asset, 700, &[], &asset.into_inner().to_byte_array())
                .unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].unblinded.value, 400);
        assert_eq!(selected[1].unblinded.value, 350);
    }

    #[test]
    fn build_lmsr_bootstrap_pset_puts_reserves_first_and_tracks_change_blinding() {
        let request = sample_lmsr_create_request();
        let contract = CompiledLmsrPool::new(request.pool_params).unwrap();
        let change_script = Script::new();
        let yes_input =
            test_explicit_utxo(&request.pool_params.yes_asset_id, 700, &change_script, 1);
        let no_input = test_explicit_utxo(&request.pool_params.no_asset_id, 600, &change_script, 2);
        let collateral_input = test_explicit_utxo(
            &request.pool_params.collateral_asset_id,
            1_000,
            &change_script,
            3,
        );
        let fee_input = test_explicit_utxo(
            &request.pool_params.collateral_asset_id,
            50,
            &change_script,
            4,
        );

        let built = build_lmsr_bootstrap_pset(
            &contract,
            request.initial_s_index,
            request.initial_reserves,
            &[yes_input],
            &[no_input],
            &[collateral_input],
            &[fee_input],
            request.fee_amount,
            &change_script,
            &request.pool_params.collateral_asset_id,
        )
        .unwrap();

        let outputs = built.pset.outputs();
        let reserve_script = contract.script_pubkey(request.initial_s_index);
        assert_eq!(outputs[0].amount, Some(request.initial_reserves.r_yes));
        assert_eq!(
            outputs[0].asset,
            Some(AssetId::from_slice(&request.pool_params.yes_asset_id).unwrap())
        );
        assert_eq!(outputs[0].script_pubkey, reserve_script);
        assert_eq!(outputs[1].amount, Some(request.initial_reserves.r_no));
        assert_eq!(
            outputs[1].asset,
            Some(AssetId::from_slice(&request.pool_params.no_asset_id).unwrap())
        );
        assert_eq!(outputs[1].script_pubkey, reserve_script);
        assert_eq!(outputs[2].amount, Some(request.initial_reserves.r_lbtc));
        assert_eq!(
            outputs[2].asset,
            Some(AssetId::from_slice(&request.pool_params.collateral_asset_id).unwrap())
        );
        assert_eq!(outputs[2].script_pubkey, reserve_script);
        assert_eq!(built.blind_output_indices, vec![4]);
    }

    #[test]
    fn build_lmsr_bootstrap_pset_allows_fee_from_collateral_inputs_when_assets_match() {
        let request = sample_lmsr_create_request();
        let contract = CompiledLmsrPool::new(request.pool_params).unwrap();
        let change_script = Script::new();
        let yes_input =
            test_explicit_utxo(&request.pool_params.yes_asset_id, 700, &change_script, 1);
        let no_input = test_explicit_utxo(&request.pool_params.no_asset_id, 600, &change_script, 2);
        let collateral_input = test_explicit_utxo(
            &request.pool_params.collateral_asset_id,
            request.initial_reserves.r_lbtc + request.fee_amount + 200,
            &change_script,
            3,
        );

        let built = build_lmsr_bootstrap_pset(
            &contract,
            request.initial_s_index,
            request.initial_reserves,
            &[yes_input],
            &[no_input],
            &[collateral_input],
            &[],
            request.fee_amount,
            &change_script,
            &request.pool_params.collateral_asset_id,
        )
        .unwrap();

        let outputs = built.pset.outputs();
        assert_eq!(outputs[3].amount, Some(request.fee_amount));
        assert_eq!(
            outputs[3].asset,
            Some(AssetId::from_slice(&request.pool_params.collateral_asset_id).unwrap())
        );
        assert_eq!(outputs[4].amount, Some(200));
        assert_eq!(built.blind_output_indices, vec![4]);
    }

    /// Verify the cancel sighash byte-order conventions.
    ///
    /// The Simplicity contract computes: `SHA256(txid_u256 || vout_u32)`
    /// via `sha_256_ctx_8_add_32` (big-endian u256) + `sha_256_ctx_8_add_4` (big-endian u32).
    ///
    /// The Rust side must match: `SHA256(Txid::to_byte_array() || vout.to_be_bytes())`.
    #[test]
    fn cancel_sighash_byte_order() {
        use sha2::{Digest, Sha256};

        // vout must be serialized as big-endian to match sha_256_ctx_8_add_4
        assert_eq!(0u32.to_be_bytes(), [0, 0, 0, 0]);
        assert_eq!(1u32.to_be_bytes(), [0, 0, 0, 1]);
        assert_eq!(256u32.to_be_bytes(), [0, 0, 1, 0]);

        // Different txids must produce different sighashes
        let hash_a: [u8; 32] = {
            let mut h = Sha256::new();
            h.update([0x01u8; 32]);
            h.update(0u32.to_be_bytes());
            h.finalize().into()
        };
        let hash_b: [u8; 32] = {
            let mut h = Sha256::new();
            h.update([0x02u8; 32]);
            h.update(0u32.to_be_bytes());
            h.finalize().into()
        };
        assert_ne!(
            hash_a, hash_b,
            "different txids must produce different hashes"
        );

        // Different vouts must produce different sighashes
        let hash_c: [u8; 32] = {
            let mut h = Sha256::new();
            h.update([0x01u8; 32]);
            h.update(1u32.to_be_bytes());
            h.finalize().into()
        };
        assert_ne!(
            hash_a, hash_c,
            "different vouts must produce different hashes"
        );

        // Determinism
        let hash_a2: [u8; 32] = {
            let mut h = Sha256::new();
            h.update([0x01u8; 32]);
            h.update(0u32.to_be_bytes());
            h.finalize().into()
        };
        assert_eq!(hash_a, hash_a2);
    }

    fn creation_yes_prevout() -> OutPoint {
        OutPoint::new(Txid::from_byte_array([0xA1; 32]), 0)
    }

    fn creation_no_prevout() -> OutPoint {
        OutPoint::new(Txid::from_byte_array([0xA2; 32]), 1)
    }

    fn zero_contract_hash() -> [u8; 32] {
        [0u8; 32]
    }

    fn creation_test_params() -> PredictionMarketParams {
        use lwk_wollet::elements::ContractHash;

        let yes_entropy = AssetId::generate_asset_entropy(
            creation_yes_prevout(),
            ContractHash::from_byte_array(zero_contract_hash()),
        );
        let no_entropy = AssetId::generate_asset_entropy(
            creation_no_prevout(),
            ContractHash::from_byte_array(zero_contract_hash()),
        );

        PredictionMarketParams {
            oracle_public_key: [0xaa; 32],
            collateral_asset_id: [0xbb; 32],
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
            collateral_per_token: 100_000,
            expiry_time: 1_000_000,
        }
    }

    fn creation_token_only_input(outpoint: OutPoint) -> lwk_wollet::elements::TxIn {
        use lwk_wollet::elements::confidential::Value;
        use lwk_wollet::elements::secp256k1_zkp::ZERO_TWEAK;
        use lwk_wollet::elements::{AssetIssuance, Sequence, TxIn, TxInWitness};

        TxIn {
            previous_output: outpoint,
            is_pegin: false,
            script_sig: Script::new(),
            sequence: Sequence::MAX,
            asset_issuance: AssetIssuance {
                asset_blinding_nonce: ZERO_TWEAK,
                asset_entropy: zero_contract_hash(),
                amount: Value::Null,
                inflation_keys: Value::Explicit(1),
            },
            witness: TxInWitness::default(),
        }
    }

    fn creation_plain_input(outpoint: OutPoint) -> lwk_wollet::elements::TxIn {
        use lwk_wollet::elements::{Sequence, TxIn, TxInWitness};

        TxIn {
            previous_output: outpoint,
            is_pegin: false,
            script_sig: Script::new(),
            sequence: Sequence::MAX,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        }
    }

    fn valid_creation_openings() -> ([u8; 32], [u8; 32], [u8; 32], [u8; 32]) {
        ([0x21; 32], [0x31; 32], [0x41; 32], [0x51; 32])
    }

    fn valid_creation_tx(params: &PredictionMarketParams) -> Transaction {
        use lwk_wollet::elements::confidential;
        use lwk_wollet::elements::{LockTime, TxOut, TxOutWitness};

        let compiled = CompiledPredictionMarket::new(*params).unwrap();
        let dormant_yes_spk = compiled.script_pubkey(MarketSlot::DormantYesRt);
        let dormant_no_spk = compiled.script_pubkey(MarketSlot::DormantNoRt);
        let collateral_asset = AssetId::from_slice(&params.collateral_asset_id).unwrap();
        let (yes_abf, yes_vbf, no_abf, no_vbf) = valid_creation_openings();

        Transaction {
            version: 2,
            lock_time: LockTime::ZERO,
            input: vec![
                creation_token_only_input(creation_yes_prevout()),
                creation_token_only_input(creation_no_prevout()),
                creation_plain_input(OutPoint::new(Txid::from_byte_array([0xA3; 32]), 2)),
            ],
            output: vec![
                confidential_dormant_creation_txout(
                    &params.yes_reissuance_token,
                    &yes_abf,
                    &yes_vbf,
                    &dormant_yes_spk,
                ),
                confidential_dormant_creation_txout(
                    &params.no_reissuance_token,
                    &no_abf,
                    &no_vbf,
                    &dormant_no_spk,
                ),
                TxOut {
                    asset: confidential::Asset::Explicit(collateral_asset),
                    value: confidential::Value::Explicit(50_000),
                    nonce: confidential::Nonce::Null,
                    script_pubkey: Script::new(),
                    witness: TxOutWitness::default(),
                },
            ],
        }
    }

    fn explicit_creation_tx(params: &PredictionMarketParams) -> Transaction {
        use lwk_wollet::elements::confidential;

        let mut tx = valid_creation_tx(params);
        tx.output[0].asset = confidential::Asset::Explicit(
            AssetId::from_slice(&params.yes_reissuance_token).unwrap(),
        );
        tx.output[0].value = confidential::Value::Explicit(1);
        tx.output[0].nonce = confidential::Nonce::Null;
        tx.output[1].asset = confidential::Asset::Explicit(
            AssetId::from_slice(&params.no_reissuance_token).unwrap(),
        );
        tx.output[1].value = confidential::Value::Explicit(1);
        tx.output[1].nonce = confidential::Nonce::Null;
        tx
    }

    fn valid_creation_anchor() -> PredictionMarketAnchor {
        let (yes_abf, yes_vbf, no_abf, no_vbf) = valid_creation_openings();
        PredictionMarketAnchor::from_openings(
            Txid::from_byte_array([0xD1; 32]),
            yes_abf,
            yes_vbf,
            no_abf,
            no_vbf,
        )
    }

    #[test]
    fn validate_market_creation_tx_valid() {
        let params = creation_test_params();
        let tx = valid_creation_tx(&params);
        let anchor = valid_creation_anchor();

        assert!(validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_wrong_dormant_rt_amount() {
        use lwk_wollet::elements::confidential;

        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        tx.output[0].value = confidential::Value::Explicit(2);
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_explicit_dormant_outputs() {
        let params = creation_test_params();
        let tx = explicit_creation_tx(&params);
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_asset_issuance_amount() {
        use lwk_wollet::elements::confidential;

        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        tx.input[0].asset_issuance.amount = confidential::Value::Explicit(1);
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_extra_issuing_input() {
        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        tx.input.push(creation_token_only_input(OutPoint::new(
            Txid::from_byte_array([0xA4; 32]),
            3,
        )));
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_nonzero_contract_hash() {
        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        tx.input[0].asset_issuance.asset_entropy = [0xC1; 32];
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_nonzero_blinding_nonce() {
        use lwk_wollet::elements::secp256k1_zkp::Tweak;

        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        tx.input[1].asset_issuance.asset_blinding_nonce = Tweak::from_slice(&[0x11; 32]).unwrap();
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_other_market_slot_outputs() {
        use lwk_wollet::elements::TxOutWitness;
        use lwk_wollet::elements::confidential;

        let params = creation_test_params();
        let compiled = CompiledPredictionMarket::new(params).unwrap();
        let mut tx = valid_creation_tx(&params);
        tx.output.push(lwk_wollet::elements::TxOut {
            asset: confidential::Asset::Explicit(
                AssetId::from_slice(&params.collateral_asset_id).unwrap(),
            ),
            value: confidential::Value::Explicit(123),
            nonce: confidential::Nonce::Null,
            script_pubkey: compiled.script_pubkey(MarketSlot::UnresolvedCollateral),
            witness: TxOutWitness::default(),
        });
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_duplicate_dormant_outputs() {
        use lwk_wollet::elements::TxOut;

        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        let duplicate_yes: TxOut = tx.output[0].clone();
        tx.output.push(duplicate_yes);
        let anchor = valid_creation_anchor();

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_accepts_blinded_dormant_output_with_matching_opening() {
        let params = creation_test_params();
        let tx = valid_creation_tx(&params);
        let anchor = valid_creation_anchor();

        assert!(validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }

    #[test]
    fn validate_market_creation_tx_rejects_blinded_dummy_dormant_output() {
        use lwk_wollet::elements::confidential;
        use lwk_wollet::elements::secp256k1_zkp::{
            Generator, PedersenCommitment, Secp256k1, Tag, Tweak,
        };

        let params = creation_test_params();
        let mut tx = valid_creation_tx(&params);
        let anchor = valid_creation_anchor();
        let secp = Secp256k1::new();
        let wrong_generator = Generator::new_blinded(
            &secp,
            Tag::from([0x99; 32]),
            Tweak::from_slice(&[0x21; 32]).unwrap(),
        );
        let wrong_commitment = PedersenCommitment::new(
            &secp,
            1,
            Tweak::from_slice(&[0x31; 32]).unwrap(),
            wrong_generator,
        );
        tx.output[0].asset = confidential::Asset::Confidential(wrong_generator);
        tx.output[0].value = confidential::Value::Confidential(wrong_commitment);
        tx.output[0].nonce = confidential::Nonce::Confidential(
            lwk_wollet::elements::secp256k1_zkp::PublicKey::from_slice(&[
                0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce,
                0x87, 0x0b, 0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81,
                0x5b, 0x16, 0xf8, 0x17, 0x98,
            ])
            .unwrap(),
        );

        assert!(!validate_prediction_market_creation_tx(&params, &tx, &anchor).unwrap());
    }
}
