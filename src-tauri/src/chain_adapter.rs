use deadcat_store::{ChainSource, ChainUtxo};
use lwk_wollet::elements::hashes::Hash as _;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChainAdapterError {
    #[error("electrum error: {0}")]
    Electrum(String),

    #[error("parse error: {0}")]
    Parse(String),
}

/// Adapter that implements `deadcat_store::ChainSource` using the `electrum-client` crate.
pub struct ElectrumChainAdapter {
    electrum_url: String,
}

impl ElectrumChainAdapter {
    pub fn new(electrum_url: &str) -> Self {
        Self {
            electrum_url: electrum_url.to_string(),
        }
    }

    fn client(&self) -> Result<electrum_client::Client, ChainAdapterError> {
        electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| ChainAdapterError::Electrum(e.to_string()))
    }

    fn script_hash_hex(script_pubkey: &[u8]) -> String {
        let mut hash = Sha256::digest(script_pubkey).to_vec();
        hash.reverse();
        hex::encode(&hash)
    }
}

impl ChainSource for ElectrumChainAdapter {
    type Error = ChainAdapterError;

    fn best_block_height(&self) -> Result<u32, Self::Error> {
        use electrum_client::ElectrumApi;

        let client = self.client()?;
        // Use raw_call instead of block_headers_subscribe() because the typed
        // API deserializes headers as Bitcoin, which fails on Liquid/Elements
        // (extra dynafed fields cause "data not consumed entirely").
        let resp = client
            .raw_call("blockchain.headers.subscribe", [])
            .map_err(|e| ChainAdapterError::Electrum(e.to_string()))?;
        let height = resp["height"]
            .as_u64()
            .ok_or_else(|| ChainAdapterError::Parse("missing height in headers response".into()))?;
        Ok(height as u32)
    }

    fn list_unspent(&self, script_pubkey: &[u8]) -> Result<Vec<ChainUtxo>, Self::Error> {
        use electrum_client::ElectrumApi;

        let client = self.client()?;
        let script_hash_hex = Self::script_hash_hex(script_pubkey);

        let resp = client
            .raw_call(
                "blockchain.scripthash.listunspent",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| ChainAdapterError::Electrum(e.to_string()))?;

        let entries = resp
            .as_array()
            .ok_or_else(|| ChainAdapterError::Parse("expected array response".into()))?;

        let mut results = Vec::new();
        for entry in entries {
            let tx_hash_hex = entry["tx_hash"]
                .as_str()
                .ok_or_else(|| ChainAdapterError::Parse("missing tx_hash".into()))?;
            let tx_pos = entry["tx_pos"]
                .as_u64()
                .ok_or_else(|| ChainAdapterError::Parse("missing tx_pos".into()))?
                as u32;
            // Electrum returns height 0 for unconfirmed; map to None
            let height =
                entry["height"]
                    .as_u64()
                    .and_then(|h| if h > 0 { Some(h as u32) } else { None });

            let txid_bytes = hex_to_txid_bytes(tx_hash_hex)?;

            // Fetch raw transaction to get the TxOut
            let raw_tx = self
                .get_transaction(&txid_bytes)?
                .ok_or_else(|| ChainAdapterError::Parse("tx not found for utxo".into()))?;

            let tx: lwk_wollet::elements::Transaction =
                lwk_wollet::elements::encode::deserialize(&raw_tx)
                    .map_err(|e| ChainAdapterError::Parse(format!("tx deserialize: {e}")))?;

            let txout = tx
                .output
                .get(tx_pos as usize)
                .ok_or_else(|| ChainAdapterError::Parse("vout out of range".into()))?;

            // Extract explicit value and asset (covenant outputs are non-confidential)
            let value = match txout.value {
                lwk_wollet::elements::confidential::Value::Explicit(v) => v,
                _ => 0,
            };
            let asset_id = match txout.asset {
                lwk_wollet::elements::confidential::Asset::Explicit(a) => {
                    a.into_inner().to_byte_array()
                }
                _ => [0u8; 32],
            };

            let raw_txout = lwk_wollet::elements::encode::serialize(txout);

            results.push(ChainUtxo {
                txid: txid_bytes,
                vout: tx_pos,
                value,
                asset_id,
                raw_txout,
                block_height: height,
            });
        }

        Ok(results)
    }

    fn is_spent(&self, txid: &[u8; 32], vout: u32) -> Result<Option<[u8; 32]>, Self::Error> {
        use electrum_client::ElectrumApi;

        // To check if an outpoint is spent, we fetch the transaction, get the
        // scriptPubKey of the output, then list unspent for that scriptPubKey.
        // If our outpoint is NOT in the unspent list, it has been spent.
        // To find the spending txid, we check the script history.
        let raw_tx = match self.get_transaction(txid)? {
            Some(tx) => tx,
            None => return Ok(None),
        };

        let tx: lwk_wollet::elements::Transaction =
            lwk_wollet::elements::encode::deserialize(&raw_tx)
                .map_err(|e| ChainAdapterError::Parse(format!("tx deserialize: {e}")))?;

        let txout = match tx.output.get(vout as usize) {
            Some(o) => o,
            None => return Ok(None),
        };

        let spk = txout.script_pubkey.as_bytes();
        let script_hash_hex = Self::script_hash_hex(spk);

        let client = self.client()?;

        // Check if this specific outpoint is in the unspent list
        let resp = client
            .raw_call(
                "blockchain.scripthash.listunspent",
                [electrum_client::Param::String(script_hash_hex.clone())],
            )
            .map_err(|e| ChainAdapterError::Electrum(e.to_string()))?;

        let txid_display = txid_to_display_hex(txid);

        if let Some(entries) = resp.as_array() {
            for entry in entries {
                if let (Some(hash), Some(pos)) =
                    (entry["tx_hash"].as_str(), entry["tx_pos"].as_u64())
                {
                    if hash == txid_display && pos == vout as u64 {
                        return Ok(None); // still unspent
                    }
                }
            }
        }

        // It's spent. Find the spending transaction via script history.
        let history = client
            .raw_call(
                "blockchain.scripthash.get_history",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| ChainAdapterError::Electrum(e.to_string()))?;

        if let Some(entries) = history.as_array() {
            // Look through history for a transaction that spends our outpoint
            for entry in entries {
                let hist_tx_hash = match entry["tx_hash"].as_str() {
                    Some(h) => h,
                    None => continue,
                };
                if hist_tx_hash == txid_display {
                    continue; // skip the original tx itself
                }

                let hist_txid_bytes = hex_to_txid_bytes(hist_tx_hash)?;
                if let Some(hist_raw) = self.get_transaction(&hist_txid_bytes)? {
                    let hist_tx: lwk_wollet::elements::Transaction =
                        match lwk_wollet::elements::encode::deserialize(&hist_raw) {
                            Ok(t) => t,
                            Err(_) => continue,
                        };

                    for input in &hist_tx.input {
                        let prev_txid = input.previous_output.txid.to_byte_array();
                        if prev_txid == *txid && input.previous_output.vout == vout {
                            return Ok(Some(hist_txid_bytes));
                        }
                    }
                }
            }
        }

        // Spent but couldn't find the spending tx (shouldn't happen normally)
        Ok(Some([0u8; 32]))
    }

    fn get_transaction(&self, txid: &[u8; 32]) -> Result<Option<Vec<u8>>, Self::Error> {
        use electrum_client::ElectrumApi;

        let client = self.client()?;

        let txid_hex = txid_to_display_hex(txid);

        let resp = client
            .raw_call(
                "blockchain.transaction.get",
                [electrum_client::Param::String(txid_hex)],
            )
            .map_err(|e| ChainAdapterError::Electrum(e.to_string()));

        match resp {
            Ok(val) => {
                let hex_str = val
                    .as_str()
                    .ok_or_else(|| ChainAdapterError::Parse("expected string response".into()))?;
                let bytes = hex::decode(hex_str)
                    .map_err(|e| ChainAdapterError::Parse(format!("hex decode: {e}")))?;
                Ok(Some(bytes))
            }
            Err(_) => Ok(None),
        }
    }
}

/// Convert an Electrum-style hex txid (display order) to internal byte order [u8; 32].
fn hex_to_txid_bytes(hex_str: &str) -> Result<[u8; 32], ChainAdapterError> {
    let bytes =
        hex::decode(hex_str).map_err(|e| ChainAdapterError::Parse(format!("bad txid hex: {e}")))?;
    if bytes.len() != 32 {
        return Err(ChainAdapterError::Parse(format!(
            "txid wrong length: {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    // Reverse from display order to internal byte order
    for (i, b) in bytes.iter().rev().enumerate() {
        arr[i] = *b;
    }
    Ok(arr)
}

/// Convert internal byte-order txid to Electrum display-order hex.
fn txid_to_display_hex(txid: &[u8; 32]) -> String {
    txid.iter().rev().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_txid_bytes_reverses_byte_order() {
        // Display order: 0102...1f20 (first byte = 01)
        let display_hex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let bytes = hex_to_txid_bytes(display_hex).unwrap();
        // Internal byte order should be reversed
        assert_eq!(bytes[0], 0x20);
        assert_eq!(bytes[31], 0x01);
    }

    #[test]
    fn txid_to_display_hex_reverses_byte_order() {
        let mut internal = [0u8; 32];
        internal[0] = 0x20;
        internal[31] = 0x01;
        let display = txid_to_display_hex(&internal);
        assert!(display.starts_with("01"));
        assert!(display.ends_with("20"));
    }

    #[test]
    fn hex_to_txid_and_display_roundtrip() {
        let original_display = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let bytes = hex_to_txid_bytes(original_display).unwrap();
        let roundtripped = txid_to_display_hex(&bytes);
        assert_eq!(roundtripped, original_display);
    }

    #[test]
    fn hex_to_txid_bytes_rejects_wrong_length() {
        assert!(hex_to_txid_bytes("abcd").is_err());
        assert!(hex_to_txid_bytes("").is_err());
    }

    #[test]
    fn hex_to_txid_bytes_rejects_invalid_hex() {
        let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert!(hex_to_txid_bytes(bad).is_err());
    }

    #[test]
    fn script_hash_hex_matches_electrum_convention() {
        // Electrum script hash = SHA256(scriptPubKey) with reversed byte order
        let spk = b"test_script_pubkey";
        let hash = ElectrumChainAdapter::script_hash_hex(spk);
        assert_eq!(hash.len(), 64); // 32 bytes hex-encoded

        // Verify reversibility: decode, reverse, re-hash should match original SHA256
        let hash_bytes = hex::decode(&hash).unwrap();
        let mut expected = Sha256::digest(spk).to_vec();
        expected.reverse();
        assert_eq!(hash_bytes, expected);
    }
}
