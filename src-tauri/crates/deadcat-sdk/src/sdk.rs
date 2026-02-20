use std::collections::HashMap;
use std::path::Path;

use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::blocking::BlockchainBackend;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp::{self, Keypair};
use lwk_wollet::elements::{AssetId, Transaction, Txid};
use lwk_wollet::{
    ElectrumClient, ElectrumUrl, TxBuilder, WalletTx, WalletTxOut, Wollet, WolletDescriptor,
};
use rand::thread_rng;

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::network::Network;
use crate::params::ContractParams;
use crate::pset::UnblindedUtxo;
use crate::pset::creation::{CreationParams, build_creation_pset};

pub struct DeadcatSdk {
    signer: SwSigner,
    wollet: Wollet,
    network: Network,
    electrum_url: String,
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
            electrum_url: electrum_url.to_string(),
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
            .electrum_url
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let mut client =
            ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
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

    pub fn address(
        &self,
        index: Option<u32>,
    ) -> Result<lwk_wollet::AddressResult> {
        self.wollet
            .address(index)
            .map_err(|e| Error::Query(e.to_string()))
    }

    pub fn utxos(&self) -> Result<Vec<WalletTxOut>> {
        self.wollet
            .utxos()
            .map_err(|e| Error::Query(e.to_string()))
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
        let url: ElectrumUrl = self
            .electrum_url
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let mut client =
            ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
        let txid = client
            .broadcast(tx)
            .map_err(|e| Error::Broadcast(e.to_string()))?;
        lwk_wollet::full_scan_with_electrum_client(&mut self.wollet, &mut client)
            .map_err(|e| Error::Electrum(e.to_string()))?;
        Ok(txid)
    }

    pub fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction> {
        let url: ElectrumUrl = self
            .electrum_url
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let client =
            ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
        let txs = client
            .get_transactions(&[*txid])
            .map_err(|e| Error::Electrum(e.to_string()))?;
        txs.into_iter()
            .next()
            .ok_or_else(|| Error::Query(format!("transaction {} not found", txid)))
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn electrum_url(&self) -> &str {
        &self.electrum_url
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

        let (yes_utxo, no_utxo) =
            select_defining_utxos(&raw_utxos, policy_asset, min_utxo_value)?;

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
    use lwk_wollet::elements::bitcoin::hashes::Hash;
    use lwk_wollet::elements::confidential::{AssetBlindingFactor, ValueBlindingFactor};
    use lwk_wollet::elements::{AddressParams, OutPoint, Script, TxOutSecrets};
    use lwk_wollet::Chain;

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
}
