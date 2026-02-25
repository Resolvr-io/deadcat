use std::collections::HashMap;
use std::path::Path;

use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::elements::confidential::Asset;
use lwk_wollet::elements::hashes::Hash as _;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp::{self, Keypair};
use lwk_wollet::elements::{AssetId, OutPoint, Script, Transaction, TxOut, Txid};
use lwk_wollet::{
    ElectrumClient, ElectrumUrl, TxBuilder, WalletTx, WalletTxOut, Wollet, WolletDescriptor,
};
use rand::RngCore;
use rand::thread_rng;

use crate::assembly::{
    CollateralSource, IssuanceAssemblyInputs, assemble_cancellation, assemble_expiry_redemption,
    assemble_issuance, assemble_oracle_resolve, assemble_post_resolution_redemption,
    compute_issuance_entropy, txout_secrets_from_unblinded,
};
use crate::chain::{ChainBackend, ElectrumBackend};
use crate::contract::CompiledContract;
use crate::error::{Error, Result};
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
use crate::params::ContractParams;
use crate::pset::UnblindedUtxo;
use crate::pset::cancellation::CancellationParams;
use crate::pset::creation::{CreationParams, build_creation_pset};
use crate::pset::expiry_redemption::ExpiryRedemptionParams;
use crate::pset::oracle_resolve::OracleResolveParams;
use crate::pset::post_resolution_redemption::PostResolutionRedemptionParams;
use crate::state::MarketState;
use crate::taproot::NUMS_KEY_BYTES;

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

/// Result of a successful AMM pool creation.
#[derive(Debug, Clone)]
pub struct PoolCreationResult {
    pub txid: Txid,
    pub pool_params: crate::amm_pool::params::AmmPoolParams,
    pub issued_lp: u64,
    pub covenant_address: String,
}

/// Result of a successful AMM pool swap.
#[derive(Debug, Clone)]
pub struct PoolSwapResult {
    pub txid: Txid,
    pub delta_in: u64,
    pub delta_out: u64,
    /// New reserves after the swap (as computed by the SDK from on-chain state).
    pub new_reserves: crate::amm_pool::math::PoolReserves,
}

/// Result of a successful AMM pool LP deposit or withdraw.
#[derive(Debug, Clone)]
pub struct PoolLpResult {
    pub txid: Txid,
    pub new_issued_lp: u64,
    /// New reserves after the LP operation (as computed by the SDK from on-chain state).
    pub new_reserves: crate::amm_pool::math::PoolReserves,
}

pub struct DeadcatSdk {
    signer: SwSigner,
    wollet: Wollet,
    network: Network,
    chain: ElectrumBackend,
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
        // Re-sync wallet after broadcast
        let url: ElectrumUrl = self
            .chain
            .electrum_url()
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let mut client = ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
        lwk_wollet::full_scan_with_electrum_client(&mut self.wollet, &mut client)
            .map_err(|e| Error::Electrum(e.to_string()))?;
        Ok(txid)
    }

    pub fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction> {
        self.chain.fetch_transaction(txid)
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn electrum_url(&self) -> &str {
        self.chain.electrum_url()
    }

    pub fn chain(&self) -> &ElectrumBackend {
        &self.chain
    }

    pub fn policy_asset(&self) -> AssetId {
        self.network.into_lwk().policy_asset()
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
    ) -> Result<(Txid, ContractParams)> {
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
        let contract = CompiledContract::create(
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

        // Fill reissuance-token outputs and blind PSET
        {
            let cp = contract.params();
            let yes_rt = AssetId::from_slice(&cp.yes_reissuance_token)
                .map_err(|e| Error::Blinding(format!("bad YES reissuance asset: {e}")))?;
            let no_rt = AssetId::from_slice(&cp.no_reissuance_token)
                .map_err(|e| Error::Blinding(format!("bad NO reissuance asset: {e}")))?;

            let outputs = sdk_pset.outputs_mut();
            outputs[0].amount = Some(1);
            outputs[0].asset = Some(yes_rt);
            outputs[1].amount = Some(1);
            outputs[1].asset = Some(no_rt);

            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;
            let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
                inner: blinding_pk,
                compressed: true,
            };

            for idx in [0usize, 1] {
                outputs[idx].blinding_key = Some(pset_blinding_key);
                outputs[idx].blinder_index = Some(0);
            }
            if outputs.len() == 4 {
                outputs[3].blinding_key = Some(pset_blinding_key);
                outputs[3].blinder_index = Some(0);
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

        Ok((txid, params))
    }

    // ── Token issuance ──────────────────────────────────────────────────

    /// Issue prediction market token pairs.
    ///
    /// Detects whether the market is in Dormant (initial issuance) or Unresolved
    /// (subsequent issuance) state and builds the appropriate transaction.
    pub fn issue_tokens(
        &mut self,
        params: &ContractParams,
        creation_txid: &Txid,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<IssuanceResult> {
        let contract = CompiledContract::new(*params)?;

        // A. Scan market state
        let (current_state, covenant_utxos) = self.scan_market_state(&contract)?;

        // B. Classify and unblind covenant UTXOs
        let (yes_rt, no_rt, collateral_covenant_utxo) =
            self.classify_covenant_utxos(&covenant_utxos, params, current_state)?;

        // C. Compute issuance entropy
        let creation_tx = self.fetch_transaction(creation_txid)?;
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
        let tx = self.sign_pset(assembled.pset)?;

        // J. Broadcast
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(IssuanceResult {
            txid,
            previous_state: current_state,
            new_state: MarketState::Unresolved,
            pairs_issued: pairs,
        })
    }

    /// Scan covenant addresses to determine the current on-chain market state.
    ///
    /// Returns the state and the UTXOs found at the corresponding covenant address.
    pub(crate) fn scan_market_state(
        &self,
        contract: &CompiledContract,
    ) -> Result<(MarketState, Vec<(OutPoint, TxOut)>)> {
        let dormant_spk = contract.script_pubkey(MarketState::Dormant);
        let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
        let resolved_yes_spk = contract.script_pubkey(MarketState::ResolvedYes);
        let resolved_no_spk = contract.script_pubkey(MarketState::ResolvedNo);

        let dormant_utxos = self.scan_covenant_utxos(&dormant_spk)?;
        let unresolved_utxos = self.scan_covenant_utxos(&unresolved_spk)?;
        let resolved_yes_utxos = self.scan_covenant_utxos(&resolved_yes_spk)?;
        let resolved_no_utxos = self.scan_covenant_utxos(&resolved_no_spk)?;

        if !dormant_utxos.is_empty() {
            Ok((MarketState::Dormant, dormant_utxos))
        } else if !unresolved_utxos.is_empty() {
            Ok((MarketState::Unresolved, unresolved_utxos))
        } else if !resolved_yes_utxos.is_empty() {
            Ok((MarketState::ResolvedYes, resolved_yes_utxos))
        } else if !resolved_no_utxos.is_empty() {
            Ok((MarketState::ResolvedNo, resolved_no_utxos))
        } else {
            Err(Error::CovenantScan(
                "no UTXOs found at any covenant addresses — \
                 the contract may have been created with an older incompatible version"
                    .into(),
            ))
        }
    }

    /// Classify and unblind covenant UTXOs into YES RT, NO RT, and optional collateral.
    fn classify_covenant_utxos(
        &self,
        covenant_utxos: &[(OutPoint, TxOut)],
        params: &ContractParams,
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
                Asset::Explicit(asset) if asset == collateral_id => {
                    let value = txout.value.explicit().unwrap_or(0);
                    collateral_covenant_utxo = Some(UnblindedUtxo {
                        outpoint: *outpoint,
                        txout: txout.clone(),
                        asset_id: params.collateral_asset_id,
                        value,
                        asset_blinding_factor: [0u8; 32],
                        value_blinding_factor: [0u8; 32],
                    });
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
        params: &ContractParams,
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
        params: &ContractParams,
        pairs_to_burn: u64,
        fee_amount: u64,
    ) -> Result<CancellationResult> {
        self.sync()?;
        let contract = CompiledContract::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract)?;
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

        let tx = self.sign_pset(assembled.pset)?;
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
    /// The oracle resolve transaction has exactly 4 outputs with no room for
    /// fee change, so the entire fee UTXO is consumed as the fee.
    pub fn resolve_market(
        &mut self,
        params: &ContractParams,
        outcome_yes: bool,
        oracle_signature: [u8; 64],
        fee_amount: u64,
    ) -> Result<ResolutionResult> {
        self.sync()?;
        let contract = CompiledContract::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract)?;
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
            fee_amount: fee_unblinded.value,
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

        let tx = self.sign_pset(assembled.pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

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
        params: &ContractParams,
        tokens_to_burn: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult> {
        self.sync()?;
        let contract = CompiledContract::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract)?;
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

        let tx = self.sign_pset(assembled.pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(RedemptionResult {
            txid,
            previous_state: current_state,
            tokens_redeemed: tokens_to_burn,
            payout_sats: payout,
        })
    }

    // ── Expiry redemption ────────────────────────────────────────────────

    /// Redeem tokens after market expiry (no oracle resolution).
    ///
    /// Burns tokens and reclaims 1x collateral_per_token per token.
    /// Requires the market to still be Unresolved and block height past expiry.
    pub fn redeem_expired(
        &mut self,
        params: &ContractParams,
        token_asset: [u8; 32],
        tokens_to_burn: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult> {
        self.sync()?;
        let contract = CompiledContract::new(*params)?;

        let (current_state, covenant_utxos) = self.scan_market_state(&contract)?;
        if current_state != MarketState::Unresolved {
            return Err(Error::NotRedeemable(current_state));
        }

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

        let tx = self.sign_pset(assembled.pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(RedemptionResult {
            txid,
            previous_state: current_state,
            tokens_redeemed: tokens_to_burn,
            payout_sats: payout,
        })
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

    // ── AMM pool blinding helper ─────────────────────────────────────────

    /// Blind selected outputs in an AMM pool PSET that has mixed covenant
    /// (explicit) and wallet (confidential) inputs.
    ///
    /// `all_inputs_with_indices`: pairs of (pset_input_index, UnblindedUtxo)
    ///   for ALL inputs (pool covenant + wallet). Required for surjection proofs.
    /// `blind_output_indices`: PSET output indices to blind (RT + wallet outputs).
    fn blind_pool_pset(
        &self,
        pset: &mut PartiallySignedTransaction,
        wallet_inputs_with_indices: &[(usize, &UnblindedUtxo)],
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

        let outputs = pset.outputs_mut();
        for &idx in blind_output_indices {
            outputs[idx].blinding_key = Some(pset_blinding_key);
            outputs[idx].blinder_index = Some(0);
        }

        let mut inp_txout_sec = HashMap::new();
        for &(idx, utxo) in wallet_inputs_with_indices {
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

        let funding_utxo = self.select_funding_utxo(offered_asset, order_amount, &[])?;

        // 6. Select fee UTXO (exclude funding outpoint)
        let (fee_utxo, change_addr) =
            self.select_fee_utxo_excluding(fee_amount, &[funding_utxo.outpoint])?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        // 7. Build PSET
        let input_utxos = [funding_utxo.clone(), fee_utxo.clone()];

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
            use crate::assembly::pset_to_pruning_transaction;
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
            use crate::assembly::pset_to_pruning_transaction;
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

    // ── AMM pool methods ──────────────────────────────────────────────────

    /// Scan the 4 pool UTXOs (YES, NO, LBTC, LP_RT) at a given covenant address.
    pub(crate) fn scan_pool_utxos(
        &self,
        contract: &crate::amm_pool::contract::CompiledAmmPool,
        issued_lp: u64,
    ) -> Result<(UnblindedUtxo, UnblindedUtxo, UnblindedUtxo, UnblindedUtxo)> {
        let covenant_spk = contract.script_pubkey(issued_lp);
        let utxos = self.scan_covenant_utxos(&covenant_spk)?;

        let params = contract.params();
        let mut yes_utxo: Option<UnblindedUtxo> = None;
        let mut no_utxo: Option<UnblindedUtxo> = None;
        let mut lbtc_utxo: Option<UnblindedUtxo> = None;
        let mut rt_utxo: Option<UnblindedUtxo> = None;

        for (outpoint, txout) in &utxos {
            if let Some(asset) = txout.asset.explicit() {
                let asset_bytes: [u8; 32] = asset.into_inner().to_byte_array();
                let value = txout.value.explicit().unwrap_or(0);
                let u = UnblindedUtxo {
                    outpoint: *outpoint,
                    txout: txout.clone(),
                    asset_id: asset_bytes,
                    value,
                    asset_blinding_factor: [0u8; 32],
                    value_blinding_factor: [0u8; 32],
                };
                if asset_bytes == params.yes_asset_id && yes_utxo.is_none() {
                    yes_utxo = Some(u);
                } else if asset_bytes == params.no_asset_id && no_utxo.is_none() {
                    no_utxo = Some(u);
                } else if asset_bytes == params.lbtc_asset_id && lbtc_utxo.is_none() {
                    lbtc_utxo = Some(u);
                } else if asset_bytes == params.lp_reissuance_token_id && rt_utxo.is_none() {
                    // RT may be confidential; try unblinding
                    rt_utxo = Some(u);
                }
            } else {
                // Confidential output — try to unblind (for RT)
                if let Ok((asset, value, abf, vbf)) = self.unblind_covenant_utxo(txout) {
                    let asset_bytes: [u8; 32] = asset.into_inner().to_byte_array();
                    if asset_bytes == params.lp_reissuance_token_id && rt_utxo.is_none() {
                        rt_utxo = Some(UnblindedUtxo {
                            outpoint: *outpoint,
                            txout: txout.clone(),
                            asset_id: asset_bytes,
                            value,
                            asset_blinding_factor: abf,
                            value_blinding_factor: vbf,
                        });
                    }
                }
            }
        }

        let yes = yes_utxo.ok_or_else(|| {
            Error::CovenantScan("YES reserve UTXO not found at pool address".into())
        })?;
        let no = no_utxo.ok_or_else(|| {
            Error::CovenantScan("NO reserve UTXO not found at pool address".into())
        })?;
        let lbtc = lbtc_utxo.ok_or_else(|| {
            Error::CovenantScan("LBTC reserve UTXO not found at pool address".into())
        })?;
        let rt = rt_utxo.ok_or_else(|| {
            Error::CovenantScan("LP reissuance token UTXO not found at pool address".into())
        })?;

        Ok((yes, no, lbtc, rt))
    }

    /// Create a new AMM pool on-chain.
    #[allow(clippy::too_many_arguments)]
    pub fn create_amm_pool(
        &mut self,
        pool_params: &crate::amm_pool::params::AmmPoolParams,
        initial_r_yes: u64,
        initial_r_no: u64,
        initial_r_lbtc: u64,
        initial_issued_lp: u64,
        fee_amount: u64,
        lp_creation_txid: &Txid,
    ) -> Result<PoolCreationResult> {
        self.sync()?;

        let contract = crate::amm_pool::contract::CompiledAmmPool::new(*pool_params)?;

        // Select funding UTXOs for each asset
        let yes_funding =
            self.select_funding_utxo(&pool_params.yes_asset_id, initial_r_yes, &[])?;
        let no_funding = self.select_funding_utxo(
            &pool_params.no_asset_id,
            initial_r_no,
            &[yes_funding.outpoint],
        )?;
        let lbtc_funding = self.select_funding_utxo(
            &pool_params.lbtc_asset_id,
            initial_r_lbtc,
            &[yes_funding.outpoint, no_funding.outpoint],
        )?;
        let rt_funding = self.select_funding_utxo(
            &pool_params.lp_reissuance_token_id,
            1,
            &[
                yes_funding.outpoint,
                no_funding.outpoint,
                lbtc_funding.outpoint,
            ],
        )?;

        // Compute LP issuance entropy from the original LP creation transaction
        let lp_creation_tx = self.fetch_transaction(lp_creation_txid)?;
        let lp_entropy = crate::assembly::compute_lp_issuance_entropy(
            &lp_creation_tx,
            &rt_funding.asset_blinding_factor,
        )?;

        let (fee_utxo, change_addr) = self.select_fee_utxo_excluding(
            fee_amount,
            &[
                yes_funding.outpoint,
                no_funding.outpoint,
                lbtc_funding.outpoint,
                rt_funding.outpoint,
            ],
        )?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        // Collect wallet inputs in PSET order for blinding (all inputs are from wallet)
        let wallet_inputs: Vec<UnblindedUtxo> = vec![
            yes_funding.clone(),
            no_funding.clone(),
            lbtc_funding.clone(),
            rt_funding.clone(),
            fee_utxo.clone(),
        ];

        let creation_params = crate::amm_pool::pset::creation::PoolCreationParams {
            yes_utxos: vec![yes_funding],
            no_utxos: vec![no_funding],
            lbtc_utxos: vec![lbtc_funding],
            lp_rt_utxo: rt_funding,
            initial_r_yes,
            initial_r_no,
            initial_r_lbtc,
            initial_issued_lp,
            lp_token_destination: change_spk.clone(),
            change_destination: Some(change_spk),
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
            lp_issuance_blinding_nonce: lp_entropy.blinding_nonce,
            lp_issuance_asset_entropy: lp_entropy.entropy,
        };

        let mut pset =
            crate::amm_pool::pset::creation::build_pool_creation_pset(&contract, &creation_params)?;

        // Blind the pool creation PSET.
        //
        // IMPORTANT: blind_last() does NOT blind issuance amounts — they stay
        // explicit (`Value::Explicit`) in the finalized transaction.  When
        // elementsd validates a reissuance it checks:
        //     fConfidential = nAmount.IsCommitment()
        //     expected_token = CalculateReissuanceToken(entropy, fConfidential)
        // Because the amount is explicit, fConfidential = false, so the
        // original LP issuance MUST also have used blind=false for the
        // reissuance token IDs to match.
        //
        // Steps:
        // 1. Fill in the Null RT output (output 3) metadata
        // 2. Set blinding keys on outputs 3+ (RT, LP tokens, change)
        //    but NOT reserves (0-2) or fee (last)
        // 3. Set blinded_issuance=0x00 on the RT input (required by
        //    blind_last to not error; does NOT actually blind the issuance)
        // 4. Provide inp_txout_sec for ALL inputs
        // 5. Call blind_last()
        {
            let lp_rt_id = AssetId::from_slice(&pool_params.lp_reissuance_token_id)
                .map_err(|e| Error::Blinding(format!("bad LP RT asset: {e}")))?;

            // Get blinding pubkey from change address
            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;
            let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
                inner: blinding_pk,
                compressed: true,
            };

            let n_outputs = pset.n_outputs();

            // Fill the Null RT output (output 3) — matches working pattern lines 215-218
            // Then set blinding keys on outputs 3+ (RT, LP tokens, change).
            // Don't blind reserves (0-2) or fee (last).
            let outputs = pset.outputs_mut();
            outputs[3].amount = Some(1);
            outputs[3].asset = Some(lp_rt_id);
            for output in outputs.iter_mut().take(n_outputs - 1).skip(3) {
                output.blinding_key = Some(pset_blinding_key);
                output.blinder_index = Some(0);
            }

            // Set blinded_issuance on the RT input right before blind_last
            pset.inputs_mut()[3].blinded_issuance = Some(0x00);

            // Provide input txout secrets for ALL inputs
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
        }

        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        let covenant_address = contract
            .address(initial_issued_lp, self.network.address_params())
            .to_string();

        Ok(PoolCreationResult {
            txid,
            pool_params: *pool_params,
            issued_lp: initial_issued_lp,
            covenant_address,
        })
    }

    /// Execute a swap against an AMM pool.
    ///
    /// - `sell_a = false`: sell B (second-named in pair), receive A (first-named).
    /// - `sell_a = true`: sell A (first-named in pair), receive B (second-named).
    pub fn pool_swap(
        &mut self,
        pool_params: &crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        swap_pair: crate::amm_pool::math::SwapPair,
        delta_in: u64,
        sell_a: bool,
        fee_amount: u64,
    ) -> Result<PoolSwapResult> {
        use crate::amm_pool::math::{PoolReserves, compute_swap_exact_input};

        self.sync()?;
        let contract = crate::amm_pool::contract::CompiledAmmPool::new(*pool_params)?;

        let (pool_yes, pool_no, pool_lbtc, pool_rt) = self.scan_pool_utxos(&contract, issued_lp)?;

        let reserves = PoolReserves {
            r_yes: pool_yes.value,
            r_no: pool_no.value,
            r_lbtc: pool_lbtc.value,
        };

        let swap_result =
            compute_swap_exact_input(&reserves, swap_pair, delta_in, pool_params.fee_bps, sell_a)?;

        // Determine which asset the trader sends in and receives out based on
        // the pair and sell_a flag (no need to inspect reserve changes).
        let (trader_send_asset, trader_receive_asset) = if sell_a {
            // Sell A (first-named), receive B (second-named)
            match swap_pair {
                crate::amm_pool::math::SwapPair::YesNo => {
                    (pool_params.yes_asset_id, pool_params.no_asset_id)
                }
                crate::amm_pool::math::SwapPair::YesLbtc => {
                    (pool_params.yes_asset_id, pool_params.lbtc_asset_id)
                }
                crate::amm_pool::math::SwapPair::NoLbtc => {
                    (pool_params.no_asset_id, pool_params.lbtc_asset_id)
                }
            }
        } else {
            // Sell B (second-named), receive A (first-named)
            match swap_pair {
                crate::amm_pool::math::SwapPair::YesNo => {
                    (pool_params.no_asset_id, pool_params.yes_asset_id)
                }
                crate::amm_pool::math::SwapPair::YesLbtc => {
                    (pool_params.lbtc_asset_id, pool_params.yes_asset_id)
                }
                crate::amm_pool::math::SwapPair::NoLbtc => {
                    (pool_params.lbtc_asset_id, pool_params.no_asset_id)
                }
            }
        };

        let trader_funding = self.select_funding_utxo(&trader_send_asset, delta_in, &[])?;
        let (fee_utxo, change_addr) =
            self.select_fee_utxo_excluding(fee_amount, &[trader_funding.outpoint])?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        let swap_params = crate::amm_pool::pset::swap::SwapParams {
            pool_yes_utxo: pool_yes,
            pool_no_utxo: pool_no,
            pool_lbtc_utxo: pool_lbtc,
            pool_lp_rt_utxo: pool_rt,
            issued_lp,
            trader_utxos: vec![trader_funding],
            swap_pair,
            new_r_yes: swap_result.new_reserves.r_yes,
            new_r_no: swap_result.new_reserves.r_no,
            new_r_lbtc: swap_result.new_reserves.r_lbtc,
            trader_receive_asset,
            trader_receive_amount: swap_result.delta_out,
            trader_receive_destination: change_spk.clone(),
            trader_change_destination: Some(change_spk),
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
        };

        let mut pset = crate::amm_pool::pset::swap::build_swap_pset(&contract, &swap_params)?;

        // Blind wallet-destination outputs AND the RT output (index 3).
        //
        // The AMM pool covenant expects the RT output to be a confidential
        // Pedersen commitment.  We must blind it here and then extract the
        // blinding factors for the Simplicity witness.
        //
        // NOTE: blinding MUST happen before witness attachment so that
        // pset_to_pruning_transaction (called inside attach_amm_pool_witnesses)
        // sees the confidential RT output.
        {
            let lp_rt_id = AssetId::from_slice(&pool_params.lp_reissuance_token_id)
                .map_err(|e| Error::Blinding(format!("bad LP RT asset: {e}")))?;

            // Fill the Null RT output placeholder (output 3).
            let outputs = pset.outputs_mut();
            outputs[3].amount = Some(1);
            outputs[3].asset = Some(lp_rt_id);

            let mut all_inputs: Vec<(usize, &UnblindedUtxo)> = vec![
                (0, &swap_params.pool_yes_utxo),
                (1, &swap_params.pool_no_utxo),
                (2, &swap_params.pool_lbtc_utxo),
                (3, &swap_params.pool_lp_rt_utxo),
            ];
            for (i, utxo) in swap_params.trader_utxos.iter().enumerate() {
                all_inputs.push((4 + i, utxo));
            }
            all_inputs.push((4 + swap_params.trader_utxos.len(), &swap_params.fee_utxo));
            let n_outputs = pset.n_outputs();
            let mut blind_indices: Vec<usize> = vec![3]; // RT output
            blind_indices.extend(4..n_outputs - 1); // wallet outputs
            self.blind_pool_pset(&mut pset, &all_inputs, &blind_indices, &change_addr)?;
        }

        // Extract the blinding factors that blind_last() chose for the RT
        // output (index 3), then attach Simplicity witnesses.
        let rt_txout = pset.outputs()[3].to_txout();
        let (_, _, rt_out_abf, rt_out_vbf) = self.unblind_covenant_utxo(&rt_txout)?;

        let rt_bf = crate::amm_pool::witness::RtBlindingFactors {
            input_abf: swap_params.pool_lp_rt_utxo.asset_blinding_factor,
            input_vbf: swap_params.pool_lp_rt_utxo.value_blinding_factor,
            output_abf: rt_out_abf,
            output_vbf: rt_out_vbf,
        };
        let spending_path = crate::amm_pool::witness::AmmPoolSpendingPath::Swap {
            swap_pair,
            issued_lp,
            blinding: rt_bf,
        };
        crate::amm_pool::assembly::attach_amm_pool_witnesses(
            &mut pset,
            &contract,
            issued_lp,
            spending_path,
        )?;

        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(PoolSwapResult {
            txid,
            delta_in: swap_result.delta_in,
            delta_out: swap_result.delta_out,
            new_reserves: swap_result.new_reserves,
        })
    }

    /// Deposit liquidity into an AMM pool (mint LP tokens).
    ///
    /// Selects separate funding UTXOs for each asset being deposited
    /// (YES, NO, L-BTC) as required by the three-asset pool covenant.
    #[allow(clippy::too_many_arguments)]
    pub fn pool_lp_deposit(
        &mut self,
        pool_params: &crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        new_r_yes: u64,
        new_r_no: u64,
        new_r_lbtc: u64,
        lp_mint_amount: u64,
        fee_amount: u64,
        lp_creation_txid: &Txid,
    ) -> Result<PoolLpResult> {
        use crate::amm_pool::math::PoolReserves;

        self.sync()?;
        let contract = crate::amm_pool::contract::CompiledAmmPool::new(*pool_params)?;

        let (pool_yes, pool_no, pool_lbtc, pool_rt) = self.scan_pool_utxos(&contract, issued_lp)?;

        // Compute LP issuance entropy from the original LP creation transaction.
        // The RT at the pool covenant is explicit (ABF = zeros).
        let lp_creation_tx = self.fetch_transaction(lp_creation_txid)?;
        let lp_entropy = crate::assembly::compute_lp_issuance_entropy(
            &lp_creation_tx,
            &pool_rt.asset_blinding_factor,
        )?;

        // Compute what the depositor needs to contribute for each asset
        let deposit_yes = new_r_yes.saturating_sub(pool_yes.value);
        let deposit_no = new_r_no.saturating_sub(pool_no.value);
        let deposit_lbtc = new_r_lbtc.saturating_sub(pool_lbtc.value);

        // Select separate funding UTXOs for each asset being deposited
        let mut exclude = Vec::new();
        let mut deposit_utxos = Vec::new();

        if deposit_yes > 0 {
            let utxo =
                self.select_funding_utxo(&pool_params.yes_asset_id, deposit_yes, &exclude)?;
            exclude.push(utxo.outpoint);
            deposit_utxos.push(utxo);
        }
        if deposit_no > 0 {
            let utxo = self.select_funding_utxo(&pool_params.no_asset_id, deposit_no, &exclude)?;
            exclude.push(utxo.outpoint);
            deposit_utxos.push(utxo);
        }
        if deposit_lbtc > 0 {
            let utxo =
                self.select_funding_utxo(&pool_params.lbtc_asset_id, deposit_lbtc, &exclude)?;
            exclude.push(utxo.outpoint);
            deposit_utxos.push(utxo);
        }

        let (fee_utxo, change_addr) = self.select_fee_utxo_excluding(fee_amount, &exclude)?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        let deposit_params = crate::amm_pool::pset::lp_deposit::LpDepositParams {
            pool_yes_utxo: pool_yes,
            pool_no_utxo: pool_no,
            pool_lbtc_utxo: pool_lbtc,
            pool_lp_rt_utxo: pool_rt.clone(),
            issued_lp,
            deposit_utxos,
            new_r_yes,
            new_r_no,
            new_r_lbtc,
            lp_mint_amount,
            lp_token_destination: change_spk.clone(),
            change_destination: Some(change_spk),
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
            lp_issuance_blinding_nonce: lp_entropy.blinding_nonce,
            lp_issuance_asset_entropy: lp_entropy.entropy,
        };

        let new_issued_lp = issued_lp
            .checked_add(lp_mint_amount)
            .ok_or_else(|| Error::AmmPool("issued_lp overflow".into()))?;

        let mut pset =
            crate::amm_pool::pset::lp_deposit::build_lp_deposit_pset(&contract, &deposit_params)?;

        // Blind wallet-destination outputs AND the RT output (index 3).
        //
        // The RT output is a Null placeholder from the PSET builder.
        // Like pool creation, we must fill it and set blinded_issuance
        // before calling blind_last().
        {
            let lp_rt_id = AssetId::from_slice(&pool_params.lp_reissuance_token_id)
                .map_err(|e| Error::Blinding(format!("bad LP RT asset: {e}")))?;

            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| Error::Blinding("change address has no blinding key".to_string()))?;
            let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
                inner: blinding_pk,
                compressed: true,
            };

            // Fill the Null RT output (output 3)
            let outputs = pset.outputs_mut();
            outputs[3].amount = Some(1);
            outputs[3].asset = Some(lp_rt_id);
            outputs[3].blinding_key = Some(pset_blinding_key);
            outputs[3].blinder_index = Some(0);

            // Set blinded_issuance on input 3 (required by blind_last for reissuance)
            pset.inputs_mut()[3].blinded_issuance = Some(0x00);

            let mut all_inputs: Vec<(usize, &UnblindedUtxo)> = vec![
                (0, &deposit_params.pool_yes_utxo),
                (1, &deposit_params.pool_no_utxo),
                (2, &deposit_params.pool_lbtc_utxo),
                (3, &deposit_params.pool_lp_rt_utxo),
            ];
            for (i, utxo) in deposit_params.deposit_utxos.iter().enumerate() {
                all_inputs.push((4 + i, utxo));
            }
            all_inputs.push((
                4 + deposit_params.deposit_utxos.len(),
                &deposit_params.fee_utxo,
            ));
            let n_outputs = pset.n_outputs();
            // Blind RT (3) + wallet outputs (4+), skip reserves (0-2) and fee (last)
            let blind_indices: Vec<usize> = (3..n_outputs - 1).collect();
            self.blind_pool_pset(&mut pset, &all_inputs, &blind_indices, &change_addr)?;
        }

        // Extract the RT output blinding factors, then attach witnesses.
        let rt_txout = pset.outputs()[3].to_txout();
        let (_, _, rt_out_abf, rt_out_vbf) = self.unblind_covenant_utxo(&rt_txout)?;

        let rt_bf = crate::amm_pool::witness::RtBlindingFactors {
            input_abf: pool_rt.asset_blinding_factor,
            input_vbf: pool_rt.value_blinding_factor,
            output_abf: rt_out_abf,
            output_vbf: rt_out_vbf,
        };
        let spending_path = crate::amm_pool::witness::AmmPoolSpendingPath::LpDepositWithdraw {
            issued_lp,
            blinding: rt_bf,
        };
        crate::amm_pool::assembly::attach_amm_pool_witnesses(
            &mut pset,
            &contract,
            issued_lp,
            spending_path,
        )?;

        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(PoolLpResult {
            txid,
            new_issued_lp,
            new_reserves: PoolReserves {
                r_yes: new_r_yes,
                r_no: new_r_no,
                r_lbtc: new_r_lbtc,
            },
        })
    }

    /// Withdraw liquidity from an AMM pool (burn LP tokens).
    pub fn pool_lp_withdraw(
        &mut self,
        pool_params: &crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        lp_burn_amount: u64,
        fee_amount: u64,
    ) -> Result<PoolLpResult> {
        use crate::amm_pool::math::{PoolReserves, compute_lp_proportional_withdraw};

        self.sync()?;
        let contract = crate::amm_pool::contract::CompiledAmmPool::new(*pool_params)?;

        let (pool_yes, pool_no, pool_lbtc, pool_rt) = self.scan_pool_utxos(&contract, issued_lp)?;

        let reserves = PoolReserves {
            r_yes: pool_yes.value,
            r_no: pool_no.value,
            r_lbtc: pool_lbtc.value,
        };

        let withdrawn = compute_lp_proportional_withdraw(&reserves, issued_lp, lp_burn_amount)?;

        let new_r_yes = reserves
            .r_yes
            .checked_sub(withdrawn.r_yes)
            .ok_or_else(|| Error::AmmPool("reserve underflow (YES)".into()))?;
        let new_r_no = reserves
            .r_no
            .checked_sub(withdrawn.r_no)
            .ok_or_else(|| Error::AmmPool("reserve underflow (NO)".into()))?;
        let new_r_lbtc = reserves
            .r_lbtc
            .checked_sub(withdrawn.r_lbtc)
            .ok_or_else(|| Error::AmmPool("reserve underflow (LBTC)".into()))?;

        // Find LP token UTXOs in wallet
        let lp_token_utxos =
            self.find_single_token_utxos(&pool_params.lp_asset_id, lp_burn_amount)?;

        let exclude: Vec<OutPoint> = lp_token_utxos.iter().map(|u| u.outpoint).collect();
        let (fee_utxo, change_addr) = self.select_fee_utxo_excluding(fee_amount, &exclude)?;
        let change_spk = change_addr.script_pubkey();
        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        let new_issued_lp = issued_lp
            .checked_sub(lp_burn_amount)
            .ok_or_else(|| Error::AmmPool("issued_lp underflow".into()))?;

        let withdraw_params = crate::amm_pool::pset::lp_withdraw::LpWithdrawParams {
            pool_yes_utxo: pool_yes,
            pool_no_utxo: pool_no,
            pool_lbtc_utxo: pool_lbtc,
            pool_lp_rt_utxo: pool_rt.clone(),
            issued_lp,
            lp_token_utxos,
            lp_burn_amount,
            new_r_yes,
            new_r_no,
            new_r_lbtc,
            withdraw_destination: change_spk,
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
        };

        let mut pset = crate::amm_pool::pset::lp_withdraw::build_lp_withdraw_pset(
            &contract,
            &withdraw_params,
        )?;

        // Blind wallet-destination outputs AND the RT output (index 3).
        // Outputs 0-2 = reserves (explicit), 3 = RT (must be confidential),
        // 4 = LP burn (OP_RETURN, not blinded), 5+ = wallet, last = fee.
        {
            let lp_rt_id = AssetId::from_slice(&pool_params.lp_reissuance_token_id)
                .map_err(|e| Error::Blinding(format!("bad LP RT asset: {e}")))?;

            // Fill the Null RT output placeholder (output 3).
            let outputs = pset.outputs_mut();
            outputs[3].amount = Some(1);
            outputs[3].asset = Some(lp_rt_id);

            let mut all_inputs: Vec<(usize, &UnblindedUtxo)> = vec![
                (0, &withdraw_params.pool_yes_utxo),
                (1, &withdraw_params.pool_no_utxo),
                (2, &withdraw_params.pool_lbtc_utxo),
                (3, &withdraw_params.pool_lp_rt_utxo),
            ];
            for (i, utxo) in withdraw_params.lp_token_utxos.iter().enumerate() {
                all_inputs.push((4 + i, utxo));
            }
            all_inputs.push((
                4 + withdraw_params.lp_token_utxos.len(),
                &withdraw_params.fee_utxo,
            ));
            let n_outputs = pset.n_outputs();
            // Blind RT (3) + wallet outputs (5+), skip reserves (0-2), LP burn (4), fee (last)
            let mut blind_indices: Vec<usize> = vec![3];
            blind_indices.extend(5..n_outputs - 1);
            self.blind_pool_pset(&mut pset, &all_inputs, &blind_indices, &change_addr)?;
        }

        // Extract the RT output blinding factors, then attach witnesses.
        let rt_txout = pset.outputs()[3].to_txout();
        let (_, _, rt_out_abf, rt_out_vbf) = self.unblind_covenant_utxo(&rt_txout)?;

        let rt_bf = crate::amm_pool::witness::RtBlindingFactors {
            input_abf: pool_rt.asset_blinding_factor,
            input_vbf: pool_rt.value_blinding_factor,
            output_abf: rt_out_abf,
            output_vbf: rt_out_vbf,
        };
        let spending_path = crate::amm_pool::witness::AmmPoolSpendingPath::LpDepositWithdraw {
            issued_lp,
            blinding: rt_bf,
        };
        crate::amm_pool::assembly::attach_amm_pool_witnesses(
            &mut pset,
            &contract,
            issued_lp,
            spending_path,
        )?;

        let tx = self.sign_pset(pset)?;
        let txid = self.broadcast_and_sync(&tx)?;

        Ok(PoolLpResult {
            txid,
            new_issued_lp,
            new_reserves: PoolReserves {
                r_yes: new_r_yes,
                r_no: new_r_no,
                r_lbtc: new_r_lbtc,
            },
        })
    }

    // ── Trade routing: combined AMM + limit order execution ──────────────

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

        let policy_bytes: [u8; 32] = self.policy_asset().into_inner().to_byte_array();

        // 1. Compile contracts
        let pool_contract = plan
            .pool_leg
            .as_ref()
            .map(|leg| crate::amm_pool::contract::CompiledAmmPool::new(leg.pool_params))
            .transpose()?;

        let order_contracts: Vec<CompiledMakerOrder> = plan
            .order_legs
            .iter()
            .map(|leg| CompiledMakerOrder::new(leg.params))
            .collect::<Result<Vec<_>>>()?;

        // 2. Collect outpoints to exclude from wallet UTXO selection
        let mut exclude: Vec<OutPoint> = Vec::new();
        if let Some(ref pool_leg) = plan.pool_leg {
            exclude.push(pool_leg.pool_utxos.yes.outpoint);
            exclude.push(pool_leg.pool_utxos.no.outpoint);
            exclude.push(pool_leg.pool_utxos.lbtc.outpoint);
            exclude.push(pool_leg.pool_utxos.rt.outpoint);
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
            pool_contract: pool_contract.as_ref(),
            order_contracts: &order_contracts,
            taker_funding_utxos: vec![taker_funding],
            fee_utxo,
            fee_amount,
            fee_asset_id: policy_bytes,
            taker_receive_destination: change_spk,
            taker_change_destination: taker_change,
        })?;
        let mut pset = pset_result.pset;

        // 6. Blind wallet-destination outputs (and RT output when pool leg present).
        //
        // The AMM pool covenant expects the RT output (index 3) to be a
        // confidential Pedersen commitment.  Pool creation blinds it via
        // blind_last(), so the on-chain RT UTXO is confidential.  We must
        // keep it confidential when cycling it through the swap/trade tx.
        let mut blind_indices = pset_result.blind_output_indices.clone();
        if let Some(ref pool_leg) = plan.pool_leg {
            // Fill the Null RT output placeholder (output 3).
            let lp_rt_id = AssetId::from_slice(&pool_leg.pool_params.lp_reissuance_token_id)
                .map_err(|e| Error::Blinding(format!("bad LP RT asset: {e}")))?;
            let outputs = pset.outputs_mut();
            outputs[3].amount = Some(1);
            outputs[3].asset = Some(lp_rt_id);

            blind_indices.push(3);
            blind_indices.sort_unstable();
        }
        self.blind_order_pset(
            &mut pset,
            &pset_result.all_input_utxos,
            &blind_indices,
            &change_addr,
        )?;

        // 7. Attach AMM pool witnesses (if pool leg present)
        if let Some(ref pool_leg) = plan.pool_leg {
            let contract = pool_contract.as_ref().unwrap();

            // Extract the blinding factors that blind_last() chose for
            // output 3 (the RT).  The covenant witness needs them to
            // verify the Pedersen commitment.
            let rt_txout = pset.outputs()[3].to_txout();
            let (_, _, rt_out_abf, rt_out_vbf) = self.unblind_covenant_utxo(&rt_txout)?;

            let rt_bf = crate::amm_pool::witness::RtBlindingFactors {
                input_abf: pool_leg.pool_utxos.rt.asset_blinding_factor,
                input_vbf: pool_leg.pool_utxos.rt.value_blinding_factor,
                output_abf: rt_out_abf,
                output_vbf: rt_out_vbf,
            };
            let spending_path = crate::amm_pool::witness::AmmPoolSpendingPath::Swap {
                swap_pair: pool_leg.swap_pair,
                issued_lp: pool_leg.issued_lp,
                blinding: rt_bf,
            };
            crate::amm_pool::assembly::attach_amm_pool_witnesses(
                &mut pset,
                contract,
                pool_leg.issued_lp,
                spending_path,
            )?;
        }

        // 8. Attach maker order witnesses for each order input
        {
            use crate::assembly::pset_to_pruning_transaction;
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
            pool_used: plan.pool_leg.is_some(),
            new_reserves: plan.pool_leg.as_ref().map(|l| l.new_reserves),
        })
    }

    /// Find the explicit collateral UTXO from a set of covenant UTXOs.
    fn find_collateral_utxo(
        covenant_utxos: &[(OutPoint, TxOut)],
        params: &ContractParams,
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
}

// ── Private helpers ──────────────────────────────────────────────────────

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
}
