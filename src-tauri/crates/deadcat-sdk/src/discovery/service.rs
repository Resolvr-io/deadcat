use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    DiscoveredMarket, ParsedDiscoveredMarketAnnouncement, build_announcement_event,
    build_contract_filter, parse_announcement_event_with_ingest,
};
use super::pool::{
    DiscoveredPool, PoolAnnouncement, build_pool_event, build_pool_filter, parse_pool_event,
};
use super::store_trait::{
    DiscoveryStore, LmsrPoolIngestInput, LmsrPoolStateSource, LmsrPoolStateUpdateInput,
    PredictionMarketCandidateIngestInput,
};
use super::{
    ATTESTATION_TAG, CONTRACT_TAG, DiscoveredOrder, ORDER_TAG, OrderAnnouncement, POOL_TAG,
    build_order_event, build_order_filter, parse_order_event,
};

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
    fn ingest_prediction_market_candidate(
        &mut self,
        _input: &PredictionMarketCandidateIngestInput,
        _seen_at_unix: u64,
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

    fn ingest_lmsr_pool(&mut self, _input: &LmsrPoolIngestInput) -> Result<(), String> {
        Ok(())
    }

    fn upsert_lmsr_pool_state(&mut self, _input: &LmsrPoolStateUpdateInput) -> Result<(), String> {
        Ok(())
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
        self.client.connect().await;

        let client = self.client.clone();
        let store = self.store.clone();
        let tx = self.tx.clone();
        let network_tag = self.config.network_tag.clone();

        let handle = tokio::spawn(async move {
            run_subscription_loop(client, store, tx, network_tag).await;
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
            match parse_announcement_event_with_ingest(event, &self.config.network_tag) {
                Ok(parsed) => {
                    self.persist_market(&parsed);
                    markets.push(parsed.market);
                }
                Err(e) => {
                    if e.contains("unsupported contract announcement version") {
                        log::warn!("skipping market announcement {}: {e}", event.id);
                    } else {
                        log::warn!("skipping unparseable announcement {}: {e}", event.id);
                    }
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
            match parse_order_event(event, &self.config.network_tag) {
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
                let content = parse_attestation_event(event, &self.config.network_tag)?;
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

        let event = build_announcement_event(&self.keys, announcement, &self.config.network_tag)?;
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

        let event = build_order_event(&self.keys, announcement, &self.config.network_tag)?;
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
            &self.config.network_tag,
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

    /// Publish a pool announcement to relays.
    pub async fn announce_pool(&self, announcement: &PoolAnnouncement) -> Result<EventId, String> {
        self.ensure_connected().await?;

        let event = build_pool_event(&self.keys, announcement, &self.config.network_tag)?;
        let output = self
            .client
            .send_event(event)
            .await
            .map_err(|e| format!("failed to send pool event: {e}"))?;
        Ok(*output.id())
    }

    /// One-shot: fetch pools from relays, optionally for a specific market.
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
            match parse_pool_event(event, &self.config.network_tag) {
                Ok(mut pool) => {
                    pool.nostr_event_json = serde_json::to_string(event).ok();
                    pools.push(pool);
                }
                Err(e) => {
                    log::warn!("skipping unparseable pool event {}: {e}", event.id);
                }
            }
        }

        let pools = dedup_latest_pools_by_id(pools);
        for pool in &pools {
            self.persist_pool(pool);
        }

        Ok(pools)
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
            self.client.connect().await;
        }
        Ok(())
    }

    fn persist_market(&self, parsed: &ParsedDiscoveredMarketAnnouncement) {
        persist_market_to_store(&self.store, parsed);
    }

    fn persist_order(&self, order: &DiscoveredOrder) {
        persist_order_to_store(&self.store, order);
    }

    fn persist_pool(&self, pool: &DiscoveredPool) {
        persist_pool_to_store(&self.store, pool, &self.config.network_tag);
    }
}

/// Convert a DiscoveredMarket into ContractParams for store ingestion.
pub fn discovered_market_to_contract_params(
    m: &DiscoveredMarket,
) -> Result<crate::prediction_market::params::PredictionMarketParams, String> {
    let decode32 = |hex_str: &str, name: &str| -> Result<[u8; 32], String> {
        let bytes = hex::decode(hex_str).map_err(|e| format!("{name}: hex decode: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| format!("{name}: expected 32 bytes"))
    };

    Ok(crate::prediction_market::params::PredictionMarketParams {
        oracle_public_key: decode32(&m.oracle_pubkey, "oracle_pubkey")?,
        collateral_asset_id: decode32(&m.collateral_asset_id, "collateral_asset_id")?,
        yes_token_asset: decode32(&m.yes_asset_id, "yes_asset_id")?,
        no_token_asset: decode32(&m.no_asset_id, "no_asset_id")?,
        yes_reissuance_token: decode32(&m.yes_reissuance_token, "yes_reissuance_token")?,
        no_reissuance_token: decode32(&m.no_reissuance_token, "no_reissuance_token")?,
        collateral_per_token: m.cpt_sats,
        expiry_time: m.expiry_height,
    })
}

/// Convert a DiscoveredOrder into MakerOrderParams for store ingestion.
fn discovered_order_to_maker_params(
    o: &DiscoveredOrder,
) -> Result<crate::maker_order::params::MakerOrderParams, String> {
    let decode32 = |hex_str: &str, name: &str| -> Result<[u8; 32], String> {
        let bytes = hex::decode(hex_str).map_err(|e| format!("{name}: hex decode: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| format!("{name}: expected 32 bytes"))
    };

    let direction = match o.direction.as_str() {
        "sell-base" => crate::maker_order::params::OrderDirection::SellBase,
        "sell-quote" => crate::maker_order::params::OrderDirection::SellQuote,
        other => return Err(format!("unknown direction: {other}")),
    };

    let maker_pubkey = decode32(&o.maker_base_pubkey, "maker_base_pubkey")?;

    Ok(crate::maker_order::params::MakerOrderParams {
        base_asset_id: decode32(&o.base_asset_id, "base_asset_id")?,
        quote_asset_id: decode32(&o.quote_asset_id, "quote_asset_id")?,
        price: o.price,
        min_fill_lots: o.min_fill_lots,
        min_remainder_lots: o.min_remainder_lots,
        direction,
        maker_receive_spk_hash: decode32(&o.maker_receive_spk_hash, "maker_receive_spk_hash")?,
        cosigner_pubkey: decode32(&o.cosigner_pubkey, "cosigner_pubkey")?,
        maker_pubkey,
    })
}

/// Background subscription loop that listens for Nostr events and dispatches them.
async fn run_subscription_loop<S: DiscoveryStore>(
    client: Client,
    store: Option<Arc<Mutex<S>>>,
    tx: broadcast::Sender<DiscoveryEvent>,
    network_tag: String,
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
                match parse_announcement_event_with_ingest(&event, &network_tag) {
                    Ok(parsed) => {
                        persist_market_to_store(&store, &parsed);
                        let _ = tx.send(DiscoveryEvent::MarketDiscovered(parsed.market));
                    }
                    Err(e) => {
                        log::warn!("skipping unparseable market announcement {}: {e}", event.id);
                    }
                }
            } else if hashtags.iter().any(|t| t == ORDER_TAG) {
                if let Ok(mut order) = parse_order_event(&event, &network_tag) {
                    order.nostr_event_json = serde_json::to_string(&*event).ok();
                    persist_order_to_store(&store, &order);
                    let _ = tx.send(DiscoveryEvent::OrderDiscovered(order));
                }
            } else if hashtags.iter().any(|t| t == ATTESTATION_TAG) {
                if let Ok(attestation) = parse_attestation_event(&event, &network_tag) {
                    let _ = tx.send(DiscoveryEvent::AttestationDiscovered(attestation));
                }
            } else if hashtags.iter().any(|t| t == POOL_TAG)
                && let Ok(mut pool) = parse_pool_event(&event, &network_tag)
            {
                pool.nostr_event_json = serde_json::to_string(&*event).ok();
                persist_pool_to_store(&store, &pool, &network_tag);
                let _ = tx.send(DiscoveryEvent::PoolDiscovered(pool));
            }
        }
    }
}

pub(crate) fn persist_market_to_store<S: DiscoveryStore>(
    store: &Option<Arc<Mutex<S>>>,
    parsed: &ParsedDiscoveredMarketAnnouncement,
) {
    let Some(store) = store else { return };
    let seen_at_unix = Timestamp::now().as_u64();
    if let Ok(mut s) = store.lock() {
        let _ = s.ingest_prediction_market_candidate(&parsed.ingest, seen_at_unix);
    }
}

pub(crate) fn persist_pool_to_store<S: DiscoveryStore>(
    store: &Option<Arc<Mutex<S>>>,
    pool: &DiscoveredPool,
    network_tag: &str,
) {
    let Some(store) = store else { return };
    let Ok(parsed) = crate::trade::convert::parse_discovered_lmsr_pool(pool, network_tag) else {
        return;
    };

    let input = LmsrPoolIngestInput {
        pool_id: parsed.lmsr_pool_id.clone(),
        market_id: pool.market_id.clone(),
        yes_asset_id: parsed.params.yes_asset_id,
        no_asset_id: parsed.params.no_asset_id,
        collateral_asset_id: parsed.params.collateral_asset_id,
        fee_bps: parsed.params.fee_bps,
        cosigner_pubkey: parsed.params.cosigner_pubkey,
        lmsr_table_root: parsed.params.lmsr_table_root,
        table_depth: parsed.params.table_depth,
        q_step_lots: parsed.params.q_step_lots,
        s_bias: parsed.params.s_bias,
        s_max_index: parsed.params.s_max_index,
        half_payout_sats: parsed.params.half_payout_sats,
        creation_txid: pool.creation_txid.clone(),
        witness_schema_version: parsed.witness_schema_version.clone(),
        current_s_index: parsed.current_s_index,
        reserve_outpoints: [
            pool.initial_reserve_outpoints[0].clone(),
            pool.initial_reserve_outpoints[1].clone(),
            pool.initial_reserve_outpoints[2].clone(),
        ],
        reserve_yes: pool.reserves.r_yes,
        reserve_no: pool.reserves.r_no,
        reserve_collateral: pool.reserves.r_lbtc,
        state_source: LmsrPoolStateSource::Announcement,
        last_transition_txid: None,
        nostr_event_id: Some(pool.id.clone()),
        nostr_event_json: pool.nostr_event_json.clone(),
    };

    if let Ok(mut s) = store.lock() {
        let _ = s.ingest_lmsr_pool(&input);
    }
}

pub(crate) fn persist_canonical_lmsr_state_to_store<S: DiscoveryStore>(
    store: &Option<Arc<Mutex<S>>>,
    input: &LmsrPoolStateUpdateInput,
) {
    let Some(store) = store else { return };
    if let Ok(mut s) = store.lock() {
        let _ = s.upsert_lmsr_pool_state(input);
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

fn dedup_latest_pools_by_id(pools: Vec<DiscoveredPool>) -> Vec<DiscoveredPool> {
    let mut dedup: HashMap<String, DiscoveredPool> = HashMap::new();
    for pool in pools {
        match dedup.get_mut(&pool.lmsr_pool_id) {
            None => {
                dedup.insert(pool.lmsr_pool_id.clone(), pool);
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
    let mut pools: Vec<_> = dedup.into_values().collect();
    pools.sort_by(|a, b| a.lmsr_pool_id.cmp(&b.lmsr_pool_id));
    pools
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::PoolReserves;
    use crate::testing::test_market_announcement;

    fn hex32(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    fn sample_pool(event_id: &str, pool_byte: u8, created_at: u64) -> DiscoveredPool {
        let creation_txid = hex32(0xaa);
        DiscoveredPool {
            id: event_id.to_string(),
            market_id: "mkt".to_string(),
            pool_id: hex32(pool_byte),
            yes_asset_id: hex32(0x01),
            no_asset_id: hex32(0x02),
            lbtc_asset_id: hex32(0x03),
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: hex32(0x04),
            reserves: PoolReserves {
                r_yes: 10,
                r_no: 20,
                r_lbtc: 30,
            },
            creator_pubkey: hex32(0x05),
            created_at,
            creation_txid: creation_txid.clone(),
            lmsr_pool_id: hex32(pool_byte),
            lmsr_table_root: hex32(0x06),
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            current_s_index: 4,
            initial_reserve_outpoints: vec![
                format!("{creation_txid}:0"),
                format!("{creation_txid}:1"),
                format!("{creation_txid}:2"),
            ],
            witness_schema_version: super::super::pool::LMSR_WITNESS_SCHEMA_V2.to_string(),
            table_manifest_hash: None,
            lmsr_table_values: None,
            nostr_event_json: None,
        }
    }

    #[test]
    fn dedup_pools_keeps_latest_event_per_pool_id() {
        let older = sample_pool("evt-older", 0x11, 100);
        let newer = sample_pool("evt-newer", 0x11, 200);
        let deduped = dedup_latest_pools_by_id(vec![older, newer.clone()]);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].id, newer.id);
    }

    #[test]
    fn dedup_pools_keeps_distinct_pool_ids() {
        let a = sample_pool("evt-a", 0x11, 100);
        let b = sample_pool("evt-b", 0x22, 200);
        let deduped = dedup_latest_pools_by_id(vec![a, b]);
        assert_eq!(deduped.len(), 2);
    }

    #[derive(Default)]
    struct SeenAtStore {
        seen_at_unix: Vec<u64>,
    }

    impl DiscoveryStore for SeenAtStore {
        fn ingest_prediction_market_candidate(
            &mut self,
            _input: &PredictionMarketCandidateIngestInput,
            seen_at_unix: u64,
        ) -> std::result::Result<(), String> {
            self.seen_at_unix.push(seen_at_unix);
            Ok(())
        }

        fn ingest_maker_order(
            &mut self,
            _params: &crate::maker_order::params::MakerOrderParams,
            _maker_pubkey: Option<&[u8; 32]>,
            _nonce: Option<&[u8; 32]>,
            _nostr_event_id: Option<&str>,
            _nostr_event_json: Option<&str>,
        ) -> std::result::Result<(), String> {
            Ok(())
        }

        fn ingest_lmsr_pool(
            &mut self,
            _input: &LmsrPoolIngestInput,
        ) -> std::result::Result<(), String> {
            Ok(())
        }

        fn upsert_lmsr_pool_state(
            &mut self,
            _input: &LmsrPoolStateUpdateInput,
        ) -> std::result::Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn persist_market_uses_local_ingest_time() {
        let keys = Keys::generate();
        let (announcement, _params) = test_market_announcement([0xaa; 32], 0x19);
        let event = build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap();
        let mut parsed = parse_announcement_event_with_ingest(&event, "liquid-testnet").unwrap();
        parsed.market.created_at = 1;

        let store = Arc::new(Mutex::new(SeenAtStore::default()));
        let before = Timestamp::now().as_u64();
        persist_market_to_store(&Some(store.clone()), &parsed);
        let after = Timestamp::now().as_u64();

        let seen_at = store.lock().unwrap().seen_at_unix[0];
        assert_ne!(seen_at, parsed.market.created_at);
        assert!(seen_at >= before);
        assert!(seen_at <= after);
    }
}
