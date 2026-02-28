use std::cell::RefCell;

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

// ── Thread-local client caches ──────────────────────────────────────────────
//
// `electrum_client::Client` and `lwk_wollet::ElectrumClient` are `!Send`,
// so they cannot live inside the `Send` `ElectrumBackend` struct. Thread-local
// storage sidesteps this: the client never crosses thread boundaries.
//
// All SDK calls are serialized through `spawn_blocking` on Tokio's blocking
// pool, which reuses OS threads — so the cached client persists across calls.
//
// Each cache entry is keyed by URL so a URL change (e.g. network switch)
// transparently creates a fresh connection.

thread_local! {
    /// Cache for `electrum_client::Client` (used by `scan_script_utxos`, `get_script_history`).
    static RAW_CLIENT: RefCell<Option<(String, electrum_client::Client)>> = const { RefCell::new(None) };

    /// Cache for `lwk_wollet::ElectrumClient` (used by `fetch_transaction`, `broadcast`).
    static LWK_CLIENT: RefCell<Option<(String, lwk_wollet::ElectrumClient)>> = const { RefCell::new(None) };
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

    /// Run `f` against a cached `electrum_client::Client`, creating one if
    /// needed (or if the URL changed). On error the cache is cleared so the
    /// next call gets a fresh connection.
    fn with_raw_client<R>(
        &self,
        f: impl FnOnce(&electrum_client::Client) -> Result<R>,
    ) -> Result<R> {
        RAW_CLIENT.with(|cell| {
            // Create / replace the client if absent or URL changed.
            {
                let mut slot = cell.borrow_mut();
                let needs_new = match slot.as_ref() {
                    Some((url, _)) => url != &self.electrum_url,
                    None => true,
                };
                if needs_new {
                    let client = electrum_client::Client::new(&self.electrum_url)
                        .map_err(|e| Error::CovenantScan(e.to_string()))?;
                    *slot = Some((self.electrum_url.clone(), client));
                }
            }

            // Borrow immutably for the closure.
            let slot = cell.borrow();
            let (_, client) = slot.as_ref().expect("just ensured Some");
            let result = f(client);

            // On error, clear the cache to force reconnect next call.
            if result.is_err() {
                drop(slot);
                *cell.borrow_mut() = None;
            }

            result
        })
    }

    /// Run `f` against a cached `lwk_wollet::ElectrumClient`, creating one if
    /// needed (or if the URL changed). On error the cache is cleared.
    fn with_lwk_client<R>(
        &self,
        f: impl FnOnce(&lwk_wollet::ElectrumClient) -> Result<R>,
    ) -> Result<R> {
        LWK_CLIENT.with(|cell| {
            {
                let mut slot = cell.borrow_mut();
                let needs_new = match slot.as_ref() {
                    Some((url, _)) => url != &self.electrum_url,
                    None => true,
                };
                if needs_new {
                    let url: lwk_wollet::ElectrumUrl = self
                        .electrum_url
                        .parse()
                        .map_err(|e| Error::Electrum(format!("{:?}", e)))?;
                    let client = lwk_wollet::ElectrumClient::new(&url)
                        .map_err(|e| Error::Electrum(e.to_string()))?;
                    *slot = Some((self.electrum_url.clone(), client));
                }
            }

            let slot = cell.borrow();
            let (_, client) = slot.as_ref().expect("just ensured Some");
            let result = f(client);

            if result.is_err() {
                drop(slot);
                *cell.borrow_mut() = None;
            }

            result
        })
    }
}

impl ChainBackend for ElectrumBackend {
    fn scan_script_utxos(&self, script_pubkey: &Script) -> Result<Vec<(OutPoint, TxOut)>> {
        use electrum_client::ElectrumApi;
        use sha2::{Digest, Sha256};

        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_pubkey.to_bytes());

        // Electrum script hash = SHA256(scriptPubKey) with reversed byte order.
        let mut hash = Sha256::digest(btc_script.as_bytes()).to_vec();
        hash.reverse();
        let script_hash_hex = hex::encode(&hash);

        // Collect (txid, vout) pairs from the raw client, then drop the borrow.
        let utxo_entries: Vec<(Txid, usize)> = self.with_raw_client(|client| {
            let resp = client
                .raw_call(
                    "blockchain.scripthash.listunspent",
                    [electrum_client::Param::String(script_hash_hex)],
                )
                .map_err(|e| Error::CovenantScan(e.to_string()))?;

            let entries = resp
                .as_array()
                .ok_or_else(|| Error::CovenantScan("expected array response".into()))?;

            let mut pairs = Vec::new();
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

                pairs.push((txid, tx_pos));
            }
            Ok(pairs)
        })?;

        // Fetch full transactions via the LWK client (separate thread-local).
        let mut results = Vec::new();
        for (txid, tx_pos) in utxo_entries {
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

        let txid = *txid;
        self.with_lwk_client(|client| {
            let txs = client
                .get_transactions(&[txid])
                .map_err(|e| Error::Electrum(e.to_string()))?;
            txs.into_iter()
                .next()
                .ok_or_else(|| Error::Query(format!("transaction {} not found", txid)))
        })
    }

    fn broadcast(&self, tx: &Transaction) -> Result<Txid> {
        use lwk_wollet::blocking::BlockchainBackend;

        self.with_lwk_client(|client| {
            client
                .broadcast(tx)
                .map_err(|e| Error::Broadcast(e.to_string()))
        })
    }

    fn get_script_history(&self, script_pubkey: &Script) -> Result<Vec<ScriptHistoryEntry>> {
        use electrum_client::ElectrumApi;
        use sha2::{Digest, Sha256};

        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_pubkey.to_bytes());

        // Same script-hash derivation as scan_script_utxos
        let mut hash = Sha256::digest(btc_script.as_bytes()).to_vec();
        hash.reverse();
        let script_hash_hex = hex::encode(&hash);

        self.with_raw_client(|client| {
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
        })
    }
}
