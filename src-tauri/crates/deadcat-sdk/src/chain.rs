use simplicityhl::elements::{OutPoint, Script, Transaction, TxOut, Txid};

use crate::error::{Error, Result};

/// A single entry returned by `get_script_history`.
#[derive(Debug, Clone)]
pub struct ScriptHistoryEntry {
    pub txid: Txid,
    /// Block height. -1 or 0 means unconfirmed (mempool), >0 means confirmed.
    pub height: i32,
}

/// Backend for interacting with the Liquid blockchain.
pub trait ChainBackend {
    /// Scan a script pubkey for unspent outputs.
    fn scan_script_utxos(&self, script_pubkey: &Script) -> Result<Vec<(OutPoint, TxOut)>>;

    /// Fetch a transaction by its txid.
    fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction>;

    /// Broadcast a signed transaction and return its txid.
    fn broadcast(&self, tx: &Transaction) -> Result<Txid>;

    /// Get the full transaction history for a script pubkey (confirmed + unconfirmed).
    fn get_script_history(&self, script_pubkey: &Script) -> Result<Vec<ScriptHistoryEntry>>;
}

/// Electrum-based chain backend for Liquid.
pub struct ElectrumBackend {
    electrum_url: String,
}

impl ElectrumBackend {
    pub fn new(electrum_url: &str) -> Self {
        Self {
            electrum_url: electrum_url.to_string(),
        }
    }

    pub fn electrum_url(&self) -> &str {
        &self.electrum_url
    }
}

impl ChainBackend for ElectrumBackend {
    fn scan_script_utxos(&self, script_pubkey: &Script) -> Result<Vec<(OutPoint, TxOut)>> {
        use electrum_client::ElectrumApi;
        use sha2::{Digest, Sha256};

        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_pubkey.to_bytes());

        let client = electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        // Electrum script hash = SHA256(scriptPubKey) with reversed byte order.
        let mut hash = Sha256::digest(btc_script.as_bytes()).to_vec();
        hash.reverse();
        let script_hash_hex = hex::encode(&hash);

        let resp = client
            .raw_call(
                "blockchain.scripthash.listunspent",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        let entries = resp
            .as_array()
            .ok_or_else(|| Error::CovenantScan("expected array response".into()))?;

        let mut results = Vec::new();
        for entry in entries {
            let tx_hash_hex = entry["tx_hash"]
                .as_str()
                .ok_or_else(|| Error::CovenantScan("missing tx_hash".into()))?;
            let tx_pos = entry["tx_pos"]
                .as_u64()
                .ok_or_else(|| Error::CovenantScan("missing tx_pos".into()))?
                as usize;

            let txid: Txid = tx_hash_hex
                .parse()
                .map_err(|e| Error::CovenantScan(format!("bad tx_hash: {e}")))?;

            let tx = self.fetch_transaction(&txid)?;
            let txout = tx
                .output
                .get(tx_pos)
                .ok_or_else(|| Error::CovenantScan("vout out of range".into()))?
                .clone();

            let outpoint = OutPoint::new(txid, tx_pos as u32);
            results.push((outpoint, txout));
        }
        Ok(results)
    }

    fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction> {
        use lwk_wollet::blocking::BlockchainBackend;

        let url: lwk_wollet::ElectrumUrl = self
            .electrum_url
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let client =
            lwk_wollet::ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
        let txs = client
            .get_transactions(&[*txid])
            .map_err(|e| Error::Electrum(e.to_string()))?;
        txs.into_iter()
            .next()
            .ok_or_else(|| Error::Query(format!("transaction {} not found", txid)))
    }

    fn broadcast(&self, tx: &Transaction) -> Result<Txid> {
        use lwk_wollet::blocking::BlockchainBackend;

        let url: lwk_wollet::ElectrumUrl = self
            .electrum_url
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let client =
            lwk_wollet::ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;
        client
            .broadcast(tx)
            .map_err(|e| Error::Broadcast(e.to_string()))
    }

    fn get_script_history(&self, script_pubkey: &Script) -> Result<Vec<ScriptHistoryEntry>> {
        use electrum_client::ElectrumApi;
        use sha2::{Digest, Sha256};

        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_pubkey.to_bytes());

        let client = electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        // Same script-hash derivation as scan_script_utxos
        let mut hash = Sha256::digest(btc_script.as_bytes()).to_vec();
        hash.reverse();
        let script_hash_hex = hex::encode(&hash);

        let resp = client
            .raw_call(
                "blockchain.scripthash.get_history",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        let entries = resp
            .as_array()
            .ok_or_else(|| Error::CovenantScan("expected array response".into()))?;

        let mut results = Vec::new();
        for entry in entries {
            let tx_hash_hex = entry["tx_hash"]
                .as_str()
                .ok_or_else(|| Error::CovenantScan("missing tx_hash".into()))?;
            let height = entry["height"]
                .as_i64()
                .ok_or_else(|| Error::CovenantScan("missing height".into()))?
                as i32;

            let txid: Txid = tx_hash_hex
                .parse()
                .map_err(|e| Error::CovenantScan(format!("bad tx_hash: {e}")))?;

            results.push(ScriptHistoryEntry { txid, height });
        }
        Ok(results)
    }
}
