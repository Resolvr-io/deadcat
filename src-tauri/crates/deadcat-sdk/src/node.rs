//! `DeadcatNode` — unified SDK coordinator.
//!
//! Owns the wallet (SDK), Nostr discovery service, and shared store behind a
//! single `&self` API. Combined methods (on-chain + Nostr) use
//! `tokio::task::spawn_blocking` internally so callers stay in async land,
//! including store-backed wallet sync.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lwk_wollet::elements::{AssetId, Transaction, Txid};
use lwk_wollet::{AddressResult, WalletTx, WalletTxOut};
use nostr_sdk::prelude::*;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use crate::announcement::{CONTRACT_ANNOUNCEMENT_VERSION, ContractAnnouncement, ContractMetadata};
use crate::discovery::config::DiscoveryConfig;
use crate::discovery::events::DiscoveryEvent;
use crate::discovery::market::{DiscoveredMarket, ParsedDiscoveredMarketAnnouncement};
use crate::discovery::pool::{parse_canonical_lmsr_outpoint, parse_pool_event};
use crate::discovery::service::{
    DiscoveryService, NoopStore, discovered_market_to_contract_params,
    persist_canonical_lmsr_state_to_store, persist_market_to_store,
};
use crate::discovery::store_trait::{
    ContractMetadataInput, DiscoveryStore, LmsrPoolIngestInput, LmsrPoolStateSource, NodeStore,
    OwnMakerOrderRecordInput, PendingOrderDeletion, PredictionMarketCandidateIngestInput,
};
use crate::discovery::{
    AttestationContent, AttestationResult, DEFAULT_RELAYS, DiscoveredOrder, OrderAnnouncement,
    bytes_to_hex,
};
use crate::error::{Error, NodeError};
use crate::lmsr_pool::api::{
    CreateLmsrPoolRequest, CreateLmsrPoolResult, LmsrPoolLocator, LmsrPoolSnapshot,
    build_pool_announcement_from_snapshot, txid_to_canonical_bytes,
};
use crate::lmsr_pool::identity::{derive_lmsr_market_id, derive_lmsr_pool_id};
use crate::lmsr_pool::math::fee_free_yes_spot_price_bps;
use crate::lmsr_pool::params::LmsrPoolId;
use crate::lmsr_pool::table::LmsrTableManifest;
use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::network::Network;
use crate::prediction_market::anchor::PredictionMarketAnchor;
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::params::{MarketId, PredictionMarketParams};
use crate::prediction_market::state::MarketState;
use crate::sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, DeadcatSdk, FillOrderResult,
    IssuanceResult, RedemptionResult, ResolutionResult,
};
use crate::trade::types::{TradeAmount, TradeDirection, TradeQuote, TradeResult, TradeSide};
use crate::{LmsrPoolSyncRepairInput, LmsrPriceHistoryEntry, LmsrPriceTransitionInput};

const ORDER_INDEX_AUTO_RESOLVE_SENTINEL: u32 = u32::MAX;

// ── Wallet snapshot ────────────────────────────────────────────────────────

/// Cached wallet state captured at the end of every SDK
/// call. Read queries (`balance`, `utxos`, `transactions`) are served directly
/// from this snapshot via a `tokio::sync::watch` channel — no mutex, no
/// `spawn_blocking`.
#[derive(Clone, Debug)]
pub struct WalletSnapshot {
    pub balance: HashMap<AssetId, u64>,
    pub utxos: Vec<WalletTxOut>,
    pub transactions: Vec<WalletTx>,
}

// ── Struct ──────────────────────────────────────────────────────────────────

/// Unified coordinator that owns the SDK wallet, Nostr discovery service,
/// and (optionally) a shared persistence store.
///
/// All public methods take `&self`; interior mutability is provided by
/// `Arc<Mutex<…>>`. SDK (blocking) calls are dispatched via
/// `tokio::task::spawn_blocking`.
pub struct DeadcatNode<S: DiscoveryStore = NoopStore> {
    sdk: Arc<Mutex<Option<DeadcatSdk>>>,
    snapshot_tx: watch::Sender<Option<WalletSnapshot>>,
    snapshot_rx: watch::Receiver<Option<WalletSnapshot>>,
    discovery: DiscoveryService<S>,
    keys: Keys,
    network: Network,
    store: Option<Arc<Mutex<S>>>,
}

// ── Construction ────────────────────────────────────────────────────────────

impl DeadcatNode<NoopStore> {
    /// Create a node without store persistence.
    pub fn new(
        keys: Keys,
        network: Network,
        mut config: DiscoveryConfig,
    ) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        config.network_tag = network.discovery_tag().to_string();
        let (discovery, rx) = DiscoveryService::new(keys.clone(), config);
        let (snapshot_tx, snapshot_rx) = watch::channel(None);
        (
            Self {
                sdk: Arc::new(Mutex::new(None)),
                snapshot_tx,
                snapshot_rx,
                discovery,
                keys,
                network,
                store: None,
            },
            rx,
        )
    }
}

impl<S: DiscoveryStore> DeadcatNode<S> {
    /// Create a node with store persistence.
    pub fn with_store(
        keys: Keys,
        network: Network,
        store: Arc<Mutex<S>>,
        mut config: DiscoveryConfig,
    ) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        config.network_tag = network.discovery_tag().to_string();
        let (discovery, rx) = DiscoveryService::with_store(keys.clone(), store.clone(), config);
        let (snapshot_tx, snapshot_rx) = watch::channel(None);
        (
            Self {
                sdk: Arc::new(Mutex::new(None)),
                snapshot_tx,
                snapshot_rx,
                discovery,
                keys,
                network,
                store: Some(store),
            },
            rx,
        )
    }

    // ── Wallet lifecycle ────────────────────────────────────────────────

    /// Unlock the wallet by initializing the SDK with the given mnemonic.
    pub fn unlock_wallet(
        &self,
        mnemonic: &str,
        electrum_url: &str,
        datadir: &Path,
    ) -> Result<(), NodeError> {
        let mut guard = self.sdk.lock().map_err(|_| NodeError::MutexPoisoned)?;
        if guard.is_some() {
            return Err(NodeError::WalletAlreadyUnlocked);
        }
        let sdk = DeadcatSdk::new(mnemonic, self.network, electrum_url, datadir)
            .map_err(NodeError::Sdk)?;
        // Seed the snapshot so balance/utxos/transactions are available
        // immediately, without waiting for the first with_sdk call.
        let snapshot = WalletSnapshot {
            balance: sdk.balance().unwrap_or_default(),
            utxos: sdk.utxos().unwrap_or_default(),
            transactions: sdk.transactions().unwrap_or_default(),
        };
        let _ = self.snapshot_tx.send(Some(snapshot));
        *guard = Some(sdk);
        Ok(())
    }

    /// Lock the wallet, dropping the SDK instance.
    pub fn lock_wallet(&self) {
        if let Ok(mut guard) = self.sdk.lock() {
            *guard = None;
        }
        let _ = self.snapshot_tx.send(None);
    }

    /// Returns `true` if the wallet is currently unlocked.
    pub fn is_wallet_unlocked(&self) -> bool {
        self.sdk.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    // ── Internal: spawn_blocking SDK helper ─────────────────────────────

    /// Run a closure against the unlocked SDK on a blocking thread.
    ///
    /// The mutex is held for the entire duration of the closure, which may
    /// include blocking network I/O (e.g. Electrum). This serializes all
    /// concurrent SDK calls, which is necessary because the underlying
    /// `Wollet` is not `Sync`.
    async fn with_sdk<F, R>(&self, f: F) -> Result<R, NodeError>
    where
        F: FnOnce(&mut DeadcatSdk) -> Result<R, Error> + Send + 'static,
        R: Send + 'static,
    {
        let sdk = self.sdk.clone();
        let snapshot_tx = self.snapshot_tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = sdk.lock().map_err(|_| NodeError::MutexPoisoned)?;
            let sdk = guard.as_mut().ok_or(NodeError::WalletLocked)?;
            let result = f(sdk);
            // Capture snapshot while still holding the lock — reads cached state, no I/O
            let snapshot = WalletSnapshot {
                balance: sdk.balance().unwrap_or_default(),
                utxos: sdk.utxos().unwrap_or_default(),
                transactions: sdk.transactions().unwrap_or_default(),
            };
            let _ = snapshot_tx.send(Some(snapshot));
            result.map_err(NodeError::Sdk)
        })
        .await
        .map_err(|e| NodeError::Task(e.to_string()))?
    }

    /// Run a closure against the shared store on a blocking thread.
    async fn with_store_blocking<F, R>(&self, f: F) -> Result<R, NodeError>
    where
        F: FnOnce(&mut S) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let store = self
            .store
            .as_ref()
            .cloned()
            .ok_or_else(|| NodeError::Watcher("store is not configured".to_string()))?;
        tokio::task::spawn_blocking(move || {
            let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
            f(&mut *guard).map_err(NodeError::Watcher)
        })
        .await
        .map_err(|e| NodeError::Task(e.to_string()))?
    }

    // ── Internal: store persistence helpers ──────────────────────────────

    fn persist_market(&self, parsed: &ParsedDiscoveredMarketAnnouncement) {
        persist_market_to_store(&self.store, parsed);
    }

    fn persist_lmsr_pool_snapshot(
        &self,
        snapshot: &LmsrPoolSnapshot,
        lmsr_table_values: Option<Vec<u64>>,
    ) {
        let Some(store) = &self.store else { return };
        let reserve_outpoints = snapshot
            .current_reserve_outpoints
            .map(|outpoint| outpoint.to_string());
        let ingest = LmsrPoolIngestInput {
            pool_id: snapshot.locator.pool_id.to_hex(),
            market_id: snapshot.locator.market_id.to_string(),
            yes_asset_id: snapshot.locator.params.yes_asset_id,
            no_asset_id: snapshot.locator.params.no_asset_id,
            collateral_asset_id: snapshot.locator.params.collateral_asset_id,
            fee_bps: snapshot.locator.params.fee_bps,
            cosigner_pubkey: snapshot.locator.params.cosigner_pubkey,
            lmsr_table_root: snapshot.locator.params.lmsr_table_root,
            table_depth: snapshot.locator.params.table_depth,
            q_step_lots: snapshot.locator.params.q_step_lots,
            s_bias: snapshot.locator.params.s_bias,
            s_max_index: snapshot.locator.params.s_max_index,
            half_payout_sats: snapshot.locator.params.half_payout_sats,
            min_r_yes: snapshot.locator.params.min_r_yes,
            min_r_no: snapshot.locator.params.min_r_no,
            min_r_collateral: snapshot.locator.params.min_r_collateral,
            creation_txid: snapshot.locator.creation_txid.to_string(),
            witness_schema_version: snapshot.locator.witness_schema_version.clone(),
            initial_reserve_outpoints: snapshot
                .locator
                .initial_reserve_outpoints
                .map(|outpoint| format!("{}:{}", hex::encode(outpoint.txid), outpoint.vout)),
            current_s_index: snapshot.current_s_index,
            reserve_outpoints,
            reserve_yes: snapshot.reserves.r_yes,
            reserve_no: snapshot.reserves.r_no,
            reserve_collateral: snapshot.reserves.r_lbtc,
            state_source: LmsrPoolStateSource::CanonicalScan,
            last_transition_txid: snapshot.last_transition_txid.map(|txid| txid.to_string()),
            lmsr_table_values,
            nostr_event_id: None,
            nostr_event_json: None,
        };
        if let Ok(mut store) = store.lock() {
            let _ = store.ingest_lmsr_pool(&ingest);
        }
    }

    async fn publish_pending_order_deletions(&self, pending: Vec<PendingOrderDeletion>) {
        for order in pending {
            self.publish_order_deletion(&order).await;
        }
    }

    async fn publish_order_deletion(&self, pending: &PendingOrderDeletion) {
        let maker_base_pubkey = hex::encode(pending.maker_base_pubkey);
        let order_nonce = hex::encode(pending.order_nonce);

        let tombstone_result = self
            .discovery
            .publish_order_tombstone(
                &pending.market_id,
                &maker_base_pubkey,
                &order_nonce,
                &pending.direction_label,
                pending.price,
            )
            .await;

        match tombstone_result {
            Ok(delete_event_id) => {
                if let Some(store) = &self.store {
                    match store.lock() {
                        Ok(mut store) => {
                            if let Err(e) = store.record_order_deletion_result(
                                pending.order_id,
                                Some(&delete_event_id.to_hex()),
                                None,
                            ) {
                                log::warn!(
                                    "failed to record order tombstone for store order {}: {e}",
                                    pending.order_id
                                );
                            }
                        }
                        Err(_) => {
                            log::warn!(
                                "failed to lock store to record order tombstone for {}",
                                pending.order_id
                            );
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "failed to publish order tombstone for store order {}: {e}",
                    pending.order_id
                );
                if let Some(store) = &self.store {
                    match store.lock() {
                        Ok(mut store) => {
                            if let Err(record_err) =
                                store.record_order_deletion_result(pending.order_id, None, Some(&e))
                            {
                                log::warn!(
                                    "failed to record tombstone publish error for store order {}: {record_err}",
                                    pending.order_id
                                );
                            }
                        }
                        Err(_) => {
                            log::warn!(
                                "failed to lock store to record tombstone error for {}",
                                pending.order_id
                            );
                        }
                    }
                }
            }
        }

        if let Err(e) = self
            .discovery
            .publish_order_deletion_request(&pending.nostr_event_id, &pending.market_id)
            .await
        {
            log::warn!(
                "failed to publish NIP-09 deletion request for order {} (event {}): {e}",
                pending.order_id,
                pending.nostr_event_id
            );
        }
    }

    async fn known_prediction_markets_for_order_recovery(&self) -> Vec<PredictionMarketParams> {
        let mut known_markets = HashMap::new();

        if self.store.is_some() {
            match self
                .with_store_blocking(|store| store.list_known_prediction_markets())
                .await
            {
                Ok(markets) => {
                    for market in markets {
                        known_markets.entry(market.market_id()).or_insert(market);
                    }
                }
                Err(e) => {
                    log::warn!("failed to load known prediction markets from store: {e}");
                }
            }
        }

        match self.discovery.fetch_markets().await {
            Ok(markets) => {
                for market in markets {
                    match discovered_market_to_contract_params(&market) {
                        Ok(params) => {
                            known_markets.entry(params.market_id()).or_insert(params);
                        }
                        Err(e) => {
                            log::warn!(
                                "failed to convert discovered market {} for order recovery: {e}",
                                market.market_id
                            );
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("failed to fetch recovery market catalog from relays: {e}");
            }
        }

        known_markets.into_values().collect()
    }

    /// Resolve the next unused maker-order index for canonical app-created orders.
    pub async fn next_maker_order_index(&self) -> Result<u32, NodeError> {
        let known_markets = self.known_prediction_markets_for_order_recovery().await;

        self.with_sdk(move |sdk| sdk.next_maker_order_index_from_markets(&known_markets))
            .await
    }

    // ── Combined on-chain + Nostr operations ────────────────────────────

    /// Create a market on-chain and announce it via Nostr.
    ///
    /// Returns the parsed `DiscoveredMarket` (with Nostr event data) and the
    /// Returns the discovered market announcement persisted for the newly
    /// created on-chain market.
    ///
    /// **Non-atomic:** If the on-chain transaction succeeds but the Nostr
    /// announcement fails, the caller receives an error even though on-chain
    /// state has changed. Use [`announce_market`](Self::announce_market) to
    /// retry the announcement independently.
    pub async fn create_market(
        &self,
        oracle_pubkey: [u8; 32],
        collateral_per_token: u64,
        expiry_time: u32,
        min_utxo_value: u64,
        fee_amount: u64,
        metadata: ContractMetadata,
    ) -> Result<DiscoveredMarket, NodeError> {
        // 1. On-chain via spawn_blocking
        let (anchor, params) = self
            .with_sdk(move |sdk| {
                sdk.create_contract_onchain(
                    oracle_pubkey,
                    collateral_per_token,
                    expiry_time,
                    min_utxo_value,
                    fee_amount,
                )
            })
            .await?;

        let creation_tx = self
            .with_sdk({
                let anchor = anchor.clone();
                move |sdk| {
                    let txid = crate::parse_market_creation_txid(&anchor.creation_txid)
                        .map_err(Error::Query)?;
                    sdk.fetch_transaction(&txid)
                }
            })
            .await?;
        let creation_tx_bytes = crate::elements::encode::serialize(&creation_tx);
        let creation_tx_hex = hex::encode(&creation_tx_bytes);

        // 2. Build and publish Nostr announcement
        let announcement = ContractAnnouncement {
            version: CONTRACT_ANNOUNCEMENT_VERSION,
            contract_params: params,
            metadata,
            anchor: anchor.clone(),
            creation_tx_hex: creation_tx_hex.clone(),
        };

        let event_id = self
            .discovery
            .announce_market(&announcement)
            .await
            .map_err(NodeError::Discovery)?;

        // 3. Build DiscoveredMarket from announcement data + the real event ID
        let market_id = params.market_id();
        let nevent = Nip19Event::new(event_id, DEFAULT_RELAYS.iter().map(|r| r.to_string()))
            .to_bech32()
            .unwrap_or_default();

        let market = DiscoveredMarket {
            id: event_id.to_hex(),
            nevent,
            market_id: bytes_to_hex(market_id.as_bytes()),
            question: announcement.metadata.question.clone(),
            category: announcement.metadata.category.clone(),
            description: announcement.metadata.description.clone(),
            resolution_source: announcement.metadata.resolution_source.clone(),
            oracle_pubkey: bytes_to_hex(&params.oracle_public_key),
            expiry_height: params.expiry_time,
            cpt_sats: params.collateral_per_token,
            collateral_asset_id: bytes_to_hex(&params.collateral_asset_id),
            yes_asset_id: bytes_to_hex(&params.yes_token_asset),
            no_asset_id: bytes_to_hex(&params.no_token_asset),
            yes_reissuance_token: bytes_to_hex(&params.yes_reissuance_token),
            no_reissuance_token: bytes_to_hex(&params.no_reissuance_token),
            creator_pubkey: self.keys.public_key().to_hex(),
            created_at: nostr_sdk::Timestamp::now().as_u64(),
            anchor: announcement.anchor.clone(),
            state: 0,
            nostr_event_json: None,
            yes_price_bps: None,
            no_price_bps: None,
        };

        let parsed = ParsedDiscoveredMarketAnnouncement {
            market: market.clone(),
            ingest: PredictionMarketCandidateIngestInput {
                params,
                metadata: ContractMetadataInput {
                    question: Some(announcement.metadata.question.clone()),
                    description: Some(announcement.metadata.description.clone()),
                    category: Some(announcement.metadata.category.clone()),
                    resolution_source: Some(announcement.metadata.resolution_source.clone()),
                    creator_pubkey: hex::decode(self.keys.public_key().to_hex()).ok(),
                    anchor: announcement.anchor.clone(),
                    nevent: Some(market.nevent.clone()),
                    nostr_event_id: Some(market.id.clone()),
                    nostr_event_json: None,
                },
                creation_tx: creation_tx_bytes,
            },
        };

        // 4. Persist to store
        self.persist_market(&parsed);

        Ok(market)
    }

    /// Announce an existing market to Nostr (no on-chain operation).
    pub async fn announce_market(
        &self,
        announcement: &ContractAnnouncement,
    ) -> Result<EventId, NodeError> {
        self.discovery
            .announce_market(announcement)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Issue token pairs for an existing market.
    pub async fn issue_tokens(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<IssuanceResult, NodeError> {
        self.with_sdk(move |sdk| sdk.issue_tokens(&params, &anchor, pairs, fee_amount))
            .await
    }

    /// Create a limit order on-chain and announce it via Nostr.
    ///
    /// `direction_label` is a user-facing string describing the order (e.g.
    /// "sell-yes", "sell-no"). The caller determines this based on how they
    /// map `base_asset_id`/`quote_asset_id` to market tokens — the SDK
    /// treats base and quote as opaque asset IDs.
    ///
    /// **Non-atomic:** If the on-chain transaction succeeds but the Nostr
    /// announcement fails, the caller receives an error even though on-chain
    /// state has changed. The order can be re-announced independently via the
    /// discovery service.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_limit_order(
        &self,
        base_asset_id: [u8; 32],
        quote_asset_id: [u8; 32],
        price: u64,
        order_amount: u64,
        direction: OrderDirection,
        min_fill_lots: u64,
        min_remainder_lots: u64,
        order_index: u32,
        fee_amount: u64,
        market_id: String,
        direction_label: String,
    ) -> Result<(CreateOrderResult, EventId), NodeError> {
        // 1. On-chain
        let result = self
            .with_sdk(move |sdk| {
                sdk.create_limit_order(
                    base_asset_id,
                    quote_asset_id,
                    price,
                    order_amount,
                    direction,
                    min_fill_lots,
                    min_remainder_lots,
                    order_index,
                    fee_amount,
                )
            })
            .await?;

        // 2. Nostr announcement
        let announcement = OrderAnnouncement {
            version: 1,
            params: result.order_params,
            market_id,
            maker_base_pubkey: hex::encode(result.maker_base_pubkey),
            order_nonce: hex::encode(result.order_nonce),
            covenant_address: result.covenant_address.clone(),
            offered_amount: result.order_amount,
            direction_label,
        };

        let event_id = self
            .discovery
            .announce_order(&announcement)
            .await
            .map_err(NodeError::Discovery)?;

        if let Some(store) = &self.store {
            match store.lock() {
                Ok(mut store) => {
                    let event_id_hex = event_id.to_hex();
                    let creation_txid = result.txid.to_string();
                    if let Err(e) = store.record_own_maker_order(OwnMakerOrderRecordInput {
                        params: &result.order_params,
                        maker_pubkey: &result.maker_base_pubkey,
                        order_nonce: &result.order_nonce,
                        nostr_event_id: &event_id_hex,
                        creation_txid: &creation_txid,
                        market_id: &announcement.market_id,
                        direction_label: &announcement.direction_label,
                        offered_amount: result.order_amount,
                    }) {
                        log::warn!("failed to persist locally-created maker order: {e}");
                    }
                }
                Err(_) => {
                    log::warn!("failed to lock store to persist locally-created maker order");
                }
            }
        }

        Ok((result, event_id))
    }

    /// Cancel a limit order on-chain.
    pub async fn cancel_limit_order(
        &self,
        params: MakerOrderParams,
        maker_pubkey: [u8; 32],
        order_index: u32,
        fee_amount: u64,
    ) -> Result<CancelOrderResult, NodeError> {
        let result = self
            .with_sdk(move |sdk| {
                let resolved_order_index = sdk.resolve_order_index(&params, maker_pubkey)?;
                let effective_order_index = match (resolved_order_index, order_index) {
                    (Some(index), ORDER_INDEX_AUTO_RESOLVE_SENTINEL) => index,
                    (Some(index), provided) if provided == index => index,
                    (Some(index), provided) => {
                        return Err(Error::MakerOrder(format!(
                            "order_index {provided} does not match resolved maker order index {index}"
                        )));
                    }
                    (None, ORDER_INDEX_AUTO_RESOLVE_SENTINEL) => {
                        return Err(Error::MakerOrder(
                            "failed to resolve maker order index from maker_base_pubkey and order params"
                                .to_string(),
                        ));
                    }
                    (None, provided) => provided,
                };
                sdk.cancel_limit_order(&params, maker_pubkey, effective_order_index, fee_amount)
            })
            .await?;

        let pending = if let Some(store) = &self.store {
            match store.lock() {
                Ok(mut store) => match store.mark_own_maker_order_cancelled(&params, &maker_pubkey)
                {
                    Ok(pending) => pending,
                    Err(e) => {
                        log::warn!("failed to mark locally-created maker order cancelled: {e}");
                        None
                    }
                },
                Err(_) => {
                    log::warn!("failed to lock store to mark maker order cancelled");
                    None
                }
            }
        } else {
            None
        };

        if let Some(pending) = pending {
            self.publish_order_deletion(&pending).await;
        }

        Ok(result)
    }

    /// Fill a limit order on-chain.
    pub async fn fill_limit_order(
        &self,
        params: MakerOrderParams,
        maker_pubkey: [u8; 32],
        nonce: [u8; 32],
        lots: u64,
        fee_amount: u64,
    ) -> Result<FillOrderResult, NodeError> {
        self.with_sdk(move |sdk| {
            sdk.fill_limit_order(&params, maker_pubkey, nonce, lots, fee_amount)
        })
        .await
    }

    // ── Oracle ──────────────────────────────────────────────────────────

    /// Sign and publish an oracle attestation via Nostr.
    pub async fn attest_market(
        &self,
        market_id: &MarketId,
        announcement_event_id: &str,
        outcome_yes: bool,
    ) -> Result<AttestationResult, NodeError> {
        self.discovery
            .publish_attestation(market_id, announcement_event_id, outcome_yes)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Resolve a market on-chain with an oracle signature.
    pub async fn resolve_market(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
        outcome_yes: bool,
        oracle_sig: [u8; 64],
        fee_amount: u64,
    ) -> Result<ResolutionResult, NodeError> {
        self.with_sdk(move |sdk| {
            sdk.resolve_market(&params, &anchor, outcome_yes, oracle_sig, fee_amount)
        })
        .await
    }

    // ── Redemption ──────────────────────────────────────────────────────

    /// Redeem winning tokens after oracle resolution.
    pub async fn redeem_tokens(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
        tokens: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult, NodeError> {
        self.with_sdk(move |sdk| sdk.redeem_tokens(&params, &anchor, tokens, fee_amount))
            .await
    }

    /// Redeem tokens after market expiry (no oracle resolution).
    pub async fn redeem_expired(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
        token_asset: [u8; 32],
        tokens: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult, NodeError> {
        self.with_sdk(move |sdk| {
            sdk.redeem_expired(&params, &anchor, token_asset, tokens, fee_amount)
        })
        .await
    }

    /// Cancel token pairs by burning equal YES and NO tokens.
    pub async fn cancel_tokens(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<CancellationResult, NodeError> {
        self.with_sdk(move |sdk| sdk.cancel_tokens(&params, &anchor, pairs, fee_amount))
            .await
    }

    // ── Trade routing ────────────────────────────────────────────────────

    /// Fetch liquidity from Nostr, scan the chain, and compute a trade quote.
    ///
    /// The returned [`TradeQuote`] can be
    /// inspected for display (price, legs, totals) and then passed to
    /// [`execute_trade`](Self::execute_trade) to broadcast the transaction.
    #[allow(clippy::too_many_arguments)]
    pub async fn quote_trade(
        &self,
        contract_params: PredictionMarketParams,
        market_id: &str,
        side: TradeSide,
        direction: TradeDirection,
        amount: TradeAmount,
    ) -> Result<TradeQuote, NodeError> {
        use crate::lmsr_pool::table::LmsrTableManifest;
        use crate::maker_order::params::OrderDirection as OD;
        use crate::pset::UnblindedUtxo;
        use crate::trade::convert::{parse_discovered_lmsr_pool, parse_discovered_order};
        use crate::trade::router::{
            ScannedLmsrPool, ScannedOrder, build_execution_plan, plan_to_route_legs,
        };

        // Only ExactInput supported for now
        let total_input = match amount {
            TradeAmount::ExactInput(v) => v,
            TradeAmount::ExactOutput(_) => {
                return Err(NodeError::Sdk(Error::ExactOutputUnsupported));
            }
        };

        // 1. Fetch Nostr data
        let pools = self.fetch_pools(Some(market_id)).await?;
        let orders = self.fetch_orders(Some(market_id)).await?;

        let mut pools_by_id = HashMap::new();
        for pool in pools {
            match pools_by_id.get_mut(&pool.lmsr_pool_id) {
                None => {
                    pools_by_id.insert(pool.lmsr_pool_id.clone(), pool);
                }
                Some(existing) => {
                    let should_replace = pool.created_at > existing.created_at
                        || (pool.created_at == existing.created_at && pool.id > existing.id);
                    if should_replace {
                        *existing = pool;
                    }
                }
            }
        }
        let mut canonical_pools: Vec<_> = pools_by_id.into_values().collect();
        canonical_pools.sort_by(|a, b| a.lmsr_pool_id.cmp(&b.lmsr_pool_id));
        let network_tag = self.network.discovery_tag();

        // 2. Parse discovered LMSR pool data (fail-closed on ambiguous selection).
        let parsed_lmsr = match canonical_pools.len() {
            0 => None,
            1 => Some(
                parse_discovered_lmsr_pool(&canonical_pools[0], network_tag)
                    .map_err(NodeError::Sdk)?,
            ),
            _ => {
                return Err(NodeError::Sdk(Error::TradeRouting(
                "multiple distinct LMSR pools discovered for market; deterministic selection is required"
                    .into(),
            )));
            }
        };

        // 3. Parse discovered order data
        let parsed_orders: Vec<_> = orders
            .iter()
            .filter_map(|o| parse_discovered_order(o).ok().map(|r| (r, o.clone())))
            .collect();

        // 4. Chain scan + route (on blocking thread via SDK)
        let store = self.store.clone();
        self.with_sdk(move |sdk| {
            // Scan order UTXOs
            let mut scanned_orders = Vec::new();
            for ((params, maker_pubkey, nonce), discovered) in &parsed_orders {
                let contract = crate::maker_order::contract::CompiledMakerOrder::new(*params)?;
                let covenant_spk = contract.script_pubkey(maker_pubkey);
                let utxos = sdk.scan_covenant_utxos(&covenant_spk)?;
                if let Some((outpoint, txout)) = utxos.into_iter().next() {
                    let asset = match params.direction {
                        OD::SellBase => params.base_asset_id,
                        OD::SellQuote => params.quote_asset_id,
                    };
                    let value = txout.value.explicit().unwrap_or(0);
                    let utxo = UnblindedUtxo {
                        outpoint,
                        txout,
                        asset_id: asset,
                        value,
                        asset_blinding_factor: [0u8; 32],
                        value_blinding_factor: [0u8; 32],
                    };
                    scanned_orders.push(ScannedOrder {
                        discovered: discovered.clone(),
                        utxo,
                        maker_base_pubkey: *maker_pubkey,
                        order_nonce: *nonce,
                        params: *params,
                    });
                } else {
                    log::debug!(
                        "skipping order {} — no live UTXO on chain (spent or not yet confirmed)",
                        discovered.id,
                    );
                }
            }

            let scanned_lmsr_pool = if let Some(parsed) = parsed_lmsr.clone() {
                let table_values = parsed.table_values.clone().ok_or_else(|| {
                    Error::TradeRouting(
                        "missing required LMSR quote data: lmsr_table_values".into(),
                    )
                })?;
                let manifest = LmsrTableManifest::new(parsed.params.table_depth, table_values)?;
                manifest.verify_matches_pool_params(&parsed.params)?;

                let scan = sdk.scan_lmsr_pool_state(
                    parsed.params,
                    parsed.creation_txid,
                    parsed.initial_reserve_outpoints,
                    parsed.current_s_index,
                    &parsed.witness_schema_version,
                )?;
                let creation_txid = hex::encode(parsed.creation_txid)
                    .parse::<Txid>()
                    .map_err(|e| Error::TradeRouting(format!("invalid creation_txid: {e}")))?;
                let transition_txid = if scan.pool_utxos.yes.outpoint.txid == creation_txid {
                    None
                } else {
                    Some(scan.pool_utxos.yes.outpoint.txid.to_string())
                };
                persist_canonical_lmsr_state_to_store(
                    &store,
                    &crate::discovery::LmsrPoolStateUpdateInput {
                        pool_id: parsed.lmsr_pool_id.clone(),
                        current_s_index: scan.current_s_index,
                        reserve_outpoints: [
                            scan.pool_utxos.yes.outpoint.to_string(),
                            scan.pool_utxos.no.outpoint.to_string(),
                            scan.pool_utxos.collateral.outpoint.to_string(),
                        ],
                        reserve_yes: scan.reserves.r_yes,
                        reserve_no: scan.reserves.r_no,
                        reserve_collateral: scan.reserves.r_lbtc,
                        last_transition_txid: transition_txid,
                    },
                );

                Some(ScannedLmsrPool {
                    params: parsed.params,
                    pool_id: parsed.lmsr_pool_id,
                    current_s_index: scan.current_s_index,
                    reserves: scan.reserves,
                    pool_utxos: scan.pool_utxos,
                    manifest,
                })
            } else {
                None
            };

            // Route
            let plan = build_execution_plan(
                scanned_lmsr_pool.as_ref(),
                &scanned_orders,
                side,
                direction,
                total_input,
                &contract_params.collateral_asset_id,
                &contract_params.yes_token_asset,
                &contract_params.no_token_asset,
            )?;

            let legs = plan_to_route_legs(&plan, &scanned_orders);

            let effective_price = if plan.total_taker_output > 0 {
                plan.total_taker_input as f64 / plan.total_taker_output as f64
            } else {
                f64::INFINITY
            };

            Ok(TradeQuote {
                side,
                direction,
                amount,
                total_input: plan.total_taker_input,
                total_output: plan.total_taker_output,
                effective_price,
                legs,
                plan,
            })
        })
        .await
    }

    /// Execute a previously quoted trade.
    ///
    /// Broadcasts the transaction on-chain.
    pub async fn execute_trade(
        &self,
        quote: TradeQuote,
        fee_amount: u64,
        _market_id: &str,
    ) -> Result<TradeResult, NodeError> {
        let plan = quote.plan;
        self.with_sdk(move |sdk| sdk.execute_trade_plan(&plan, fee_amount))
            .await
    }

    /// Bootstrap a new LMSR reserve bundle on-chain and return a publish-ready announcement.
    pub async fn create_lmsr_pool(
        &self,
        request: CreateLmsrPoolRequest,
    ) -> Result<CreateLmsrPoolResult, NodeError> {
        let table_values = request.table_values.clone();
        let request_for_sdk = request.clone();
        let snapshot = self
            .with_sdk(move |sdk| sdk.create_lmsr_pool_bootstrap(&request_for_sdk))
            .await?;
        let announcement = build_pool_announcement_from_snapshot(&snapshot, table_values)
            .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
        self.persist_lmsr_pool_snapshot(&snapshot, Some(request.table_values.clone()));

        Ok(CreateLmsrPoolResult {
            txid: snapshot.locator.creation_txid,
            snapshot,
            announcement,
        })
    }

    /// Re-scan canonical LMSR reserve state from a typed pool locator.
    ///
    /// This re-derives the canonical `market_id` plus the node-network-bound
    /// canonical `pool_id` before scanning, and persists enough snapshot data
    /// to bootstrap an empty store.
    pub async fn scan_lmsr_pool(
        &self,
        locator: LmsrPoolLocator,
    ) -> Result<LmsrPoolSnapshot, NodeError> {
        if locator.hinted_s_index > locator.params.s_max_index {
            return Err(NodeError::Sdk(Error::LmsrPool(format!(
                "hinted_s_index {} exceeds s_max_index {}",
                locator.hinted_s_index, locator.params.s_max_index
            ))));
        }
        let creation_txid = locator.creation_txid;
        let creation_txid_bytes = txid_to_canonical_bytes(&creation_txid)
            .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
        let params = locator.params;
        let expected_market_id = derive_lmsr_market_id(params);
        if locator.market_id != expected_market_id {
            return Err(NodeError::Sdk(Error::LmsrPool(format!(
                "locator market_id {} does not match canonical market_id {}",
                locator.market_id, expected_market_id
            ))));
        }
        let expected_pool_id = derive_lmsr_pool_id(
            self.network,
            params,
            creation_txid_bytes,
            locator.initial_reserve_outpoints,
        )
        .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
        if locator.pool_id != expected_pool_id {
            return Err(NodeError::Sdk(Error::LmsrPool(format!(
                "locator pool_id {} does not match canonical pool_id {}",
                locator.pool_id.to_hex(),
                expected_pool_id.to_hex()
            ))));
        }
        let initial_reserve_outpoints = locator.initial_reserve_outpoints;
        let hinted_s_index = locator.hinted_s_index;
        let witness_schema_version = locator.witness_schema_version.clone();

        let scan = self
            .with_sdk(move |sdk| {
                sdk.scan_lmsr_pool_state(
                    params,
                    creation_txid_bytes,
                    initial_reserve_outpoints,
                    hinted_s_index,
                    &witness_schema_version,
                )
            })
            .await?;

        let current_reserve_outpoints = [
            scan.pool_utxos.yes.outpoint,
            scan.pool_utxos.no.outpoint,
            scan.pool_utxos.collateral.outpoint,
        ];
        let last_transition_txid = if current_reserve_outpoints[0].txid == creation_txid {
            None
        } else {
            Some(current_reserve_outpoints[0].txid)
        };
        let snapshot = LmsrPoolSnapshot {
            locator,
            current_s_index: scan.current_s_index,
            reserves: scan.reserves,
            current_reserve_outpoints,
            last_transition_txid,
        };
        self.persist_lmsr_pool_snapshot(&snapshot, None);
        persist_canonical_lmsr_state_to_store(
            &self.store,
            &crate::discovery::LmsrPoolStateUpdateInput {
                pool_id: snapshot.locator.pool_id.to_hex(),
                current_s_index: snapshot.current_s_index,
                reserve_outpoints: snapshot
                    .current_reserve_outpoints
                    .map(|outpoint| outpoint.to_string()),
                reserve_yes: snapshot.reserves.r_yes,
                reserve_no: snapshot.reserves.r_no,
                reserve_collateral: snapshot.reserves.r_lbtc,
                last_transition_txid: snapshot.last_transition_txid.map(|txid| txid.to_string()),
            },
        );

        Ok(snapshot)
    }

    /// Scan a pool and return a pre-populated adjust request with current UTXOs.
    ///
    /// The caller sets `new_reserves`, `table_values`, `fee_amount`, and
    /// `pool_index` on the returned request, then passes it to
    /// [`adjust_lmsr_pool`](Self::adjust_lmsr_pool).
    pub async fn scan_for_adjust(
        &self,
        locator: LmsrPoolLocator,
    ) -> Result<
        (
            LmsrPoolSnapshot,
            crate::lmsr_pool::api::AdjustLmsrPoolRequest,
        ),
        NodeError,
    > {
        // Reuse the same validation as scan_lmsr_pool
        if locator.hinted_s_index > locator.params.s_max_index {
            return Err(NodeError::Sdk(Error::LmsrPool(format!(
                "hinted_s_index {} exceeds s_max_index {}",
                locator.hinted_s_index, locator.params.s_max_index
            ))));
        }
        let creation_txid = locator.creation_txid;
        let creation_txid_bytes = txid_to_canonical_bytes(&creation_txid)
            .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
        let params = locator.params;
        let expected_market_id = derive_lmsr_market_id(params);
        if locator.market_id != expected_market_id {
            return Err(NodeError::Sdk(Error::LmsrPool(format!(
                "locator market_id {} does not match canonical market_id {}",
                locator.market_id, expected_market_id
            ))));
        }
        let expected_pool_id = derive_lmsr_pool_id(
            self.network,
            params,
            creation_txid_bytes,
            locator.initial_reserve_outpoints,
        )
        .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
        if locator.pool_id != expected_pool_id {
            return Err(NodeError::Sdk(Error::LmsrPool(format!(
                "locator pool_id {} does not match canonical pool_id {}",
                locator.pool_id.to_hex(),
                expected_pool_id.to_hex()
            ))));
        }
        let initial_reserve_outpoints = locator.initial_reserve_outpoints;
        let hinted_s_index = locator.hinted_s_index;
        let witness_schema_version = locator.witness_schema_version.clone();

        let scan = self
            .with_sdk(move |sdk| {
                sdk.scan_lmsr_pool_state(
                    params,
                    creation_txid_bytes,
                    initial_reserve_outpoints,
                    hinted_s_index,
                    &witness_schema_version,
                )
            })
            .await?;

        let current_reserve_outpoints = [
            scan.pool_utxos.yes.outpoint,
            scan.pool_utxos.no.outpoint,
            scan.pool_utxos.collateral.outpoint,
        ];
        let last_transition_txid = if current_reserve_outpoints[0].txid == creation_txid {
            None
        } else {
            Some(current_reserve_outpoints[0].txid)
        };
        let snapshot = LmsrPoolSnapshot {
            locator: locator.clone(),
            current_s_index: scan.current_s_index,
            reserves: scan.reserves,
            current_reserve_outpoints,
            last_transition_txid,
        };
        self.persist_lmsr_pool_snapshot(&snapshot, None);

        let request = crate::lmsr_pool::api::AdjustLmsrPoolRequest {
            locator,
            current_pool_utxos: scan.pool_utxos,
            current_s_index: scan.current_s_index,
            current_reserves: scan.reserves,
            // Caller must set these:
            new_reserves: scan.reserves, // default: no change
            table_values: Vec::new(),    // caller must provide
            fee_amount: 0,               // caller must provide
            pool_index: 0,               // caller must provide
        };

        Ok((snapshot, request))
    }

    /// Adjust LMSR pool reserves via AdminAdjust transition.
    pub async fn adjust_lmsr_pool(
        &self,
        request: crate::lmsr_pool::api::AdjustLmsrPoolRequest,
    ) -> Result<crate::lmsr_pool::api::AdjustLmsrPoolResult, NodeError> {
        let table_values = request.table_values.clone();
        let result = self
            .with_sdk(move |sdk| sdk.adjust_lmsr_pool(&request))
            .await?;
        self.persist_lmsr_pool_snapshot(&result.new_snapshot, Some(table_values));
        Ok(result)
    }

    /// Close an LMSR pool by adjusting reserves to covenant minimums.
    ///
    /// NOTE: Unlike `adjust_lmsr_pool`, this does NOT persist the post-close
    /// snapshot because `CloseLmsrPoolResult` doesn't carry one. A follow-up
    /// `scan_lmsr_pool` call will pick up the new on-chain state.
    pub async fn close_lmsr_pool(
        &self,
        request: crate::lmsr_pool::api::CloseLmsrPoolRequest,
    ) -> Result<crate::lmsr_pool::api::CloseLmsrPoolResult, NodeError> {
        self.with_sdk(move |sdk| sdk.close_lmsr_pool(&request))
            .await
    }

    // ── Discovery (delegated to DiscoveryService) ───────────────────────

    /// Fetch all markets from Nostr relays.
    pub async fn fetch_markets(&self) -> Result<Vec<DiscoveredMarket>, NodeError> {
        self.discovery
            .fetch_markets()
            .await
            .map_err(NodeError::Discovery)
    }

    /// Fetch orders from Nostr relays, optionally for a specific market.
    pub async fn fetch_orders(
        &self,
        market_id: Option<&str>,
    ) -> Result<Vec<DiscoveredOrder>, NodeError> {
        self.discovery
            .fetch_orders(market_id)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Fetch pool announcements from Nostr relays, optionally for a specific market.
    pub async fn fetch_pools(
        &self,
        market_id: Option<&str>,
    ) -> Result<Vec<crate::discovery::pool::DiscoveredPool>, NodeError> {
        self.discovery
            .fetch_pools(market_id)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Publish a pool announcement to Nostr relays.
    pub async fn announce_pool(
        &self,
        announcement: &crate::discovery::pool::PoolAnnouncement,
    ) -> Result<EventId, NodeError> {
        self.discovery
            .announce_pool(announcement)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Fetch an attestation for a specific market from Nostr relays.
    pub async fn fetch_attestation(
        &self,
        market_id_hex: &str,
    ) -> Result<Option<AttestationContent>, NodeError> {
        self.discovery
            .fetch_attestation(market_id_hex)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Start the background Nostr subscription loop.
    pub async fn start_subscription(&self) -> Result<JoinHandle<()>, NodeError> {
        self.discovery.start().await.map_err(NodeError::Discovery)
    }

    /// Get an additional broadcast receiver for discovery events.
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.discovery.subscribe()
    }

    // ── Wallet queries (via spawn_blocking) ─────────────────────────────

    /// Set the chain genesis hash for Simplicity admin operations.
    ///
    /// Required for regtest where each `elementsd` instance has a unique
    /// genesis hash.
    pub async fn set_chain_genesis_hash(&self, hash: [u8; 32]) -> Result<(), NodeError> {
        self.with_sdk(move |sdk| {
            sdk.set_chain_genesis_hash(hash);
            Ok(())
        })
        .await
    }

    /// Derive the x-only admin public key for the given pool index.
    pub async fn pool_admin_pubkey(&self, pool_index: u32) -> Result<[u8; 32], NodeError> {
        self.with_sdk(move |sdk| sdk.pool_admin_pubkey(pool_index))
            .await
    }

    /// Sync the wallet with the Electrum backend.
    pub async fn sync_wallet(&self) -> Result<(), NodeError> {
        let electrum_url = self
            .with_sdk(|sdk| {
                sdk.sync()?;
                Ok(sdk.electrum_url().to_string())
            })
            .await?;

        let pending = if self.store.is_some() {
            let electrum_url_for_store = electrum_url.clone();
            match self
                .with_store_blocking(move |store| {
                    if let Err(e) = store.sync_own_order_state(&electrum_url_for_store) {
                        log::warn!(
                            "failed to sync own maker order state from {}: {e}",
                            electrum_url_for_store
                        );
                    }
                    match store.list_pending_order_deletions() {
                        Ok(pending) => Ok(pending),
                        Err(e) => {
                            log::warn!("failed to list pending order deletions: {e}");
                            Ok(Vec::new())
                        }
                    }
                })
                .await
            {
                Ok(pending) => pending,
                Err(e) => {
                    log::warn!("failed to run store-backed wallet sync: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        self.publish_pending_order_deletions(pending).await;
        Ok(())
    }

    /// Get the wallet balance by asset (from cached snapshot — lock-free).
    pub fn balance(&self) -> Result<HashMap<AssetId, u64>, NodeError> {
        self.snapshot_rx
            .borrow()
            .as_ref()
            .map(|s| s.balance.clone())
            .ok_or(NodeError::WalletLocked)
    }

    /// Get a wallet address.
    pub async fn address(&self, index: Option<u32>) -> Result<AddressResult, NodeError> {
        self.with_sdk(move |sdk| sdk.address(index)).await
    }

    /// Get unspent wallet outputs (from cached snapshot — lock-free).
    pub fn utxos(&self) -> Result<Vec<WalletTxOut>, NodeError> {
        self.snapshot_rx
            .borrow()
            .as_ref()
            .map(|s| s.utxos.clone())
            .ok_or(NodeError::WalletLocked)
    }

    /// Get wallet transaction history (from cached snapshot — lock-free).
    pub fn transactions(&self) -> Result<Vec<WalletTx>, NodeError> {
        self.snapshot_rx
            .borrow()
            .as_ref()
            .map(|s| s.transactions.clone())
            .ok_or(NodeError::WalletLocked)
    }

    /// Fetch a raw transaction from the Electrum backend.
    pub async fn fetch_transaction(&self, txid: Txid) -> Result<Transaction, NodeError> {
        self.with_sdk(move |sdk| sdk.fetch_transaction(&txid)).await
    }

    /// Return the L-BTC policy asset ID for this network.
    pub async fn policy_asset(&self) -> Result<AssetId, NodeError> {
        self.with_sdk(|sdk| Ok(sdk.policy_asset())).await
    }

    /// Walk the canonical market lineage from the proof-carrying dormant anchor and return the
    /// current lifecycle
    /// state of the live canonical covenant bundle.
    pub async fn market_state(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
    ) -> Result<MarketState, NodeError> {
        self.with_sdk(move |sdk| {
            let contract = CompiledPredictionMarket::new(params)?;
            let (state, _utxos) = sdk.scan_market_state(&contract, &anchor)?;
            Ok(state)
        })
        .await
    }

    /// Send L-BTC to an address.
    pub async fn send_lbtc(
        &self,
        address: String,
        amount: u64,
        fee_rate: Option<f32>,
    ) -> Result<(Txid, u64), NodeError> {
        self.with_sdk(move |sdk| sdk.send_lbtc(&address, amount, fee_rate))
            .await
    }

    /// Validate a market was created with the canonical proof-carrying dormant bootstrap.
    pub async fn validate_market_creation(
        &self,
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
    ) -> Result<bool, NodeError> {
        self.with_sdk(move |sdk| sdk.validate_market_creation(&params, &anchor))
            .await
    }

    // ── Accessors ───────────────────────────────────────────────────────

    /// The Nostr identity keys.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// The network this node is configured for.
    pub fn network(&self) -> Network {
        self.network
    }

    /// Subscribe to wallet snapshot changes.
    pub fn subscribe_snapshot(&self) -> watch::Receiver<Option<WalletSnapshot>> {
        self.snapshot_rx.clone()
    }

    /// A reference to the underlying discovery service.
    pub fn discovery(&self) -> &DiscoveryService<S> {
        &self.discovery
    }

    // ── Static helpers ──────────────────────────────────────────────────

    /// Generate a new BIP-39 mnemonic suitable for wallet creation.
    ///
    /// This is a static method — the returned mnemonic can be persisted and
    /// later passed to [`unlock_wallet`](Self::unlock_wallet).
    pub fn generate_mnemonic(network: Network) -> Result<String, NodeError> {
        let (mnemonic_str, _signer) =
            DeadcatSdk::generate_mnemonic(network.is_mainnet()).map_err(NodeError::Sdk)?;
        Ok(mnemonic_str)
    }

    // ── Boltz key derivation ────────────────────────────────────────────

    /// Derive the Boltz submarine swap refund public key (hex-encoded).
    pub async fn boltz_submarine_refund_pubkey_hex(&self) -> Result<String, NodeError> {
        self.with_sdk(|sdk| sdk.boltz_submarine_refund_pubkey_hex())
            .await
    }

    /// Derive the Boltz reverse swap claim public key (hex-encoded).
    pub async fn boltz_reverse_claim_pubkey_hex(&self) -> Result<String, NodeError> {
        self.with_sdk(|sdk| sdk.boltz_reverse_claim_pubkey_hex())
            .await
    }

    // ── Electrum URL accessors ──────────────────────────────────────────

    /// Return the Electrum URL from the active SDK, or `None` if the wallet is locked.
    pub fn electrum_url(&self) -> Option<String> {
        self.sdk
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|sdk| sdk.electrum_url().to_string()))
    }

    /// Return the default Electrum URL for this node's network.
    pub fn default_electrum_url(&self) -> &str {
        self.network.default_electrum_url()
    }
}

fn market_id_from_hex(market_id: &str) -> Result<MarketId, NodeError> {
    let bytes = crate::trade::convert::hex_to_bytes32(market_id).map_err(NodeError::Sdk)?;
    Ok(MarketId(bytes))
}

#[derive(Clone, Debug)]
struct ResolvedPoolSyncMetadata {
    locator: LmsrPoolLocator,
    lmsr_table_values: Option<Vec<u64>>,
    repair_input: Option<LmsrPoolSyncRepairInput>,
}

struct EventRepairMetadata {
    market_id: String,
    creation_txid: String,
    witness_schema_version: String,
    params: crate::LmsrPoolParams,
    initial_reserve_outpoints: [crate::LmsrInitialOutpoint; 3],
    initial_reserve_outpoint_strings: [String; 3],
    lmsr_table_values: Option<Vec<u64>>,
}

fn parse_stored_initial_reserve_outpoints(
    pool: &crate::LmsrPoolSyncInfo,
) -> Result<[crate::LmsrInitialOutpoint; 3], NodeError> {
    let outpoints = pool
        .stored_initial_reserve_outpoints
        .as_ref()
        .ok_or_else(|| {
            NodeError::Sdk(Error::LmsrPool(
                "missing stored initial reserve outpoints".into(),
            ))
        })?;
    Ok([
        parse_canonical_lmsr_outpoint(&outpoints[0], "initial_reserve_outpoints[0]")
            .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?,
        parse_canonical_lmsr_outpoint(&outpoints[1], "initial_reserve_outpoints[1]")
            .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?,
        parse_canonical_lmsr_outpoint(&outpoints[2], "initial_reserve_outpoints[2]")
            .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?,
    ])
}

struct LocatorSyncParts<'a> {
    network: Network,
    pool_id_hex: &'a str,
    market_id_hex: &'a str,
    params: crate::LmsrPoolParams,
    creation_txid_hex: &'a str,
    initial_reserve_outpoints: [crate::LmsrInitialOutpoint; 3],
    hinted_s_index: u64,
    witness_schema_version: &'a str,
}

fn locator_from_sync_parts(parts: LocatorSyncParts<'_>) -> Result<LmsrPoolLocator, NodeError> {
    let market_id = market_id_from_hex(parts.market_id_hex)?;
    let pool_id = LmsrPoolId::from_hex(parts.pool_id_hex)
        .map_err(|e| NodeError::Sdk(Error::LmsrPool(e.to_string())))?;
    let creation_txid = parts
        .creation_txid_hex
        .parse::<Txid>()
        .map_err(|e| NodeError::Sdk(Error::LmsrPool(format!("invalid creation_txid: {e}"))))?;
    let creation_txid_bytes =
        txid_to_canonical_bytes(&creation_txid).map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
    let expected_pool_id = derive_lmsr_pool_id(
        parts.network,
        parts.params,
        creation_txid_bytes,
        parts.initial_reserve_outpoints,
    )
    .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
    if pool_id != expected_pool_id {
        return Err(NodeError::Sdk(Error::LmsrPool(format!(
            "stored pool_id {} does not match canonical pool_id {}",
            pool_id.to_hex(),
            expected_pool_id.to_hex()
        ))));
    }
    Ok(LmsrPoolLocator {
        market_id,
        pool_id,
        params: parts.params,
        creation_txid,
        initial_reserve_outpoints: parts.initial_reserve_outpoints,
        hinted_s_index: parts.hinted_s_index,
        witness_schema_version: parts.witness_schema_version.to_string(),
    })
}

fn build_snapshot_from_history(
    locator: LmsrPoolLocator,
    scan: &crate::sdk::LmsrPoolScanHistoryResult,
) -> LmsrPoolSnapshot {
    let current_reserve_outpoints = [
        scan.pool_utxos.yes.outpoint,
        scan.pool_utxos.no.outpoint,
        scan.pool_utxos.collateral.outpoint,
    ];
    let last_transition_txid = scan
        .transitions
        .last()
        .map(|transition| transition.transition_txid);
    LmsrPoolSnapshot {
        locator,
        current_s_index: scan.current_s_index,
        reserves: scan.reserves,
        current_reserve_outpoints,
        last_transition_txid,
    }
}

fn is_irreversible_transition(network: Network, best_block_height: u32, block_height: u32) -> bool {
    best_block_height
        .checked_sub(block_height)
        .map(|diff| diff + 1 >= network.irreversible_confirmations())
        .unwrap_or(false)
}

fn repair_metadata_from_nostr_event(
    network: Network,
    pool: &crate::LmsrPoolSyncInfo,
) -> Result<EventRepairMetadata, NodeError> {
    let event_json = pool
        .nostr_event_json
        .as_deref()
        .ok_or_else(|| NodeError::Sdk(Error::LmsrPool("missing nostr_event_json".into())))?;
    let event: Event = serde_json::from_str(event_json)
        .map_err(|e| NodeError::Sdk(Error::LmsrPool(format!("decode nostr_event_json: {e}"))))?;
    let discovered = parse_pool_event(&event, network.discovery_tag())
        .map_err(|e| NodeError::Sdk(Error::LmsrPool(format!("parse stored pool event: {e}"))))?;
    if discovered.pool_id != pool.pool_id {
        return Err(NodeError::Sdk(Error::LmsrPool(format!(
            "stored pool_id {} does not match Nostr event pool_id {}",
            pool.pool_id, discovered.pool_id
        ))));
    }
    let parsed =
        crate::trade::convert::parse_discovered_lmsr_pool(&discovered, network.discovery_tag())
            .map_err(NodeError::Sdk)?;
    let initial_reserve_outpoint_strings: [String; 3] = discovered
        .initial_reserve_outpoints
        .clone()
        .try_into()
        .map_err(|outpoints: Vec<String>| {
            NodeError::Sdk(Error::LmsrPool(format!(
                "expected 3 Nostr initial reserve outpoints, got {}",
                outpoints.len()
            )))
        })?;

    Ok(EventRepairMetadata {
        market_id: discovered.market_id,
        creation_txid: discovered.creation_txid,
        witness_schema_version: parsed.witness_schema_version,
        params: parsed.params,
        initial_reserve_outpoints: parsed.initial_reserve_outpoints,
        initial_reserve_outpoint_strings,
        lmsr_table_values: parsed.table_values,
    })
}

fn resolved_sync_metadata(
    network: Network,
    pool: &crate::LmsrPoolSyncInfo,
) -> Result<ResolvedPoolSyncMetadata, NodeError> {
    let mut resolution_errors = Vec::new();
    let stored_locator = match serde_json::from_str::<crate::LmsrPoolParams>(&pool.params_json) {
        Ok(params) => {
            match parse_stored_initial_reserve_outpoints(pool).and_then(|initial_outpoints| {
                locator_from_sync_parts(LocatorSyncParts {
                    network,
                    pool_id_hex: &pool.pool_id,
                    market_id_hex: &pool.market_id,
                    params,
                    creation_txid_hex: &pool.creation_txid,
                    initial_reserve_outpoints: initial_outpoints,
                    hinted_s_index: pool.current_s_index,
                    witness_schema_version: &pool.witness_schema_version,
                })
            }) {
                Ok(locator) => Some(locator),
                Err(err) => {
                    resolution_errors.push(format!("stored metadata invalid: {err}"));
                    None
                }
            }
        }
        Err(err) => {
            resolution_errors.push(format!(
                "decode stored lmsr params: {err} (unsupported legacy local DB format; recreate the local DB)"
            ));
            None
        }
    };

    let needs_repair = stored_locator.is_none() || pool.lmsr_table_values.is_none();
    let repaired = if needs_repair {
        match repair_metadata_from_nostr_event(network, pool) {
            Ok(repaired) => Some(repaired),
            Err(err) => {
                resolution_errors.push(format!("nostr repair failed: {err}"));
                None
            }
        }
    } else {
        None
    };

    if let Some(repaired) = repaired {
        let locator = locator_from_sync_parts(LocatorSyncParts {
            network,
            pool_id_hex: &pool.pool_id,
            market_id_hex: &repaired.market_id,
            params: repaired.params,
            creation_txid_hex: &repaired.creation_txid,
            initial_reserve_outpoints: repaired.initial_reserve_outpoints,
            hinted_s_index: pool.current_s_index,
            witness_schema_version: &repaired.witness_schema_version,
        })?;
        let params_json = serde_json::to_string(&repaired.params).map_err(|e| {
            NodeError::Sdk(Error::LmsrPool(format!("serialize repaired params: {e}")))
        })?;
        let should_repair = pool.market_id != repaired.market_id
            || pool.creation_txid != repaired.creation_txid
            || pool.witness_schema_version != repaired.witness_schema_version
            || pool.stored_initial_reserve_outpoints.as_ref()
                != Some(&repaired.initial_reserve_outpoint_strings)
            || pool.params_json != params_json
            || (pool.lmsr_table_values.is_none() && repaired.lmsr_table_values.is_some());
        return Ok(ResolvedPoolSyncMetadata {
            locator,
            lmsr_table_values: pool
                .lmsr_table_values
                .clone()
                .or_else(|| repaired.lmsr_table_values.clone()),
            repair_input: should_repair.then_some(LmsrPoolSyncRepairInput {
                pool_id: pool.pool_id.clone(),
                market_id: repaired.market_id,
                creation_txid: repaired.creation_txid,
                witness_schema_version: repaired.witness_schema_version,
                params: repaired.params,
                initial_reserve_outpoints: repaired.initial_reserve_outpoint_strings,
                lmsr_table_values: repaired.lmsr_table_values,
            }),
        });
    }

    if let Some(locator) = stored_locator {
        return Ok(ResolvedPoolSyncMetadata {
            locator,
            lmsr_table_values: pool.lmsr_table_values.clone(),
            repair_input: None,
        });
    }

    Err(NodeError::Sdk(Error::LmsrPool(format!(
        "cannot resolve LMSR sync metadata for pool {}: {}",
        pool.pool_id,
        resolution_errors.join("; ")
    ))))
}

impl<S: NodeStore> DeadcatNode<S> {
    fn resolve_and_repair_pool_sync_metadata(
        &self,
        pool: crate::LmsrPoolSyncInfo,
    ) -> Result<ResolvedPoolSyncMetadata, NodeError> {
        let resolved = resolved_sync_metadata(self.network, &pool)?;
        if let Some(repair_input) = resolved.repair_input.as_ref() {
            let store = self
                .store
                .as_ref()
                .cloned()
                .ok_or_else(|| NodeError::Store("node store not configured".into()))?;
            let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
            guard
                .repair_lmsr_pool_sync_info(repair_input)
                .map_err(NodeError::Store)?;
        }
        Ok(resolved)
    }

    /// Resolve a persisted LMSR pool row into a canonical locator, repairing
    /// recoverable metadata first. Stored initial reserve outpoints are read in
    /// canonical raw-hex byte order so pool-id derivation matches discovery.
    pub fn resolve_lmsr_pool_locator(&self, pool_id: &str) -> Result<LmsrPoolLocator, NodeError> {
        let store = self
            .store
            .as_ref()
            .cloned()
            .ok_or_else(|| NodeError::Store("node store not configured".into()))?;
        let pool = {
            let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
            guard
                .list_lmsr_pool_sync_info()
                .map_err(NodeError::Store)?
                .into_iter()
                .find(|pool| pool.pool_id == pool_id)
                .ok_or_else(|| NodeError::Store(format!("unknown LMSR pool_id {pool_id}")))?
        };
        self.resolve_and_repair_pool_sync_metadata(pool)
            .map(|resolved| resolved.locator)
    }

    /// Sync wallet state and backfill irreversible LMSR transition history.
    pub async fn sync(&self) -> Result<(), NodeError> {
        self.sync_wallet().await?;

        let store = self
            .store
            .as_ref()
            .cloned()
            .ok_or_else(|| NodeError::Store("node store not configured".into()))?;
        let pools = {
            let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
            guard.list_lmsr_pool_sync_info().map_err(NodeError::Store)?
        };

        for pool in pools {
            let resolved = match self.resolve_and_repair_pool_sync_metadata(pool.clone()) {
                Ok(resolved) => resolved,
                Err(err) => {
                    log::warn!("skipping LMSR sync for pool {}: {}", pool.pool_id, err);
                    continue;
                }
            };
            let locator = resolved.locator.clone();
            let pool_id = locator.pool_id.to_hex();
            let market_id = locator.market_id.to_string();
            let params = locator.params;
            let creation_txid_bytes = txid_to_canonical_bytes(&locator.creation_txid)
                .map_err(|e| NodeError::Sdk(Error::LmsrPool(e)))?;
            let initial_reserve_outpoints = locator.initial_reserve_outpoints;
            let hinted_s_index = locator.hinted_s_index;
            let witness_schema_version = locator.witness_schema_version.clone();
            let scan = self
                .with_sdk(move |sdk| {
                    sdk.scan_lmsr_pool_state_with_history(
                        params,
                        creation_txid_bytes,
                        initial_reserve_outpoints,
                        hinted_s_index,
                        &witness_schema_version,
                    )
                })
                .await?;

            let snapshot = build_snapshot_from_history(locator, &scan);
            self.persist_lmsr_pool_snapshot(&snapshot, resolved.lmsr_table_values.clone());

            let Some(table_values) = resolved.lmsr_table_values.clone() else {
                log::warn!(
                    "skipping LMSR price history ingestion for pool {}: missing lmsr_table_values",
                    pool_id
                );
                continue;
            };
            let manifest = LmsrTableManifest::new(params.table_depth, table_values)
                .and_then(|manifest| {
                    manifest.verify_matches_pool_params(&params)?;
                    Ok(manifest)
                })
                .map_err(|e| NodeError::Sdk(Error::LmsrPool(e.to_string())))?;
            let transitions: Result<Vec<LmsrPriceTransitionInput>, NodeError> = scan
                .transitions
                .into_iter()
                .filter_map(|transition| {
                    let block_height = transition.block_height?;
                    is_irreversible_transition(self.network, scan.best_block_height, block_height)
                        .then_some((transition, block_height))
                })
                .map(|(transition, block_height)| {
                    Ok(LmsrPriceTransitionInput {
                        pool_id: pool_id.clone(),
                        market_id: market_id.clone(),
                        transition_txid: transition.transition_txid.to_string(),
                        old_s_index: transition.old_s_index,
                        new_s_index: transition.new_s_index,
                        reserve_yes: transition.reserves.r_yes,
                        reserve_no: transition.reserves.r_no,
                        reserve_collateral: transition.reserves.r_lbtc,
                        implied_yes_price_bps: fee_free_yes_spot_price_bps(
                            &manifest,
                            &params,
                            transition.new_s_index,
                        )
                        .map_err(NodeError::Sdk)?,
                        block_height,
                    })
                })
                .collect();
            let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
            for transition in transitions? {
                guard
                    .record_lmsr_price_transition(&transition)
                    .map_err(NodeError::Store)?;
            }
        }

        Ok(())
    }

    pub fn get_market_price_history(
        &self,
        market_id: &str,
        since_block_height: Option<u32>,
        limit: Option<i64>,
    ) -> Result<Vec<LmsrPriceHistoryEntry>, NodeError> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| NodeError::Store("node store not configured".into()))?;
        let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
        guard
            .get_market_price_history(market_id, since_block_height, limit)
            .map_err(NodeError::Store)
    }

    pub fn get_pool_price_history(
        &self,
        pool_id: &str,
        since_block_height: Option<u32>,
        limit: Option<i64>,
    ) -> Result<Vec<LmsrPriceHistoryEntry>, NodeError> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| NodeError::Store("node store not configured".into()))?;
        let mut guard = store.lock().map_err(|_| NodeError::MutexPoisoned)?;
        guard
            .get_pool_price_history(pool_id, since_block_height, limit)
            .map_err(NodeError::Store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::pool::{
        LMSR_WITNESS_SCHEMA_V2, PoolAnnouncement, PoolParams, build_pool_event,
    };

    fn sample_table_values() -> Vec<u64> {
        vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175]
    }

    fn sample_params(table_values: &[u64]) -> crate::LmsrPoolParams {
        crate::LmsrPoolParams {
            yes_asset_id: [0x11; 32],
            no_asset_id: [0x22; 32],
            collateral_asset_id: [0x33; 32],
            lmsr_table_root: crate::lmsr_table_root(table_values).unwrap(),
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            fee_bps: 30,
            min_r_yes: 7,
            min_r_no: 8,
            min_r_collateral: 9,
            cosigner_pubkey: [0x44; 32],
        }
    }

    fn sample_pool_announcement() -> PoolAnnouncement {
        let table_values = sample_table_values();
        let params = sample_params(&table_values);
        let creation_txid =
            "00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f".to_string();
        let initial_reserve_outpoints = vec![
            format!("{creation_txid}:0"),
            format!("{creation_txid}:1"),
            format!("{creation_txid}:2"),
        ];
        let creation_txid_bytes = crate::trade::convert::hex_to_bytes32(&creation_txid).unwrap();
        let parsed_outpoints = [
            crate::LmsrInitialOutpoint {
                txid: creation_txid_bytes,
                vout: 0,
            },
            crate::LmsrInitialOutpoint {
                txid: creation_txid_bytes,
                vout: 1,
            },
            crate::LmsrInitialOutpoint {
                txid: creation_txid_bytes,
                vout: 2,
            },
        ];
        let pool_id = derive_lmsr_pool_id(
            Network::LiquidTestnet,
            params,
            creation_txid_bytes,
            parsed_outpoints,
        )
        .unwrap()
        .to_hex();

        PoolAnnouncement {
            version: crate::discovery::pool::LMSR_POOL_ANNOUNCEMENT_VERSION,
            params: PoolParams {
                yes_asset_id: params.yes_asset_id,
                no_asset_id: params.no_asset_id,
                lbtc_asset_id: params.collateral_asset_id,
                fee_bps: params.fee_bps,
                min_r_yes: params.min_r_yes,
                min_r_no: params.min_r_no,
                min_r_collateral: params.min_r_collateral,
                cosigner_pubkey: params.cosigner_pubkey,
            },
            market_id: derive_lmsr_market_id(params).to_string(),
            reserves: crate::PoolReserves {
                r_yes: 200_000,
                r_no: 200_000,
                r_lbtc: 300_000,
            },
            creation_txid,
            lmsr_pool_id: pool_id,
            lmsr_table_root: hex::encode(params.lmsr_table_root),
            table_depth: params.table_depth,
            q_step_lots: params.q_step_lots,
            s_bias: params.s_bias,
            s_max_index: params.s_max_index,
            half_payout_sats: params.half_payout_sats,
            current_s_index: 4,
            initial_reserve_outpoints,
            witness_schema_version: LMSR_WITNESS_SCHEMA_V2.to_string(),
            table_manifest_hash: None,
            lmsr_table_values: Some(table_values),
        }
    }

    fn canonical_params_json(announcement: &PoolAnnouncement) -> String {
        serde_json::to_string(&sample_params(
            announcement
                .lmsr_table_values
                .as_ref()
                .expect("sample announcement includes table values"),
        ))
        .unwrap()
    }

    #[test]
    fn resolved_sync_metadata_accepts_canonical_stored_anchors_without_repair() {
        let announcement = sample_pool_announcement();
        let pool = crate::LmsrPoolSyncInfo {
            pool_id: announcement.lmsr_pool_id.clone(),
            market_id: announcement.market_id.clone(),
            creation_txid: announcement.creation_txid.clone(),
            stored_initial_reserve_outpoints: Some(
                announcement
                    .initial_reserve_outpoints
                    .clone()
                    .try_into()
                    .expect("sample announcement has 3 initial reserve outpoints"),
            ),
            witness_schema_version: announcement.witness_schema_version.clone(),
            current_s_index: announcement.current_s_index,
            params_json: canonical_params_json(&announcement),
            lmsr_table_values: announcement.lmsr_table_values.clone(),
            nostr_event_json: None,
        };

        let resolved = resolved_sync_metadata(Network::LiquidTestnet, &pool).unwrap();
        assert_eq!(resolved.locator.pool_id.to_hex(), announcement.lmsr_pool_id);
        assert_eq!(
            resolved.locator.creation_txid.to_string(),
            announcement.creation_txid
        );
        assert_eq!(resolved.lmsr_table_values, announcement.lmsr_table_values);
        assert!(resolved.repair_input.is_none());
    }

    #[test]
    fn resolved_sync_metadata_repairs_poisoned_anchors_from_nostr_event() {
        let announcement = sample_pool_announcement();
        let event = build_pool_event(
            &Keys::generate(),
            &announcement,
            Network::LiquidTestnet.discovery_tag(),
        )
        .unwrap();
        let pool = crate::LmsrPoolSyncInfo {
            pool_id: announcement.lmsr_pool_id.clone(),
            market_id: announcement.market_id.clone(),
            creation_txid: announcement.creation_txid.clone(),
            stored_initial_reserve_outpoints: Some([
                format!("{}:7", announcement.creation_txid),
                format!("{}:8", announcement.creation_txid),
                format!("{}:9", announcement.creation_txid),
            ]),
            witness_schema_version: announcement.witness_schema_version.clone(),
            current_s_index: announcement.current_s_index,
            params_json: canonical_params_json(&announcement),
            lmsr_table_values: None,
            nostr_event_json: Some(serde_json::to_string(&event).unwrap()),
        };

        let resolved = resolved_sync_metadata(Network::LiquidTestnet, &pool).unwrap();
        assert_eq!(resolved.locator.pool_id.to_hex(), announcement.lmsr_pool_id);
        assert_eq!(resolved.locator.initial_reserve_outpoints[0].vout, 0);
        assert_eq!(resolved.lmsr_table_values, announcement.lmsr_table_values);
        let repair = resolved.repair_input.expect("repair input");
        assert_eq!(repair.params.min_r_yes, announcement.params.min_r_yes);
        assert_eq!(
            repair.initial_reserve_outpoints[0],
            announcement.initial_reserve_outpoints[0]
        );
    }

    #[test]
    fn resolved_sync_metadata_errors_when_pool_is_unrecoverable() {
        let announcement = sample_pool_announcement();
        let params_json = canonical_params_json(&announcement);
        let pool = crate::LmsrPoolSyncInfo {
            pool_id: announcement.lmsr_pool_id,
            market_id: announcement.market_id,
            creation_txid: announcement.creation_txid,
            stored_initial_reserve_outpoints: None,
            witness_schema_version: announcement.witness_schema_version,
            current_s_index: announcement.current_s_index,
            params_json,
            lmsr_table_values: None,
            nostr_event_json: None,
        };

        let err = resolved_sync_metadata(Network::LiquidTestnet, &pool).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot resolve LMSR sync metadata")
        );
    }
}

#[cfg(test)]
mod order_cleanup_tests {
    use super::*;
    use std::time::Duration;

    use lwk_test_util::{TEST_MNEMONIC, TestEnvBuilder, regtest_policy_asset};
    use nostr_relay_builder::prelude::*;

    use crate::testing::{TestStore, test_order_announcement};

    fn sample_order_market_params(collateral_asset_id: [u8; 32]) -> PredictionMarketParams {
        PredictionMarketParams {
            oracle_public_key: [0x21; 32],
            collateral_asset_id,
            yes_token_asset: [0x31; 32],
            no_token_asset: [0x41; 32],
            yes_reissuance_token: [0x51; 32],
            no_reissuance_token: [0x61; 32],
            collateral_per_token: 1_000,
            expiry_time: 123_456,
        }
    }

    #[tokio::test]
    async fn publish_order_deletion_publishes_tombstone_and_kind5_cleanup() {
        let mock = MockRelay::run().await.unwrap();
        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![mock.url()],
            network_tag: "liquid-testnet".to_string(),
            ..Default::default()
        };
        let (node, _rx) =
            DeadcatNode::with_store(keys, Network::LiquidTestnet, store.clone(), config);

        let announcement = test_order_announcement("market-node-delete");
        let event_id = node
            .discovery()
            .announce_order(&announcement)
            .await
            .unwrap();
        let pending = PendingOrderDeletion {
            order_id: 7,
            market_id: announcement.market_id.clone(),
            direction_label: announcement.direction_label.clone(),
            maker_base_pubkey: [0xaa; 32],
            order_nonce: [0x11; 32],
            price: announcement.params.price,
            nostr_event_id: event_id.to_hex(),
        };

        node.publish_order_deletion(&pending).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        let orders = node
            .fetch_orders(Some(&announcement.market_id))
            .await
            .unwrap();
        assert!(
            orders.is_empty(),
            "node cleanup should hide the original order"
        );

        let deletion_events = node
            .discovery()
            .client()
            .fetch_events(
                vec![Filter::new().kind(Kind::Custom(5))],
                Duration::from_secs(5),
            )
            .await
            .unwrap();
        let deletion_event = deletion_events
            .iter()
            .find(|event| {
                event.tags.iter().any(|tag| {
                    let fields = tag.as_slice();
                    fields.len() >= 2 && fields[0] == "e" && fields[1] == pending.nostr_event_id
                })
            })
            .expect("expected a NIP-09 deletion request for the original order event");
        let hashtags: Vec<_> = deletion_event
            .tags
            .iter()
            .filter_map(|tag| {
                let fields = tag.as_slice();
                (fields.len() >= 2 && fields[0] == "t").then(|| fields[1].to_string())
            })
            .collect();
        assert!(hashtags.iter().any(|tag| tag == "order"));
        assert!(hashtags.iter().any(|tag| tag == &pending.market_id));
        assert!(deletion_event.tags.iter().any(|tag| {
            let fields = tag.as_slice();
            fields.len() >= 2 && fields[0] == "network" && fields[1] == "liquid-testnet"
        }));

        let store = store.lock().unwrap();
        assert_eq!(store.deletion_results.len(), 1);
        assert_eq!(store.deletion_results[0].order_id, pending.order_id);
        assert!(store.deletion_results[0].delete_event_id.is_some());
        assert!(store.deletion_results[0].error.is_none());
    }

    #[tokio::test]
    async fn sync_wallet_runs_store_sync_and_publishes_pending_deletions() {
        if std::env::var_os("ELEMENTSD_EXEC").is_none() {
            return;
        }

        let env = TestEnvBuilder::from_env().with_electrum().build();
        let mock = MockRelay::run().await.unwrap();
        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![mock.url()],
            network_tag: "liquid-regtest".to_string(),
            ..Default::default()
        };
        let (node, _rx) =
            DeadcatNode::with_store(keys, Network::LiquidRegtest, store.clone(), config);

        let wallet_dir = tempfile::tempdir().unwrap();
        node.unlock_wallet(TEST_MNEMONIC, &env.electrum_url(), wallet_dir.path())
            .unwrap();

        let announcement = test_order_announcement("market-node-sync-wallet");
        let event_id = node
            .discovery()
            .announce_order(&announcement)
            .await
            .unwrap();
        {
            let mut store = store.lock().unwrap();
            store.pending_order_deletions.push(PendingOrderDeletion {
                order_id: 9,
                market_id: announcement.market_id.clone(),
                direction_label: announcement.direction_label.clone(),
                maker_base_pubkey: [0xaa; 32],
                order_nonce: [0x11; 32],
                price: announcement.params.price,
                nostr_event_id: event_id.to_hex(),
            });
        }

        node.sync_wallet().await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        let orders = node
            .fetch_orders(Some(&announcement.market_id))
            .await
            .unwrap();
        assert!(
            orders.is_empty(),
            "sync should publish the pending order cleanup"
        );

        let store = store.lock().unwrap();
        assert_eq!(store.synced_electrum_urls, vec![env.electrum_url()]);
        assert_eq!(store.deletion_results.len(), 1);
        assert_eq!(store.deletion_results[0].order_id, 9);
        assert!(store.deletion_results[0].delete_event_id.is_some());
        assert!(store.deletion_results[0].error.is_none());
    }

    #[tokio::test]
    async fn cancel_limit_order_auto_resolves_missing_index() {
        if std::env::var_os("ELEMENTSD_EXEC").is_none() {
            return;
        }

        let env = TestEnvBuilder::from_env().with_electrum().build();
        let mock = MockRelay::run().await.unwrap();
        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![mock.url()],
            network_tag: "liquid-regtest".to_string(),
            ..Default::default()
        };
        let (node, _rx) =
            DeadcatNode::with_store(keys, Network::LiquidRegtest, store.clone(), config);

        let wallet_dir = tempfile::tempdir().unwrap();
        node.unlock_wallet(TEST_MNEMONIC, &env.electrum_url(), wallet_dir.path())
            .unwrap();

        for _ in 0..3 {
            let addr = node.address(None).await.unwrap();
            env.elementsd_sendtoaddress(addr.address(), 200_000, None);
        }
        env.elementsd_generate(1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        node.sync_wallet().await.unwrap();

        let market =
            sample_order_market_params(regtest_policy_asset().into_inner().to_byte_array());
        let market_id = market.market_id().to_string();
        let (created, _event_id) = node
            .create_limit_order(
                market.yes_token_asset,
                market.collateral_asset_id,
                27,
                10_000,
                OrderDirection::SellQuote,
                1,
                1,
                1,
                500,
                market_id,
                "buy-yes".to_string(),
            )
            .await
            .unwrap();

        env.elementsd_generate(1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        node.sync_wallet().await.unwrap();

        let cancelled = node
            .cancel_limit_order(
                created.order_params,
                created.maker_base_pubkey,
                ORDER_INDEX_AUTO_RESOLVE_SENTINEL,
                500,
            )
            .await
            .unwrap();
        assert_eq!(cancelled.refunded_amount, 10_000);

        let store = store.lock().unwrap();
        assert_eq!(store.cancelled_orders.len(), 1);
        assert_eq!(store.cancelled_orders[0].0, created.order_params);
        assert_eq!(store.cancelled_orders[0].1, created.maker_base_pubkey);
    }

    #[tokio::test]
    async fn cancel_limit_order_rejects_mismatched_explicit_index() {
        if std::env::var_os("ELEMENTSD_EXEC").is_none() {
            return;
        }

        let env = TestEnvBuilder::from_env().with_electrum().build();
        let mock = MockRelay::run().await.unwrap();
        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![mock.url()],
            network_tag: "liquid-regtest".to_string(),
            ..Default::default()
        };
        let (node, _rx) = DeadcatNode::with_store(keys, Network::LiquidRegtest, store, config);

        let wallet_dir = tempfile::tempdir().unwrap();
        node.unlock_wallet(TEST_MNEMONIC, &env.electrum_url(), wallet_dir.path())
            .unwrap();

        for _ in 0..3 {
            let addr = node.address(None).await.unwrap();
            env.elementsd_sendtoaddress(addr.address(), 200_000, None);
        }
        env.elementsd_generate(1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        node.sync_wallet().await.unwrap();

        let market =
            sample_order_market_params(regtest_policy_asset().into_inner().to_byte_array());
        let market_id = market.market_id().to_string();
        let (created, _event_id) = node
            .create_limit_order(
                market.yes_token_asset,
                market.collateral_asset_id,
                27,
                10_000,
                OrderDirection::SellQuote,
                1,
                1,
                1,
                500,
                market_id,
                "buy-yes".to_string(),
            )
            .await
            .unwrap();

        let err = node
            .cancel_limit_order(created.order_params, created.maker_base_pubkey, 0, 500)
            .await
            .expect_err("mismatched explicit index should be rejected");
        assert!(
            err.to_string()
                .contains("does not match resolved maker order index 1"),
            "unexpected error: {err}"
        );
    }
}
