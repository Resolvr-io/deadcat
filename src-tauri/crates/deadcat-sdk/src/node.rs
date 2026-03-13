//! `DeadcatNode` — unified SDK coordinator.
//!
//! Owns the wallet (SDK), Nostr discovery service, and shared store behind a
//! single `&self` API. Combined methods (on-chain + Nostr) use
//! `tokio::task::spawn_blocking` internally so callers stay in async land.

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
use crate::discovery::market::DiscoveredMarket;
use crate::discovery::service::{
    DiscoveryService, NoopStore, persist_canonical_lmsr_state_to_store, persist_market_to_store,
};
use crate::discovery::store_trait::DiscoveryStore;
use crate::discovery::{
    AttestationContent, AttestationResult, DEFAULT_RELAYS, DiscoveredOrder, OrderAnnouncement,
    bytes_to_hex,
};
use crate::error::{Error, NodeError};
use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::network::Network;
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::params::{MarketId, PredictionMarketParams};
use crate::prediction_market::state::MarketState;
use crate::sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, DeadcatSdk, FillOrderResult,
    IssuanceResult, RedemptionResult, ResolutionResult,
};
use crate::trade::types::{TradeAmount, TradeDirection, TradeQuote, TradeResult, TradeSide};

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

    // ── Internal: store persistence helpers ──────────────────────────────

    fn persist_market(&self, market: &DiscoveredMarket) {
        persist_market_to_store(&self.store, market);
    }

    // ── Combined on-chain + Nostr operations ────────────────────────────

    /// Create a market on-chain and announce it via Nostr.
    ///
    /// Returns the parsed `DiscoveredMarket` (with Nostr event data) and the
    /// on-chain creation `Txid`.
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
    ) -> Result<(DiscoveredMarket, Txid), NodeError> {
        // 1. On-chain via spawn_blocking
        let (txid, params) = self
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

        // 2. Build and publish Nostr announcement
        let announcement = ContractAnnouncement {
            version: CONTRACT_ANNOUNCEMENT_VERSION,
            contract_params: params,
            metadata,
            creation_txid: Some(txid.to_string()),
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
            question: announcement.metadata.question,
            category: announcement.metadata.category,
            description: announcement.metadata.description,
            resolution_source: announcement.metadata.resolution_source,
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
            creation_txid: announcement.creation_txid,
            state: 0,
            nostr_event_json: None,
            yes_price_bps: None,
            no_price_bps: None,
        };

        // 4. Persist to store
        self.persist_market(&market);

        Ok((market, txid))
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
        creation_txid: Txid,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<IssuanceResult, NodeError> {
        self.with_sdk(move |sdk| sdk.issue_tokens(&params, &creation_txid, pairs, fee_amount))
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
        self.with_sdk(move |sdk| {
            sdk.cancel_limit_order(&params, maker_pubkey, order_index, fee_amount)
        })
        .await
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
        outcome_yes: bool,
        oracle_sig: [u8; 64],
        fee_amount: u64,
    ) -> Result<ResolutionResult, NodeError> {
        self.with_sdk(move |sdk| sdk.resolve_market(&params, outcome_yes, oracle_sig, fee_amount))
            .await
    }

    // ── Redemption ──────────────────────────────────────────────────────

    /// Redeem winning tokens after oracle resolution.
    pub async fn redeem_tokens(
        &self,
        params: PredictionMarketParams,
        tokens: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult, NodeError> {
        self.with_sdk(move |sdk| sdk.redeem_tokens(&params, tokens, fee_amount))
            .await
    }

    /// Redeem tokens after market expiry (no oracle resolution).
    pub async fn redeem_expired(
        &self,
        params: PredictionMarketParams,
        token_asset: [u8; 32],
        tokens: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult, NodeError> {
        self.with_sdk(move |sdk| sdk.redeem_expired(&params, token_asset, tokens, fee_amount))
            .await
    }

    /// Cancel token pairs by burning equal YES and NO tokens.
    pub async fn cancel_tokens(
        &self,
        params: PredictionMarketParams,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<CancellationResult, NodeError> {
        self.with_sdk(move |sdk| sdk.cancel_tokens(&params, pairs, fee_amount))
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

    /// Sync the wallet with the Electrum backend.
    pub async fn sync_wallet(&self) -> Result<(), NodeError> {
        self.with_sdk(|sdk| sdk.sync()).await
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

    /// Scan covenant addresses to determine the current on-chain state of a market.
    pub async fn market_state(
        &self,
        params: PredictionMarketParams,
    ) -> Result<MarketState, NodeError> {
        self.with_sdk(move |sdk| {
            let contract = CompiledPredictionMarket::new(params)?;
            let (state, _utxos) = sdk.scan_market_state(&contract)?;
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

    /// Validate a market was created at its Dormant covenant address.
    pub async fn validate_market_creation(
        &self,
        params: PredictionMarketParams,
        creation_txid: String,
    ) -> Result<bool, NodeError> {
        let txid: Txid = creation_txid
            .parse()
            .map_err(|e| NodeError::Discovery(format!("bad txid: {e}")))?;

        self.with_sdk(move |sdk| sdk.validate_market_creation(&params, &txid))
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
