use std::time::Duration;

use simplicityhl::elements::{OutPoint, Script, Transaction, TxOut, Txid};

use crate::error::{Error, Result};

/// Backend for interacting with the Liquid blockchain.
pub trait ChainBackend {
    /// Return the current best block height.
    fn best_block_height(&self) -> Result<u32>;

    /// Scan a script pubkey for unspent outputs.
    fn scan_script_utxos(&self, script_pubkey: &Script) -> Result<Vec<(OutPoint, TxOut)>>;

    /// Fetch transaction history touching a script pubkey.
    fn script_history_txids(&self, script_pubkey: &Script) -> Result<Vec<Txid>>;

    /// Fetch a transaction by its txid.
    fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction>;

    /// Return the confirmed block height for a transaction, if known.
    fn transaction_height(&self, txid: &Txid) -> Result<Option<u32>>;

    /// Broadcast a signed transaction and return its txid.
    fn broadcast(&self, tx: &Transaction) -> Result<Txid>;
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

    fn is_transient_missing_tx_error(msg: &str) -> bool {
        let lower = msg.to_ascii_lowercase();
        lower.contains("missing transaction")
            || lower.contains("no such mempool or blockchain transaction")
    }

    fn script_hash_hex(script_pubkey: &[u8]) -> String {
        use sha2::{Digest, Sha256};

        let mut hash = Sha256::digest(script_pubkey).to_vec();
        hash.reverse();
        hex::encode(&hash)
    }
}

impl ChainBackend for ElectrumBackend {
    fn best_block_height(&self) -> Result<u32> {
        use electrum_client::ElectrumApi;

        let client = electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| Error::Electrum(e.to_string()))?;
        let resp = client
            .raw_call("blockchain.headers.subscribe", [])
            .map_err(|e| Error::Electrum(e.to_string()))?;
        let height = resp["height"]
            .as_u64()
            .ok_or_else(|| Error::Query("missing height in headers response".into()))?;
        Ok(height as u32)
    }

    fn scan_script_utxos(&self, script_pubkey: &Script) -> Result<Vec<(OutPoint, TxOut)>> {
        use electrum_client::ElectrumApi;

        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_pubkey.to_bytes());

        let client = electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        let script_hash_hex = Self::script_hash_hex(btc_script.as_bytes());

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

    fn script_history_txids(&self, script_pubkey: &Script) -> Result<Vec<Txid>> {
        use electrum_client::ElectrumApi;

        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_pubkey.to_bytes());

        let client = electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        let script_hash_hex = Self::script_hash_hex(btc_script.as_bytes());

        let resp = client
            .raw_call(
                "blockchain.scripthash.get_history",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| Error::CovenantScan(e.to_string()))?;

        let entries = resp
            .as_array()
            .ok_or_else(|| Error::CovenantScan("expected array response".into()))?;

        let mut txids = Vec::new();
        for entry in entries {
            let tx_hash_hex = entry["tx_hash"]
                .as_str()
                .ok_or_else(|| Error::CovenantScan("missing tx_hash".into()))?;
            let txid: Txid = tx_hash_hex
                .parse()
                .map_err(|e| Error::CovenantScan(format!("bad tx_hash: {e}")))?;
            txids.push(txid);
        }
        Ok(txids)
    }

    fn fetch_transaction(&self, txid: &Txid) -> Result<Transaction> {
        use lwk_wollet::blocking::BlockchainBackend;

        let url: lwk_wollet::ElectrumUrl = self
            .electrum_url
            .parse()
            .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
        let client =
            lwk_wollet::ElectrumClient::new(&url).map_err(|e| Error::Electrum(e.to_string()))?;

        const MAX_ATTEMPTS: usize = 10;
        const RETRY_DELAY: Duration = Duration::from_millis(350);

        for attempt in 0..MAX_ATTEMPTS {
            match client.get_transactions(&[*txid]) {
                Ok(txs) => {
                    return txs
                        .into_iter()
                        .next()
                        .ok_or_else(|| Error::Query(format!("transaction {} not found", txid)));
                }
                Err(err) => {
                    let msg = err.to_string();
                    let can_retry =
                        attempt + 1 < MAX_ATTEMPTS && Self::is_transient_missing_tx_error(&msg);
                    if can_retry {
                        std::thread::sleep(RETRY_DELAY);
                        continue;
                    }
                    return Err(Error::Electrum(msg));
                }
            }
        }

        Err(Error::Electrum(format!(
            "failed to fetch transaction {txid} after {MAX_ATTEMPTS} attempts"
        )))
    }

    fn transaction_height(&self, txid: &Txid) -> Result<Option<u32>> {
        use electrum_client::ElectrumApi;

        let client = electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| Error::Electrum(e.to_string()))?;
        let tx = self.fetch_transaction(txid)?;
        let first_output = tx
            .output
            .first()
            .ok_or_else(|| Error::Query(format!("transaction {txid} has no outputs")))?;
        let script_hash_hex = Self::script_hash_hex(first_output.script_pubkey.as_bytes());
        let history = client
            .raw_call(
                "blockchain.scripthash.get_history",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| Error::Electrum(e.to_string()))?;
        let entries = history
            .as_array()
            .ok_or_else(|| Error::Query("expected array response".into()))?;
        for entry in entries {
            let tx_hash_hex = entry["tx_hash"]
                .as_str()
                .ok_or_else(|| Error::Query("missing tx_hash".into()))?;
            if tx_hash_hex == txid.to_string() {
                let height = entry["height"]
                    .as_i64()
                    .ok_or_else(|| Error::Query("missing height".into()))?;
                if height <= 0 {
                    return Ok(None);
                }
                return Ok(Some(height as u32));
            }
        }
        Ok(None)
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
}
