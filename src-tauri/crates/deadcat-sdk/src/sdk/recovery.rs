use std::collections::{HashMap, HashSet};

use rayon::prelude::*;

use super::*;

#[derive(Debug, Clone, Copy)]
pub struct LimitOrderRecoveryConfig {
    pub order_index_gap_limit: u32,
    pub price_min: u64,
    pub price_max: u64,
    pub max_candidate_outputs: usize,
}

impl Default for LimitOrderRecoveryConfig {
    fn default() -> Self {
        Self {
            order_index_gap_limit: ORDER_INDEX_GAP_LIMIT_DEFAULT,
            price_min: LIMIT_ORDER_PRICE_MIN,
            price_max: LIMIT_ORDER_PRICE_MAX,
            max_candidate_outputs: ORDER_RECOVERY_MAX_CANDIDATE_OUTPUTS_DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveredOwnOrderStatus {
    ActiveConfirmed,
    ActiveMempool,
    SpentOrFilled,
    Ambiguous,
}

#[derive(Debug, Clone)]
pub struct RecoveredOwnOrder {
    pub txid: Txid,
    pub vout: u32,
    pub outpoint: OutPoint,
    pub offered_asset_id: [u8; 32],
    pub offered_amount: u64,
    pub order_index: Option<u32>,
    pub maker_base_pubkey: Option<[u8; 32]>,
    pub order_nonce: Option<[u8; 32]>,
    pub params: Option<MakerOrderParams>,
    pub status: RecoveredOwnOrderStatus,
    pub ambiguity_count: u32,
}

impl RecoveredOwnOrder {
    pub fn is_cancelable(&self) -> bool {
        matches!(
            self.status,
            RecoveredOwnOrderStatus::ActiveConfirmed | RecoveredOwnOrderStatus::ActiveMempool
        ) && self.order_index.is_some()
            && self.maker_base_pubkey.is_some()
            && self.order_nonce.is_some()
            && self.params.is_some()
    }
}

#[derive(Debug, Clone)]
pub(super) struct WalletOrderCandidateOutput {
    outpoint: OutPoint,
    txid: Txid,
    vout: u32,
    pub(super) asset_id: [u8; 32],
    value: u64,
    script_pubkey: Script,
    confirmed: bool,
}

#[derive(Debug, Clone)]
struct RecoveryMatch {
    candidate: WalletOrderCandidateOutput,
    order_index: u32,
    maker_base_pubkey: [u8; 32],
    order_nonce: [u8; 32],
    params: MakerOrderParams,
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

    pub(super) fn extract_wallet_candidate_outputs(
        &self,
        max_candidate_outputs: usize,
    ) -> Result<Vec<WalletOrderCandidateOutput>> {
        let wallet_scripts = self.wallet_script_pubkeys(1_024);
        let mut out = Vec::new();

        for wallet_tx in self.transactions()? {
            if !Self::is_wallet_authored_tx(&wallet_tx) {
                continue;
            }

            let tx = self.fetch_transaction(&wallet_tx.txid)?;
            for (vout, txout) in tx.output.iter().enumerate() {
                if txout.script_pubkey.is_empty() {
                    continue;
                }
                // Maker-order covenant outputs are Taproot outputs. Restricting candidates here
                // avoids scanning unrelated wallet-authored outputs during recovery/index lookup.
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

                out.push(WalletOrderCandidateOutput {
                    outpoint: OutPoint::new(wallet_tx.txid, vout as u32),
                    txid: wallet_tx.txid,
                    vout: vout as u32,
                    asset_id: asset.into_inner().to_byte_array(),
                    value,
                    script_pubkey: txout.script_pubkey.clone(),
                    confirmed: wallet_tx.height.is_some(),
                });

                if out.len() >= max_candidate_outputs {
                    return Ok(out);
                }
            }
        }

        Ok(out)
    }

    fn recover_own_limit_order_matches_with_candidates(
        &self,
        candidate_base_assets: &[[u8; 32]],
        candidates: &[WalletOrderCandidateOutput],
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

        let policy_asset = self.policy_asset().into_inner().to_byte_array();
        let allowed_base_assets: HashSet<[u8; 32]> = candidate_base_assets
            .iter()
            .copied()
            .filter(|asset| *asset != policy_asset)
            .collect();
        if allowed_base_assets.is_empty() {
            return Ok(Vec::new());
        }

        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let mut scripts_to_candidates: HashMap<Vec<u8>, Vec<WalletOrderCandidateOutput>> =
            HashMap::new();
        let mut sell_base_assets = HashSet::<[u8; 32]>::new();
        let all_base_assets = allowed_base_assets;

        for candidate in candidates {
            scripts_to_candidates
                .entry(candidate.script_pubkey.to_bytes())
                .or_default()
                .push(candidate.clone());
            if candidate.asset_id != policy_asset && all_base_assets.contains(&candidate.asset_id) {
                sell_base_assets.insert(candidate.asset_id);
            }
        }

        let sell_base_assets: Vec<[u8; 32]> = sell_base_assets.into_iter().collect();
        let all_base_assets: Vec<[u8; 32]> = all_base_assets.into_iter().collect();
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
            base_asset: [u8; 32],
            price: u64,
            direction_tag: u8,
        }

        // Note: this currently materializes a full cartesian task set
        // (index x base_asset x price x direction) before parallel execution.
        // Bounded v2 constants keep it tractable for now; a streamed/chunked
        // iterator is the intended follow-up optimization.
        let mut tasks = Vec::new();
        for (order_index, maker_base_pubkey) in maker_keys {
            for base_asset in &sell_base_assets {
                for price in config.price_min..=config.price_max {
                    tasks.push(ScanTask {
                        order_index,
                        maker_base_pubkey,
                        base_asset: *base_asset,
                        price,
                        direction_tag: Self::direction_tag(OrderDirection::SellBase),
                    });
                }
            }

            for base_asset in &all_base_assets {
                for price in config.price_min..=config.price_max {
                    tasks.push(ScanTask {
                        order_index,
                        maker_base_pubkey,
                        base_asset: *base_asset,
                        price,
                        direction_tag: Self::direction_tag(OrderDirection::SellQuote),
                    });
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
                let order_nonce = Self::derive_order_nonce_v2(
                    &order_nonce_seed,
                    task.order_index,
                    &task.base_asset,
                    &policy_asset,
                    task.price,
                    direction,
                );

                let (params, _p_order) = MakerOrderParams::new(
                    task.base_asset,
                    policy_asset,
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

                let Some(candidates_for_script) = scripts_to_candidates.get(&script_bytes) else {
                    return Ok(Vec::new());
                };

                let mut local_matches = Vec::new();
                for candidate in candidates_for_script {
                    let asset_match = match direction {
                        OrderDirection::SellBase => candidate.asset_id == task.base_asset,
                        OrderDirection::SellQuote => candidate.asset_id == policy_asset,
                    };
                    if !asset_match {
                        continue;
                    }
                    local_matches.push(RecoveryMatch {
                        candidate: candidate.clone(),
                        order_index: task.order_index,
                        maker_base_pubkey: task.maker_base_pubkey,
                        order_nonce,
                        params,
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

    fn recover_own_limit_order_matches(
        &self,
        candidate_base_assets: &[[u8; 32]],
        config: LimitOrderRecoveryConfig,
    ) -> Result<Vec<RecoveryMatch>> {
        let candidates = self.extract_wallet_candidate_outputs(config.max_candidate_outputs)?;
        self.recover_own_limit_order_matches_with_candidates(
            candidate_base_assets,
            &candidates,
            config,
        )
    }

    /// Recover own limit orders by deterministic key+parameter scan (v2).
    pub fn recover_own_limit_orders(
        &self,
        candidate_base_assets: &[[u8; 32]],
        config: LimitOrderRecoveryConfig,
    ) -> Result<Vec<RecoveredOwnOrder>> {
        if config.order_index_gap_limit == 0 {
            return Ok(Vec::new());
        }

        // Use expanding windows to avoid paying the full configured index range for
        // low-index wallets while still discovering contiguous higher indices.
        let mut gap_limit = 1u32.min(config.order_index_gap_limit);
        let matches = loop {
            let matches = self.recover_own_limit_order_matches(
                candidate_base_assets,
                LimitOrderRecoveryConfig {
                    order_index_gap_limit: gap_limit,
                    ..config
                },
            )?;

            let max_index = matches.iter().map(|m| m.order_index).max();
            match max_index {
                Some(index)
                    if index.saturating_add(1) == gap_limit
                        && gap_limit < config.order_index_gap_limit =>
                {
                    gap_limit = gap_limit
                        .saturating_mul(2)
                        .min(config.order_index_gap_limit);
                    continue;
                }
                _ => break matches,
            }
        };
        if matches.is_empty() {
            return Ok(Vec::new());
        }

        let mut grouped: HashMap<(Txid, u32), Vec<RecoveryMatch>> = HashMap::new();
        for m in matches {
            grouped
                .entry((m.candidate.txid, m.candidate.vout))
                .or_default()
                .push(m);
        }

        let mut recovered = Vec::new();
        for ((_txid, _vout), group) in grouped {
            if group.len() == 1 {
                let m = &group[0];
                let contract = CompiledMakerOrder::new(m.params)?;
                let covenant_spk = contract.script_pubkey(&m.maker_base_pubkey);
                let active = self
                    .scan_covenant_utxos(&covenant_spk)?
                    .iter()
                    .any(|(op, _)| *op == m.candidate.outpoint);

                let status = if active {
                    if m.candidate.confirmed {
                        RecoveredOwnOrderStatus::ActiveConfirmed
                    } else {
                        RecoveredOwnOrderStatus::ActiveMempool
                    }
                } else {
                    RecoveredOwnOrderStatus::SpentOrFilled
                };

                recovered.push(RecoveredOwnOrder {
                    txid: m.candidate.txid,
                    vout: m.candidate.vout,
                    outpoint: m.candidate.outpoint,
                    offered_asset_id: m.candidate.asset_id,
                    offered_amount: m.candidate.value,
                    order_index: Some(m.order_index),
                    maker_base_pubkey: Some(m.maker_base_pubkey),
                    order_nonce: Some(m.order_nonce),
                    params: Some(m.params),
                    status,
                    ambiguity_count: 1,
                });
            } else {
                let first = &group[0];
                recovered.push(RecoveredOwnOrder {
                    txid: first.candidate.txid,
                    vout: first.candidate.vout,
                    outpoint: first.candidate.outpoint,
                    offered_asset_id: first.candidate.asset_id,
                    offered_amount: first.candidate.value,
                    order_index: None,
                    maker_base_pubkey: None,
                    order_nonce: None,
                    params: None,
                    status: RecoveredOwnOrderStatus::Ambiguous,
                    ambiguity_count: group.len() as u32,
                });
            }
        }

        // Deterministic, allocation-free ordering by txid bytes then vout.
        recovered.sort_by(|a, b| {
            b.txid
                .to_byte_array()
                .cmp(&a.txid.to_byte_array())
                .then_with(|| b.vout.cmp(&a.vout))
        });

        Ok(recovered)
    }

    pub(super) fn next_maker_order_index_with_candidates(
        &self,
        candidate_base_assets: &[[u8; 32]],
        candidate_outputs: &[WalletOrderCandidateOutput],
    ) -> Result<u32> {
        // Use progressive scan windows so wallets with low order counts don't always pay the
        // full 0..ORDER_INDEX_GAP_LIMIT_DEFAULT search cost.
        let mut gap_limit = 1u32.min(ORDER_INDEX_GAP_LIMIT_DEFAULT);
        loop {
            let matches = self.recover_own_limit_order_matches_with_candidates(
                candidate_base_assets,
                candidate_outputs,
                LimitOrderRecoveryConfig {
                    order_index_gap_limit: gap_limit,
                    ..LimitOrderRecoveryConfig::default()
                },
            )?;

            let max_index = matches.iter().map(|m| m.order_index).max();
            match max_index {
                Some(index)
                    if index.saturating_add(1) == gap_limit
                        && gap_limit < ORDER_INDEX_GAP_LIMIT_DEFAULT =>
                {
                    gap_limit = gap_limit
                        .saturating_mul(2)
                        .min(ORDER_INDEX_GAP_LIMIT_DEFAULT);
                    continue;
                }
                Some(index) => {
                    return index.checked_add(1).ok_or_else(|| {
                        Error::MakerOrder("maker order index overflow".to_string())
                    });
                }
                None => return Ok(0),
            }
        }
    }

    pub fn resolve_order_index(
        &self,
        params: &MakerOrderParams,
        maker_base_pubkey: [u8; 32],
        offered_amount: u64,
        outpoint_hint: Option<OutPoint>,
    ) -> Result<Option<u32>> {
        if params.min_fill_lots != LIMIT_ORDER_MIN_FILL_LOTS_V2
            || params.min_remainder_lots != LIMIT_ORDER_MIN_REMAINDER_LOTS_V2
        {
            return Ok(None);
        }

        let order_nonce_seed = self.derive_order_nonce_seed()?;
        let mut matched = Vec::new();

        for order_index in 0..ORDER_INDEX_GAP_LIMIT_DEFAULT {
            let maker_keypair = self.derive_maker_keypair(order_index)?;
            let (maker_xonly, _parity) = maker_keypair.x_only_public_key();
            if maker_xonly.serialize() != maker_base_pubkey {
                continue;
            }

            let expected_nonce = Self::derive_order_nonce_v2(
                &order_nonce_seed,
                order_index,
                &params.base_asset_id,
                &params.quote_asset_id,
                params.price,
                params.direction,
            );
            let (reconstructed, _p_order) = MakerOrderParams::new(
                params.base_asset_id,
                params.quote_asset_id,
                params.price,
                LIMIT_ORDER_MIN_FILL_LOTS_V2,
                LIMIT_ORDER_MIN_REMAINDER_LOTS_V2,
                params.direction,
                NUMS_KEY_BYTES,
                &maker_base_pubkey,
                &expected_nonce,
            );
            if reconstructed != *params {
                continue;
            }

            if let Some(outpoint) = outpoint_hint {
                let contract = CompiledMakerOrder::new(reconstructed)?;
                let covenant_spk = contract.script_pubkey(&maker_base_pubkey);
                let utxos = self.scan_covenant_utxos(&covenant_spk)?;
                let matches_outpoint = utxos.into_iter().any(|(op, txout)| {
                    op == outpoint && txout.value.explicit().unwrap_or(0) == offered_amount
                });
                if !matches_outpoint {
                    continue;
                }
            }

            matched.push(order_index);
        }

        match matched.as_slice() {
            [] => Ok(None),
            [index] => Ok(Some(*index)),
            _ => Err(Error::MakerOrder(
                "ambiguous maker order index resolution".to_string(),
            )),
        }
    }
}
