use std::collections::{HashMap, HashSet};

use rayon::prelude::*;

use super::*;

#[derive(Debug, Clone, Copy)]
pub struct LimitOrderRecoveryConfig {
    pub order_index_gap_limit: u32,
    pub price_min: u64,
    pub price_max: u64,
}

impl Default for LimitOrderRecoveryConfig {
    fn default() -> Self {
        Self {
            order_index_gap_limit: ORDER_INDEX_GAP_LIMIT_DEFAULT,
            price_min: LIMIT_ORDER_PRICE_MIN,
            price_max: LIMIT_ORDER_PRICE_MAX,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct WalletAuthoredTaprootOutput {
    pub(super) asset_id: [u8; 32],
    pub(super) script_pubkey: Script,
}

#[derive(Debug, Clone)]
struct RecoveryMatch {
    order_index: u32,
    maker_base_pubkey: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RecoveryMarketPair {
    base_asset_id: [u8; 32],
    quote_asset_id: [u8; 32],
}

fn canonical_recovery_markets(
    candidate_markets: &[PredictionMarketParams],
) -> Vec<RecoveryMarketPair> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for market in candidate_markets {
        for pair in [
            RecoveryMarketPair {
                base_asset_id: market.yes_token_asset,
                quote_asset_id: market.collateral_asset_id,
            },
            RecoveryMarketPair {
                base_asset_id: market.no_token_asset,
                quote_asset_id: market.collateral_asset_id,
            },
        ] {
            if seen.insert(pair) {
                out.push(pair);
            }
        }
    }

    out
}

impl DeadcatSdk {
    fn is_taproot_script_pubkey(script_pubkey: &Script) -> bool {
        let bytes = script_pubkey.to_bytes();
        bytes.len() == 34 && bytes[0] == 0x51 && bytes[1] == 0x20
    }

    fn is_wallet_authored_tx(wallet_tx: &WalletTx) -> bool {
        if wallet_tx.fee > 0 {
            return true;
        }
        if wallet_tx.balance.values().any(|delta| *delta < 0) {
            return true;
        }
        wallet_tx.type_.to_lowercase() != "incoming"
    }

    fn wallet_script_pubkeys(&self, max_address_index: u32) -> HashSet<Vec<u8>> {
        let mut wallet_scripts = HashSet::new();
        for i in 0..max_address_index {
            if let Ok(addr) = self.wollet.address(Some(i)) {
                wallet_scripts.insert(addr.address().script_pubkey().to_bytes());
            }
        }
        wallet_scripts
    }

    pub(super) fn extract_wallet_authored_taproot_outputs(
        &self,
        max_outputs: usize,
    ) -> Result<Vec<WalletAuthoredTaprootOutput>> {
        let wallet_scripts = self.wallet_script_pubkeys(1_024);
        let mut out = Vec::new();

        for wallet_tx in self.transactions()? {
            if !Self::is_wallet_authored_tx(&wallet_tx) {
                continue;
            }

            let tx = self.fetch_transaction(&wallet_tx.txid)?;
            for txout in &tx.output {
                if txout.script_pubkey.is_empty() {
                    continue;
                }
                // Recovery starts from wallet-authored explicit Taproot outputs and
                // only later upgrades positively-matched maker-order scripts into
                // maker-index evidence.
                if !Self::is_taproot_script_pubkey(&txout.script_pubkey) {
                    continue;
                }
                if wallet_scripts.contains(&txout.script_pubkey.to_bytes()) {
                    continue;
                }
                let Some(asset) = txout.asset.explicit() else {
                    continue;
                };
                let Some(value) = txout.value.explicit() else {
                    continue;
                };
                let _ = value;

                out.push(WalletAuthoredTaprootOutput {
                    asset_id: asset.into_inner().to_byte_array(),
                    script_pubkey: txout.script_pubkey.clone(),
                });

                if out.len() >= max_outputs {
                    return Ok(out);
                }
            }
        }

        Ok(out)
    }

    fn recover_own_limit_order_matches_with_candidates(
        &self,
        candidate_markets: &[PredictionMarketParams],
        raw_taproot_outputs: &[WalletAuthoredTaprootOutput],
        config: LimitOrderRecoveryConfig,
    ) -> Result<Vec<RecoveryMatch>> {
        if config.order_index_gap_limit == 0 {
            return Ok(Vec::new());
        }
        if config.price_min == 0 || config.price_min > config.price_max {
            return Err(Error::MakerOrder(
                "invalid recovery config: bad price range".to_string(),
            ));
        }
        if raw_taproot_outputs.is_empty() {
            return Ok(Vec::new());
        }

        let recovery_markets = canonical_recovery_markets(candidate_markets);
        if recovery_markets.is_empty() {
            return Err(Error::MakerOrder(
                "wallet contains maker-order candidate outputs but no market catalog is available for recovery"
                    .to_string(),
            ));
        }

        let mut scripts_to_outputs: HashMap<Vec<u8>, Vec<WalletAuthoredTaprootOutput>> =
            HashMap::new();
        let output_assets: HashSet<[u8; 32]> = raw_taproot_outputs
            .iter()
            .map(|output| output.asset_id)
            .collect();

        for output in raw_taproot_outputs {
            scripts_to_outputs
                .entry(output.script_pubkey.to_bytes())
                .or_default()
                .push(output.clone());
        }

        let order_nonce_seed = self.derive_order_nonce_seed()?;
        let mut maker_keys = Vec::with_capacity(config.order_index_gap_limit as usize);
        for order_index in 0..config.order_index_gap_limit {
            let maker_keypair = self.derive_maker_keypair(order_index)?;
            let (maker_xonly, _parity) = maker_keypair.x_only_public_key();
            maker_keys.push((order_index, maker_xonly.serialize()));
        }

        #[derive(Clone, Copy)]
        struct ScanTask {
            order_index: u32,
            maker_base_pubkey: [u8; 32],
            base_asset_id: [u8; 32],
            quote_asset_id: [u8; 32],
            price: u64,
            direction_tag: u8,
        }

        let mut tasks = Vec::new();
        for (order_index, maker_base_pubkey) in maker_keys {
            for market in &recovery_markets {
                if output_assets.contains(&market.base_asset_id) {
                    for price in config.price_min..=config.price_max {
                        tasks.push(ScanTask {
                            order_index,
                            maker_base_pubkey,
                            base_asset_id: market.base_asset_id,
                            quote_asset_id: market.quote_asset_id,
                            price,
                            direction_tag: Self::direction_tag(OrderDirection::SellBase),
                        });
                    }
                }

                if output_assets.contains(&market.quote_asset_id) {
                    for price in config.price_min..=config.price_max {
                        tasks.push(ScanTask {
                            order_index,
                            maker_base_pubkey,
                            base_asset_id: market.base_asset_id,
                            quote_asset_id: market.quote_asset_id,
                            price,
                            direction_tag: Self::direction_tag(OrderDirection::SellQuote),
                        });
                    }
                }
            }
        }

        let matches = tasks
            .into_par_iter()
            .map(|task| -> Result<Vec<RecoveryMatch>> {
                let direction =
                    if task.direction_tag == Self::direction_tag(OrderDirection::SellBase) {
                        OrderDirection::SellBase
                    } else {
                        OrderDirection::SellQuote
                    };
                let nonce_identity = OrderNonceIdentity {
                    base_asset_id: task.base_asset_id,
                    quote_asset_id: task.quote_asset_id,
                    price: task.price,
                    direction,
                    min_fill_lots: LIMIT_ORDER_MIN_FILL_LOTS_V2,
                    min_remainder_lots: LIMIT_ORDER_MIN_REMAINDER_LOTS_V2,
                };
                let order_nonce = Self::derive_order_nonce_v2(
                    &order_nonce_seed,
                    task.order_index,
                    &nonce_identity,
                );

                let (params, _p_order) = MakerOrderParams::new(
                    task.base_asset_id,
                    task.quote_asset_id,
                    task.price,
                    LIMIT_ORDER_MIN_FILL_LOTS_V2,
                    LIMIT_ORDER_MIN_REMAINDER_LOTS_V2,
                    direction,
                    NUMS_KEY_BYTES,
                    &task.maker_base_pubkey,
                    &order_nonce,
                );
                let contract = CompiledMakerOrder::new(params)?;
                let script_bytes = contract.script_pubkey(&task.maker_base_pubkey).to_bytes();

                let Some(outputs_for_script) = scripts_to_outputs.get(&script_bytes) else {
                    return Ok(Vec::new());
                };

                let expected_asset_id = match direction {
                    OrderDirection::SellBase => task.base_asset_id,
                    OrderDirection::SellQuote => task.quote_asset_id,
                };

                let mut local_matches = Vec::new();
                for output in outputs_for_script {
                    if output.asset_id != expected_asset_id {
                        continue;
                    }
                    local_matches.push(RecoveryMatch {
                        order_index: task.order_index,
                        maker_base_pubkey: task.maker_base_pubkey,
                    });
                }

                Ok(local_matches)
            })
            .collect::<Result<Vec<Vec<RecoveryMatch>>>>()?
            .into_iter()
            .flatten()
            .collect();

        Ok(matches)
    }

    pub(crate) fn next_maker_order_index_from_markets(
        &self,
        candidate_markets: &[PredictionMarketParams],
    ) -> Result<u32> {
        let raw_taproot_outputs = self.extract_wallet_authored_taproot_outputs(
            ORDER_RECOVERY_MAX_CANDIDATE_OUTPUTS_DEFAULT,
        )?;
        if raw_taproot_outputs.is_empty() {
            return Ok(0);
        }
        if candidate_markets.is_empty() {
            return Err(Error::MakerOrder(
                "wallet contains wallet-authored Taproot outputs but no market catalog is available for recovery"
                    .to_string(),
            ));
        }

        self.next_maker_order_index_with_candidates(candidate_markets, &raw_taproot_outputs)
    }

    pub(super) fn next_maker_order_index_with_candidates(
        &self,
        candidate_markets: &[PredictionMarketParams],
        raw_taproot_outputs: &[WalletAuthoredTaprootOutput],
    ) -> Result<u32> {
        if raw_taproot_outputs.is_empty() {
            return Ok(0);
        }

        let max_gap_limit = u32::try_from(raw_taproot_outputs.len().saturating_add(1))
            .unwrap_or(ORDER_INDEX_GAP_LIMIT_DEFAULT)
            .min(ORDER_INDEX_GAP_LIMIT_DEFAULT);
        let mut gap_limit = 1u32.min(max_gap_limit);

        let matches = loop {
            let matches = self.recover_own_limit_order_matches_with_candidates(
                candidate_markets,
                raw_taproot_outputs,
                LimitOrderRecoveryConfig {
                    order_index_gap_limit: gap_limit,
                    ..LimitOrderRecoveryConfig::default()
                },
            )?;

            let max_index = matches.iter().map(|m| m.order_index).max();
            match max_index {
                Some(index)
                    if index.saturating_add(1) == gap_limit && gap_limit < max_gap_limit =>
                {
                    gap_limit = gap_limit.saturating_mul(2).min(max_gap_limit);
                    continue;
                }
                _ => break matches,
            }
        };

        if matches.is_empty() {
            return Ok(0);
        }

        let used_maker_pubkeys = matches
            .iter()
            .map(|m| m.maker_base_pubkey)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        self.next_maker_order_index(&used_maker_pubkeys)
    }

    pub(super) fn wallet_contains_limit_order_identity(
        &self,
        maker_base_pubkey: &[u8; 32],
        order_params: &MakerOrderParams,
        offered_asset_id: &[u8; 32],
    ) -> Result<bool> {
        let raw_taproot_outputs = self.extract_wallet_authored_taproot_outputs(
            ORDER_RECOVERY_MAX_CANDIDATE_OUTPUTS_DEFAULT,
        )?;
        if raw_taproot_outputs.is_empty() {
            return Ok(false);
        }

        let contract = CompiledMakerOrder::new(*order_params)?;
        let target_script = contract.script_pubkey(maker_base_pubkey).to_bytes();

        Ok(raw_taproot_outputs.iter().any(|output| {
            output.asset_id == *offered_asset_id && output.script_pubkey.to_bytes() == target_script
        }))
    }

    pub fn resolve_order_index(
        &self,
        params: &MakerOrderParams,
        maker_base_pubkey: [u8; 32],
    ) -> Result<Option<u32>> {
        let order_nonce_seed = self.derive_order_nonce_seed()?;

        for order_index in 0..ORDER_INDEX_RESOLUTION_LIMIT_DEFAULT {
            let maker_keypair = self.derive_maker_keypair(order_index)?;
            let (maker_xonly, _parity) = maker_keypair.x_only_public_key();
            if maker_xonly.serialize() != maker_base_pubkey {
                continue;
            }

            let expected_nonce = Self::derive_order_nonce_v2(
                &order_nonce_seed,
                order_index,
                &OrderNonceIdentity::from_params(params),
            );
            let (reconstructed, _p_order) = MakerOrderParams::new(
                params.base_asset_id,
                params.quote_asset_id,
                params.price,
                params.min_fill_lots,
                params.min_remainder_lots,
                params.direction,
                NUMS_KEY_BYTES,
                &maker_base_pubkey,
                &expected_nonce,
            );
            if reconstructed == *params {
                return Ok(Some(order_index));
            }
        }

        Ok(None)
    }
}
