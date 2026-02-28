use std::sync::{Arc, Mutex};
use std::time::Duration;

use nostr_sdk::prelude::*;
use tokio::sync::broadcast;

use crate::announcement::ContractAnnouncement;
use crate::prediction_market::params::MarketId;

use super::attestation::{
    AttestationContent, AttestationResult, build_attestation_event, build_attestation_filter,
    build_attestation_subscription_filter, parse_attestation_event, sign_attestation,
};
use super::config::DiscoveryConfig;
use super::events::DiscoveryEvent;
use super::market::{
    DiscoveredMarket, build_announcement_event, build_contract_filter, parse_announcement_event,
};
use super::pool::{
    DiscoveredPool, PoolAnnouncement, build_pool_event, build_pool_filter, parse_pool_event,
};
use super::store_trait::{DiscoveredMarketMetadata, DiscoveryStore};
use super::{
    ATTESTATION_TAG, CONTRACT_TAG, DiscoveredOrder, ORDER_TAG, OrderAnnouncement, POOL_TAG,
    build_order_event, build_order_filter, parse_order_event,
};

/// Decode a hex string into a fixed-size 32-byte array.
fn hex_to_bytes32(hex_str: &str, field: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("{field}: hex decode: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{field}: expected 32 bytes"))
}

/// Stats returned by `DiscoveryService::reconcile()`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ReconciliationStats {
    pub events_sent: usize,
    pub events_failed: usize,
    pub events_skipped: usize,
}

impl std::fmt::Display for ReconciliationStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "sent={}, failed={}, skipped={}",
            self.events_sent, self.events_failed, self.events_skipped
        )
    }
}

/// Unified Nostr discovery service for markets, orders, and attestations.
///
/// Subscribes to Nostr relays, pushes real-time `DiscoveryEvent` notifications
/// via `tokio::broadcast`, and optionally persists discovered data to a shared store.
pub struct DiscoveryService<S: DiscoveryStore = NoopStore> {
    client: Client,
    keys: Keys,
    config: DiscoveryConfig,
    store: Option<Arc<Mutex<S>>>,
    tx: broadcast::Sender<DiscoveryEvent>,
}

/// A no-op store implementation for when persistence is not needed.
pub struct NoopStore;

impl DiscoveryStore for NoopStore {
    fn ingest_market(
        &mut self,
        _params: &crate::prediction_market::params::PredictionMarketParams,
        _meta: Option<&DiscoveredMarketMetadata>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn ingest_maker_order(
        &mut self,
        _params: &crate::maker_order::params::MakerOrderParams,
        _maker_pubkey: Option<&[u8; 32]>,
        _nonce: Option<&[u8; 32]>,
        _nostr_event_id: Option<&str>,
        _nostr_event_json: Option<&str>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn ingest_amm_pool(
        &mut self,
        _params: &crate::amm_pool::params::AmmPoolParams,
        _issued_lp: u64,
        _nostr_event_id: Option<&str>,
        _nostr_event_json: Option<&str>,
        _market_id: Option<&[u8; 32]>,
        _creation_txid: Option<&[u8; 32]>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn update_pool_state(
        &mut self,
        _pool_id: &crate::amm_pool::params::PoolId,
        _params: &crate::amm_pool::params::AmmPoolParams,
        _issued_lp: u64,
    ) -> Result<(), String> {
        Ok(())
    }

    fn get_pool_info(
        &mut self,
        _pool_id: &crate::amm_pool::params::PoolId,
    ) -> Result<Option<super::store_trait::PoolInfo>, String> {
        Ok(None)
    }

    fn get_latest_pool_snapshot_resume(
        &mut self,
        _pool_id: &[u8; 32],
    ) -> Result<Option<([u8; 32], u64)>, String> {
        Ok(None)
    }

    fn insert_pool_snapshot(
        &mut self,
        _pool_id: &[u8; 32],
        _txid: &[u8; 32],
        _r_yes: u64,
        _r_no: u64,
        _r_lbtc: u64,
        _issued_lp: u64,
        _block_height: Option<i32>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn get_pool_id_for_market(
        &mut self,
        _market_id: &crate::prediction_market::params::MarketId,
    ) -> Result<Option<crate::amm_pool::params::PoolId>, String> {
        Ok(None)
    }

    fn get_latest_pool_snapshot(
        &mut self,
        _pool_id: &crate::amm_pool::params::PoolId,
    ) -> Result<Option<super::store_trait::PoolSnapshot>, String> {
        Ok(None)
    }

    fn get_pool_snapshot_history(
        &mut self,
        _pool_id: &crate::amm_pool::params::PoolId,
    ) -> Result<Vec<super::store_trait::PoolSnapshot>, String> {
        Ok(vec![])
    }

    fn get_all_market_spks(&mut self) -> Result<Vec<([u8; 32], Vec<Vec<u8>>)>, String> {
        Ok(vec![])
    }

    fn get_all_pool_watch_info(
        &mut self,
    ) -> Result<Vec<(crate::amm_pool::params::PoolId, Vec<u8>)>, String> {
        Ok(vec![])
    }

    fn get_all_nostr_events(&mut self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
}

impl DiscoveryService<NoopStore> {
    /// Create a new `DiscoveryService` without store persistence.
    ///
    /// Returns the service and a broadcast receiver for discovery events.
    pub fn new(keys: Keys, config: DiscoveryConfig) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        let (tx, rx) = broadcast::channel(256);
        let client = Client::new(keys.clone());
        (
            Self {
                client,
                keys,
                config,
                store: None,
                tx,
            },
            rx,
        )
    }
}

impl<S: DiscoveryStore> DiscoveryService<S> {
    /// Create a new `DiscoveryService` with store persistence.
    ///
    /// Returns the service and a broadcast receiver for discovery events.
    pub fn with_store(
        keys: Keys,
        store: Arc<Mutex<S>>,
        config: DiscoveryConfig,
    ) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        let (tx, rx) = broadcast::channel(256);
        let client = Client::new(keys.clone());
        (
            Self {
                client,
                keys,
                config,
                store: Some(store),
                tx,
            },
            rx,
        )
    }

    /// Get an additional broadcast receiver for discovery events.
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.tx.subscribe()
    }

    /// Connect to configured relays and start the background subscription loop.
    ///
    /// Spawns a tokio task that subscribes to market + order filters and
    /// processes incoming events. Returns a `JoinHandle` the caller can
    /// abort to stop the loop.
    pub async fn start(&self) -> Result<tokio::task::JoinHandle<()>, String> {
        // Add relays and connect
        for url in &self.config.relays {
            self.client
                .add_relay(url.as_str())
                .await
                .map_err(|e| format!("failed to add relay {url}: {e}"))?;
        }
        self.client
            .connect_with_timeout(Duration::from_secs(5))
            .await;

        let client = self.client.clone();
        let store = self.store.clone();
        let tx = self.tx.clone();

        let handle = tokio::spawn(async move {
            run_subscription_loop(client, store, tx).await;
        });

        Ok(handle)
    }

    /// One-shot: fetch all markets from relays, optionally persist, and return.
    pub async fn fetch_markets(&self) -> Result<Vec<DiscoveredMarket>, String> {
        self.ensure_connected().await?;

        let filter = build_contract_filter();
        let events = self
            .client
            .fetch_events(vec![filter], self.config.fetch_timeout)
            .await
            .map_err(|e| format!("failed to fetch events: {e}"))?;

        let mut markets = Vec::new();
        for event in events.iter() {
            match parse_announcement_event(event) {
                Ok(mut market) => {
                    market.nostr_event_json = serde_json::to_string(event).ok();
                    self.persist_market(&market);
                    markets.push(market);
                }
                Err(e) => {
                    log::warn!("skipping unparseable announcement {}: {e}", event.id);
                }
            }
        }

        Ok(markets)
    }

    /// One-shot: fetch orders from relays, optionally for a specific market.
    pub async fn fetch_orders(
        &self,
        market_id_hex: Option<&str>,
    ) -> Result<Vec<DiscoveredOrder>, String> {
        self.ensure_connected().await?;

        let filter = build_order_filter(market_id_hex);
        let events = self
            .client
            .fetch_events(vec![filter], self.config.fetch_timeout)
            .await
            .map_err(|e| format!("failed to fetch order events: {e}"))?;

        let mut orders = Vec::new();
        for event in events.iter() {
            match parse_order_event(event) {
                Ok(mut order) => {
                    order.nostr_event_json = serde_json::to_string(event).ok();
                    self.persist_order(&order);
                    orders.push(order);
                }
                Err(e) => {
                    log::warn!("skipping unparseable order event {}: {e}", event.id);
                }
            }
        }

        Ok(orders)
    }

    /// One-shot: fetch attestation for a specific market.
    pub async fn fetch_attestation(
        &self,
        market_id_hex: &str,
    ) -> Result<Option<AttestationContent>, String> {
        self.ensure_connected().await?;

        let filter = build_attestation_filter(market_id_hex);
        let events = self
            .client
            .fetch_events(vec![filter], self.config.fetch_timeout)
            .await
            .map_err(|e| format!("failed to fetch attestation events: {e}"))?;

        match events.iter().next() {
            Some(event) => {
                let content = parse_attestation_event(event)?;
                Ok(Some(content))
            }
            None => Ok(None),
        }
    }

    /// Publish a market announcement to relays.
    pub async fn announce_market(
        &self,
        announcement: &ContractAnnouncement,
    ) -> Result<EventId, String> {
        self.ensure_connected().await?;

        let event = build_announcement_event(&self.keys, announcement)?;
        let output = self
            .client
            .send_event(event)
            .await
            .map_err(|e| format!("failed to send event: {e}"))?;
        Ok(*output.id())
    }

    /// Publish a limit order announcement to relays.
    pub async fn announce_order(
        &self,
        announcement: &OrderAnnouncement,
    ) -> Result<EventId, String> {
        self.ensure_connected().await?;

        let event = build_order_event(&self.keys, announcement)?;
        let output = self
            .client
            .send_event(event)
            .await
            .map_err(|e| format!("failed to send order event: {e}"))?;
        Ok(*output.id())
    }

    /// Sign and publish an oracle attestation.
    pub async fn publish_attestation(
        &self,
        market_id: &MarketId,
        announcement_event_id: &str,
        outcome_yes: bool,
    ) -> Result<AttestationResult, String> {
        self.ensure_connected().await?;

        let market_id_hex = hex::encode(market_id.as_bytes());

        let (sig_bytes, msg_bytes) = sign_attestation(&self.keys, market_id, outcome_yes)?;
        let sig_hex = hex::encode(sig_bytes);
        let msg_hex = hex::encode(msg_bytes);

        let event = build_attestation_event(
            &self.keys,
            &market_id_hex,
            announcement_event_id,
            outcome_yes,
            &sig_hex,
            &msg_hex,
        )?;

        let output = self
            .client
            .send_event(event)
            .await
            .map_err(|e| format!("failed to send attestation event: {e}"))?;

        Ok(AttestationResult {
            market_id: market_id_hex,
            outcome_yes,
            signature_hex: sig_hex,
            nostr_event_id: output.id().to_hex(),
        })
    }

    /// Publish an AMM pool announcement to relays.
    pub async fn announce_pool(&self, announcement: &PoolAnnouncement) -> Result<EventId, String> {
        self.ensure_connected().await?;

        let event = build_pool_event(&self.keys, announcement)?;
        let output = self
            .client
            .send_event(event)
            .await
            .map_err(|e| format!("failed to send pool event: {e}"))?;
        Ok(*output.id())
    }

    /// One-shot: fetch AMM pools from relays, optionally for a specific market.
    pub async fn fetch_pools(
        &self,
        market_id_hex: Option<&str>,
    ) -> Result<Vec<DiscoveredPool>, String> {
        self.ensure_connected().await?;

        let filter = build_pool_filter(market_id_hex);
        let events = self
            .client
            .fetch_events(vec![filter], self.config.fetch_timeout)
            .await
            .map_err(|e| format!("failed to fetch pool events: {e}"))?;

        let mut pools = Vec::new();
        for event in events.iter() {
            match parse_pool_event(event) {
                Ok(mut pool) => {
                    pool.nostr_event_json = serde_json::to_string(event).ok();
                    self.persist_pool(&pool);
                    pools.push(pool);
                }
                Err(e) => {
                    log::warn!("skipping unparseable pool event {}: {e}", event.id);
                }
            }
        }

        Ok(pools)
    }

    /// Load all stored Nostr event JSON strings for reconciliation.
    ///
    /// Returns an empty vec if no store is configured.
    pub fn load_all_nostr_events(&self) -> Result<Vec<String>, String> {
        match self.store {
            Some(ref store) => {
                let mut s = store
                    .lock()
                    .map_err(|_| "store lock poisoned".to_string())?;
                s.get_all_nostr_events()
            }
            None => Ok(vec![]),
        }
    }

    /// Reconcile stored discovery events with relays.
    ///
    /// Reads all persisted Nostr event JSON from the store, deserializes each
    /// into a signed `Event`, and re-sends it to connected relays. NIP-33
    /// replaceable events make this idempotent — relays deduplicate by
    /// (pubkey, kind, d-tag).
    pub async fn reconcile(&self) -> Result<ReconciliationStats, String> {
        self.ensure_connected().await?;
        let event_jsons = self.load_all_nostr_events()?;
        Ok(send_reconciliation_events(&self.client, &event_jsons).await)
    }

    /// Get a reference to the underlying Nostr client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get a reference to the keys.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    // --- internal helpers ---

    async fn ensure_connected(&self) -> Result<(), String> {
        if self.client.relays().await.is_empty() {
            for url in &self.config.relays {
                self.client
                    .add_relay(url.as_str())
                    .await
                    .map_err(|e| format!("failed to add relay {url}: {e}"))?;
            }
            self.client
                .connect_with_timeout(Duration::from_secs(5))
                .await;
        }
        Ok(())
    }

    fn persist_market(&self, market: &DiscoveredMarket) {
        persist_market_to_store(&self.store, market);
    }

    fn persist_order(&self, order: &DiscoveredOrder) {
        persist_order_to_store(&self.store, order);
    }

    fn persist_pool(&self, pool: &DiscoveredPool) {
        persist_pool_to_store(&self.store, pool);
    }
}

/// Send pre-loaded Nostr event JSON strings to relays.
///
/// Each JSON string is deserialized into a signed `Event` and sent.
/// Malformed JSON is skipped (counted in `events_skipped`).
/// This is a standalone function so callers can drop borrows (e.g. a node
/// mutex guard) before performing network I/O.
pub async fn send_reconciliation_events(
    client: &Client,
    event_jsons: &[String],
) -> ReconciliationStats {
    let mut stats = ReconciliationStats::default();
    for json in event_jsons {
        let event: Event = match serde_json::from_str(json) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("reconcile: skipping unparseable event: {e}");
                stats.events_skipped += 1;
                continue;
            }
        };
        match client.send_event(event).await {
            Ok(_) => stats.events_sent += 1,
            Err(e) => {
                log::warn!("reconcile: failed to send event: {e}");
                stats.events_failed += 1;
            }
        }
    }
    stats
}

/// Convert a DiscoveredMarket into ContractParams for store ingestion.
pub fn discovered_market_to_contract_params(
    m: &DiscoveredMarket,
) -> Result<crate::prediction_market::params::PredictionMarketParams, String> {
    Ok(crate::prediction_market::params::PredictionMarketParams {
        oracle_public_key: hex_to_bytes32(&m.oracle_pubkey, "oracle_pubkey")?,
        collateral_asset_id: hex_to_bytes32(&m.collateral_asset_id, "collateral_asset_id")?,
        yes_token_asset: hex_to_bytes32(&m.yes_asset_id, "yes_asset_id")?,
        no_token_asset: hex_to_bytes32(&m.no_asset_id, "no_asset_id")?,
        yes_reissuance_token: hex_to_bytes32(&m.yes_reissuance_token, "yes_reissuance_token")?,
        no_reissuance_token: hex_to_bytes32(&m.no_reissuance_token, "no_reissuance_token")?,
        collateral_per_token: m.cpt_sats,
        expiry_time: m.expiry_height,
    })
}

/// Convert a DiscoveredOrder into MakerOrderParams for store ingestion.
fn discovered_order_to_maker_params(
    o: &DiscoveredOrder,
) -> Result<crate::maker_order::params::MakerOrderParams, String> {
    let direction = match o.direction.as_str() {
        "sell-base" => crate::maker_order::params::OrderDirection::SellBase,
        "sell-quote" => crate::maker_order::params::OrderDirection::SellQuote,
        other => return Err(format!("unknown direction: {other}")),
    };

    let maker_pubkey = hex_to_bytes32(&o.maker_base_pubkey, "maker_base_pubkey")?;

    Ok(crate::maker_order::params::MakerOrderParams {
        base_asset_id: hex_to_bytes32(&o.base_asset_id, "base_asset_id")?,
        quote_asset_id: hex_to_bytes32(&o.quote_asset_id, "quote_asset_id")?,
        price: o.price,
        min_fill_lots: o.min_fill_lots,
        min_remainder_lots: o.min_remainder_lots,
        direction,
        maker_receive_spk_hash: hex_to_bytes32(
            &o.maker_receive_spk_hash,
            "maker_receive_spk_hash",
        )?,
        cosigner_pubkey: hex_to_bytes32(&o.cosigner_pubkey, "cosigner_pubkey")?,
        maker_pubkey,
    })
}

/// Convert a DiscoveredPool into AmmPoolParams for store ingestion.
pub fn discovered_pool_to_amm_params(
    p: &DiscoveredPool,
) -> Result<crate::amm_pool::params::AmmPoolParams, String> {
    Ok(crate::amm_pool::params::AmmPoolParams {
        yes_asset_id: hex_to_bytes32(&p.yes_asset_id, "yes_asset_id")?,
        no_asset_id: hex_to_bytes32(&p.no_asset_id, "no_asset_id")?,
        lbtc_asset_id: hex_to_bytes32(&p.lbtc_asset_id, "lbtc_asset_id")?,
        lp_asset_id: hex_to_bytes32(&p.lp_asset_id, "lp_asset_id")?,
        lp_reissuance_token_id: hex_to_bytes32(
            &p.lp_reissuance_token_id,
            "lp_reissuance_token_id",
        )?,
        fee_bps: p.fee_bps,
        cosigner_pubkey: hex_to_bytes32(&p.cosigner_pubkey, "cosigner_pubkey")?,
    })
}

/// Background subscription loop that listens for Nostr events and dispatches them.
async fn run_subscription_loop<S: DiscoveryStore>(
    client: Client,
    store: Option<Arc<Mutex<S>>>,
    tx: broadcast::Sender<DiscoveryEvent>,
) {
    // Set up the notification receiver BEFORE subscribing so we don't miss events
    let mut notifications = client.notifications();

    let market_filter = build_contract_filter();
    let order_filter = build_order_filter(None);
    let attestation_filter = build_attestation_subscription_filter();
    let pool_filter = build_pool_filter(None);

    if let Err(e) = client
        .subscribe(
            vec![market_filter, order_filter, attestation_filter, pool_filter],
            None,
        )
        .await
    {
        log::error!("failed to subscribe: {e}");
        return;
    }

    while let Ok(notification) = notifications.recv().await {
        if let RelayPoolNotification::Event { event, .. } = notification {
            let hashtags: Vec<String> = event
                .tags
                .iter()
                .filter_map(|t| {
                    let tag_vec = t.as_slice();
                    if tag_vec.len() >= 2 && tag_vec[0] == "t" {
                        Some(tag_vec[1].to_string())
                    } else {
                        None
                    }
                })
                .collect();

            if hashtags.iter().any(|t| t == CONTRACT_TAG) {
                if let Ok(mut market) = parse_announcement_event(&event) {
                    market.nostr_event_json = serde_json::to_string(&*event).ok();
                    persist_market_to_store(&store, &market);
                    let _ = tx.send(DiscoveryEvent::MarketDiscovered(market));
                }
            } else if hashtags.iter().any(|t| t == ORDER_TAG) {
                if let Ok(mut order) = parse_order_event(&event) {
                    order.nostr_event_json = serde_json::to_string(&*event).ok();
                    persist_order_to_store(&store, &order);
                    let _ = tx.send(DiscoveryEvent::OrderDiscovered(order));
                }
            } else if hashtags.iter().any(|t| t == ATTESTATION_TAG) {
                if let Ok(attestation) = parse_attestation_event(&event) {
                    let _ = tx.send(DiscoveryEvent::AttestationDiscovered(attestation));
                }
            } else if hashtags.iter().any(|t| t == POOL_TAG)
                && let Ok(mut pool) = parse_pool_event(&event)
            {
                pool.nostr_event_json = serde_json::to_string(&*event).ok();
                persist_pool_to_store(&store, &pool);
                let _ = tx.send(DiscoveryEvent::PoolDiscovered(pool));
            }
        }
    }
}

pub(crate) fn persist_market_to_store<S: DiscoveryStore>(
    store: &Option<Arc<Mutex<S>>>,
    market: &DiscoveredMarket,
) {
    let Some(store) = store else { return };
    let Ok(params) = discovered_market_to_contract_params(market) else {
        return;
    };
    let meta = DiscoveredMarketMetadata {
        question: Some(market.question.clone()),
        description: Some(market.description.clone()),
        category: Some(market.category.clone()),
        resolution_source: Some(market.resolution_source.clone()),
        creator_pubkey: hex::decode(&market.creator_pubkey).ok(),
        creation_txid: market.creation_txid.clone(),
        nevent: Some(market.nevent.clone()),
        nostr_event_id: Some(market.id.clone()),
        nostr_event_json: market.nostr_event_json.clone(),
    };
    if let Ok(mut s) = store.lock() {
        let _ = s.ingest_market(&params, Some(&meta));
    }
}

pub(crate) fn persist_pool_to_store<S: DiscoveryStore>(
    store: &Option<Arc<Mutex<S>>>,
    pool: &DiscoveredPool,
) {
    let Some(store) = store else { return };
    let Ok(params) = discovered_pool_to_amm_params(pool) else {
        return;
    };
    let market_id: Option<[u8; 32]> = hex::decode(&pool.market_id)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
    let creation_txid: Option<[u8; 32]> = pool
        .creation_txid
        .as_deref()
        .and_then(|s| hex::decode(s).ok())
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
    if let Ok(mut s) = store.lock() {
        let _ = s.ingest_amm_pool(
            &params,
            pool.issued_lp,
            Some(&pool.id),
            pool.nostr_event_json.as_deref(),
            market_id.as_ref(),
            creation_txid.as_ref(),
        );
    }
}

fn persist_order_to_store<S: DiscoveryStore>(
    store: &Option<Arc<Mutex<S>>>,
    order: &DiscoveredOrder,
) {
    let Some(store) = store else { return };
    let Ok(params) = discovered_order_to_maker_params(order) else {
        return;
    };
    let maker_pubkey = hex::decode(&order.maker_base_pubkey)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
    let nonce = hex::decode(&order.order_nonce)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
    if let Ok(mut s) = store.lock() {
        let _ = s.ingest_maker_order(
            &params,
            maker_pubkey.as_ref(),
            nonce.as_ref(),
            Some(&order.id),
            order.nostr_event_json.as_deref(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconciliation_stats_default_is_all_zero() {
        let stats = ReconciliationStats::default();
        assert_eq!(stats.events_sent, 0);
        assert_eq!(stats.events_failed, 0);
        assert_eq!(stats.events_skipped, 0);
    }

    #[test]
    fn reconciliation_stats_display() {
        let stats = ReconciliationStats {
            events_sent: 5,
            events_failed: 1,
            events_skipped: 2,
        };
        assert_eq!(stats.to_string(), "sent=5, failed=1, skipped=2");
    }

    #[test]
    fn reconciliation_stats_serde_roundtrip() {
        let stats = ReconciliationStats {
            events_sent: 3,
            events_failed: 0,
            events_skipped: 1,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["events_sent"], 3);
        assert_eq!(parsed["events_failed"], 0);
        assert_eq!(parsed["events_skipped"], 1);
    }

    #[test]
    fn noop_store_load_all_nostr_events_returns_empty() {
        let keys = Keys::generate();
        let config = DiscoveryConfig::default();
        let (service, _rx) = DiscoveryService::new(keys, config);
        let events = service.load_all_nostr_events().unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn reconcile_with_noop_store_returns_default_stats() {
        let keys = Keys::generate();
        let config = DiscoveryConfig {
            relays: vec![], // no relays — ensure_connected is a no-op
            ..Default::default()
        };
        let (service, _rx) = DiscoveryService::new(keys, config);
        let stats = service.reconcile().await.unwrap();
        assert_eq!(stats.events_sent, 0);
        assert_eq!(stats.events_failed, 0);
        assert_eq!(stats.events_skipped, 0);
    }

    #[tokio::test]
    async fn send_reconciliation_events_skips_bad_json() {
        let client = Client::default();
        let events = vec![
            "not valid json".to_string(),
            "{\"also\": \"not a nostr event\"}".to_string(),
        ];
        let stats = send_reconciliation_events(&client, &events).await;
        assert_eq!(stats.events_skipped, 2);
        assert_eq!(stats.events_sent, 0);
        assert_eq!(stats.events_failed, 0);
    }

    #[tokio::test]
    async fn send_reconciliation_events_empty_is_noop() {
        let client = Client::default();
        let stats = send_reconciliation_events(&client, &[]).await;
        assert_eq!(stats.events_sent, 0);
        assert_eq!(stats.events_failed, 0);
        assert_eq!(stats.events_skipped, 0);
    }
}
