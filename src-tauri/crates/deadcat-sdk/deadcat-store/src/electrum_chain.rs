use deadcat_sdk::elements::hashes::Hash as _;
use deadcat_sdk::lwk_wollet;
use electrum_client::ElectrumApi;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::sync::{ChainSource, ChainUtxo};

#[derive(Debug, Error)]
pub enum ElectrumChainError {
    #[error("electrum error: {0}")]
    Electrum(String),

    #[error("parse error: {0}")]
    Parse(String),
}

pub struct ElectrumChainAdapter {
    electrum_url: String,
}

impl ElectrumChainAdapter {
    pub fn new(electrum_url: &str) -> Self {
        Self {
            electrum_url: electrum_url.to_string(),
        }
    }

    fn client(&self) -> Result<electrum_client::Client, ElectrumChainError> {
        electrum_client::Client::new(&self.electrum_url)
            .map_err(|e| ElectrumChainError::Electrum(e.to_string()))
    }

    fn script_hash_hex(script_pubkey: &[u8]) -> String {
        let mut hash = Sha256::digest(script_pubkey).to_vec();
        hash.reverse();
        hex::encode(&hash)
    }
}

impl ChainSource for ElectrumChainAdapter {
    type Error = ElectrumChainError;

    fn best_block_height(&self) -> Result<u32, Self::Error> {
        let client = self.client()?;
        let resp = client
            .raw_call("blockchain.headers.subscribe", [])
            .map_err(|e| ElectrumChainError::Electrum(e.to_string()))?;
        let height = resp["height"].as_u64().ok_or_else(|| {
            ElectrumChainError::Parse("missing height in headers response".into())
        })?;
        Ok(height as u32)
    }

    fn list_unspent(&self, script_pubkey: &[u8]) -> Result<Vec<ChainUtxo>, Self::Error> {
        let client = self.client()?;
        let script_hash_hex = Self::script_hash_hex(script_pubkey);
        let resp = client
            .raw_call(
                "blockchain.scripthash.listunspent",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| ElectrumChainError::Electrum(e.to_string()))?;

        let entries = resp
            .as_array()
            .ok_or_else(|| ElectrumChainError::Parse("expected array response".into()))?;

        let mut results = Vec::new();
        for entry in entries {
            let tx_hash_hex = entry["tx_hash"]
                .as_str()
                .ok_or_else(|| ElectrumChainError::Parse("missing tx_hash".into()))?;
            let tx_pos = entry["tx_pos"]
                .as_u64()
                .ok_or_else(|| ElectrumChainError::Parse("missing tx_pos".into()))?
                as u32;
            let height = entry["height"]
                .as_u64()
                .and_then(|h| if h > 0 { Some(h as u32) } else { None });

            let txid_bytes = hex_to_txid_bytes(tx_hash_hex)?;
            let raw_tx = self
                .get_transaction(&txid_bytes)?
                .ok_or_else(|| ElectrumChainError::Parse("tx not found for utxo".into()))?;

            let tx: lwk_wollet::elements::Transaction =
                lwk_wollet::elements::encode::deserialize(&raw_tx)
                    .map_err(|e| ElectrumChainError::Parse(format!("tx deserialize: {e}")))?;
            let txout = tx
                .output
                .get(tx_pos as usize)
                .ok_or_else(|| ElectrumChainError::Parse("vout out of range".into()))?;

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
        let raw_tx = match self.get_transaction(txid)? {
            Some(tx) => tx,
            None => return Ok(None),
        };
        let tx: lwk_wollet::elements::Transaction =
            lwk_wollet::elements::encode::deserialize(&raw_tx)
                .map_err(|e| ElectrumChainError::Parse(format!("tx deserialize: {e}")))?;
        let txout = match tx.output.get(vout as usize) {
            Some(output) => output,
            None => return Ok(None),
        };

        let script_hash_hex = Self::script_hash_hex(txout.script_pubkey.as_bytes());
        let client = self.client()?;
        let resp = client
            .raw_call(
                "blockchain.scripthash.listunspent",
                [electrum_client::Param::String(script_hash_hex.clone())],
            )
            .map_err(|e| ElectrumChainError::Electrum(e.to_string()))?;

        let txid_display = txid_to_display_hex(txid);
        if let Some(entries) = resp.as_array() {
            for entry in entries {
                if let (Some(hash), Some(pos)) =
                    (entry["tx_hash"].as_str(), entry["tx_pos"].as_u64())
                    && hash == txid_display
                    && pos == vout as u64
                {
                    return Ok(None);
                }
            }
        }

        let history = client
            .raw_call(
                "blockchain.scripthash.get_history",
                [electrum_client::Param::String(script_hash_hex)],
            )
            .map_err(|e| ElectrumChainError::Electrum(e.to_string()))?;

        if let Some(entries) = history.as_array() {
            for entry in entries {
                let Some(history_tx_hash) = entry["tx_hash"].as_str() else {
                    continue;
                };
                if history_tx_hash == txid_display {
                    continue;
                }

                let history_txid = hex_to_txid_bytes(history_tx_hash)?;
                if let Some(history_raw) = self.get_transaction(&history_txid)? {
                    let history_tx: lwk_wollet::elements::Transaction =
                        match lwk_wollet::elements::encode::deserialize(&history_raw) {
                            Ok(tx) => tx,
                            Err(_) => continue,
                        };
                    for input in &history_tx.input {
                        if input.previous_output.txid.to_byte_array() == *txid
                            && input.previous_output.vout == vout
                        {
                            return Ok(Some(history_txid));
                        }
                    }
                }
            }
        }

        Ok(Some([0u8; 32]))
    }

    fn get_transaction(&self, txid: &[u8; 32]) -> Result<Option<Vec<u8>>, Self::Error> {
        let client = self.client()?;
        let txid_hex = txid_to_display_hex(txid);
        let response = client.raw_call(
            "blockchain.transaction.get",
            [electrum_client::Param::String(txid_hex)],
        );

        let resp = match response {
            Ok(resp) => resp,
            Err(err) => {
                let electrum_client::Error::Protocol(payload) = &err else {
                    return Err(ElectrumChainError::Electrum(err.to_string()));
                };
                let message = payload
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let code = payload.get("code").and_then(|value| value.as_i64());
                if message.contains("no such mempool or blockchain transaction")
                    || message.contains("transaction not found")
                    || (matches!(code, Some(-5)) && message.contains("transaction"))
                {
                    return Ok(None);
                }
                return Err(ElectrumChainError::Electrum(err.to_string()));
            }
        };

        let hex_str = resp
            .as_str()
            .ok_or_else(|| ElectrumChainError::Parse("expected string response".into()))?;
        let bytes = hex::decode(hex_str)
            .map_err(|e| ElectrumChainError::Parse(format!("hex decode: {e}")))?;
        Ok(Some(bytes))
    }
}

fn hex_to_txid_bytes(hex_str: &str) -> Result<[u8; 32], ElectrumChainError> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| ElectrumChainError::Parse(format!("bad txid hex: {e}")))?;
    if bytes.len() != 32 {
        return Err(ElectrumChainError::Parse(format!(
            "txid wrong length: {}",
            bytes.len()
        )));
    }

    let mut arr = [0u8; 32];
    for (index, byte) in bytes.iter().rev().enumerate() {
        arr[index] = *byte;
    }
    Ok(arr)
}

fn txid_to_display_hex(txid: &[u8; 32]) -> String {
    txid.iter()
        .rev()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
