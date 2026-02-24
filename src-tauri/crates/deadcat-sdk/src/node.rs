//! `DeadcatNode` — unified SDK coordinator.
//!
//! Owns the wallet (SDK), Nostr discovery service, and shared store behind a
//! single `&self` API. Combined methods (on-chain + Nostr) use
//! `tokio::task::spawn_blocking` internally so callers stay in async land.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lwk_wollet::elements::{AssetId, Txid};
use lwk_wollet::{AddressResult, WalletTxOut};
use nostr_sdk::prelude::*;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::announcement::{ContractAnnouncement, ContractMetadata};
use crate::discovery::config::DiscoveryConfig;
use crate::discovery::events::DiscoveryEvent;
use crate::discovery::market::DiscoveredMarket;
use crate::discovery::service::{DiscoveryService, NoopStore, persist_market_to_store};
use crate::discovery::store_trait::DiscoveryStore;
use crate::discovery::pool::{DiscoveredPool, PoolAnnouncement};
use crate::discovery::{
    AttestationContent, AttestationResult, DiscoveredOrder, OrderAnnouncement,
    DEFAULT_RELAYS, bytes_to_hex,
};
use crate::error::{Error, NodeError};
use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::network::Network;
use crate::params::{ContractParams, MarketId};
use crate::sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, DeadcatSdk, FillOrderResult,
    IssuanceResult, RedemptionResult, ResolutionResult,
};

// ── Struct ──────────────────────────────────────────────────────────────────

/// Unified coordinator that owns the SDK wallet, Nostr discovery service,
/// and (optionally) a shared persistence store.
///
/// All public methods take `&self`; interior mutability is provided by
/// `Arc<Mutex<…>>`. SDK (blocking) calls are dispatched via
/// `tokio::task::spawn_blocking`.
pub struct DeadcatNode<S: DiscoveryStore = NoopStore> {
    sdk: Arc<Mutex<Option<DeadcatSdk>>>,
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
        config: DiscoveryConfig,
    ) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        let (discovery, rx) = DiscoveryService::new(keys.clone(), config);
        (
            Self {
                sdk: Arc::new(Mutex::new(None)),
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
        config: DiscoveryConfig,
    ) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        let (discovery, rx) =
            DiscoveryService::with_store(keys.clone(), store.clone(), config);
        (
            Self {
                sdk: Arc::new(Mutex::new(None)),
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
        *guard = Some(sdk);
        Ok(())
    }

    /// Lock the wallet, dropping the SDK instance.
    pub fn lock_wallet(&self) {
        if let Ok(mut guard) = self.sdk.lock() {
            *guard = None;
        }
    }

    /// Returns `true` if the wallet is currently unlocked.
    pub fn is_wallet_unlocked(&self) -> bool {
        self.sdk
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
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
        tokio::task::spawn_blocking(move || {
            let mut guard = sdk.lock().map_err(|_| NodeError::MutexPoisoned)?;
            let sdk = guard.as_mut().ok_or(NodeError::WalletLocked)?;
            f(sdk).map_err(NodeError::Sdk)
        })
        .await
        .map_err(|e| NodeError::Task(e.to_string()))?
    }

    // ── Internal: store persistence helpers ──────────────────────────────

    fn persist_market(&self, market: &DiscoveredMarket) {
        persist_market_to_store(&self.store, market);
    }

    fn persist_pool(&self, pool: &DiscoveredPool) {
        use crate::discovery::service::persist_pool_to_store;
        persist_pool_to_store(&self.store, pool);
    }

    /// Persist updated pool state (issued_lp + reserves) to the store.
    fn persist_pool_state(
        &self,
        params: &crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
    ) {
        if let Some(ref store) = self.store {
            if let Ok(mut guard) = store.lock() {
                let pool_id = crate::amm_pool::params::PoolId::from_params(params);
                if let Err(e) = guard.update_pool_state(&pool_id, params, issued_lp, r_yes, r_no, r_lbtc) {
                    log::warn!("failed to persist pool state: {e}");
                }
            }
        }
    }

    /// Publish an updated NIP-33 replaceable pool announcement after LP state changes.
    async fn update_pool_announcement(
        &self,
        pool_params: &crate::amm_pool::params::AmmPoolParams,
        new_issued_lp: u64,
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
        market_id: &str,
    ) {
        let cmr_hex = match crate::amm_pool::contract::CompiledAmmPool::new(*pool_params) {
            Ok(c) => hex::encode(c.cmr().as_ref()),
            Err(_) => return,
        };

        let announcement = PoolAnnouncement {
            version: 1,
            params: *pool_params,
            market_id: market_id.to_string(),
            issued_lp: new_issued_lp,
            covenant_cmr: cmr_hex,
            // Outpoints intentionally empty — see design doc §D5.
            // Consumers must chain-scan the covenant address to find current UTXOs.
            outpoints: Vec::new(),
            reserves: crate::amm_pool::math::PoolReserves {
                r_yes,
                r_no,
                r_lbtc,
            },
        };

        if let Err(e) = self.discovery.announce_pool(&announcement).await {
            log::warn!("failed to update pool Nostr announcement: {e}");
        }
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
            version: 1,
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
            starting_yes_price: announcement.metadata.starting_yes_price,
            creator_pubkey: self.keys.public_key().to_hex(),
            created_at: nostr_sdk::Timestamp::now().as_u64(),
            creation_txid: announcement.creation_txid,
            state: 0,
            nostr_event_json: None,
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
        params: ContractParams,
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
        params: ContractParams,
        outcome_yes: bool,
        oracle_sig: [u8; 64],
        fee_amount: u64,
    ) -> Result<ResolutionResult, NodeError> {
        self.with_sdk(move |sdk| {
            sdk.resolve_market(&params, outcome_yes, oracle_sig, fee_amount)
        })
        .await
    }

    // ── Redemption ──────────────────────────────────────────────────────

    /// Redeem winning tokens after oracle resolution.
    pub async fn redeem_tokens(
        &self,
        params: ContractParams,
        tokens: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult, NodeError> {
        self.with_sdk(move |sdk| sdk.redeem_tokens(&params, tokens, fee_amount))
            .await
    }

    /// Redeem tokens after market expiry (no oracle resolution).
    pub async fn redeem_expired(
        &self,
        params: ContractParams,
        token_asset: [u8; 32],
        tokens: u64,
        fee_amount: u64,
    ) -> Result<RedemptionResult, NodeError> {
        self.with_sdk(move |sdk| {
            sdk.redeem_expired(&params, token_asset, tokens, fee_amount)
        })
        .await
    }

    /// Cancel token pairs by burning equal YES and NO tokens.
    pub async fn cancel_tokens(
        &self,
        params: ContractParams,
        pairs: u64,
        fee_amount: u64,
    ) -> Result<CancellationResult, NodeError> {
        self.with_sdk(move |sdk| sdk.cancel_tokens(&params, pairs, fee_amount))
            .await
    }

    // ── AMM Pool ──────────────────────────────────────────────────────

    /// Create a new AMM pool: on-chain TX + Nostr announcement + store persistence.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_pool(
        &self,
        pool_params: crate::amm_pool::params::AmmPoolParams,
        initial_r_yes: u64,
        initial_r_no: u64,
        initial_r_lbtc: u64,
        initial_issued_lp: u64,
        fee_amount: u64,
        market_id: String,
    ) -> Result<(DiscoveredPool, Txid), NodeError> {
        // 1. On-chain
        let result = self
            .with_sdk(move |sdk| {
                sdk.create_amm_pool(
                    &pool_params,
                    initial_r_yes,
                    initial_r_no,
                    initial_r_lbtc,
                    initial_issued_lp,
                    fee_amount,
                )
            })
            .await?;

        let txid = result.txid;

        // 2. Build and publish Nostr announcement
        let compiled = crate::amm_pool::contract::CompiledAmmPool::new(result.pool_params)
            .map_err(|e| NodeError::Sdk(e))?;
        let announcement = PoolAnnouncement {
            version: 1,
            params: result.pool_params,
            market_id: market_id.clone(),
            issued_lp: result.issued_lp,
            covenant_cmr: hex::encode(compiled.cmr().as_ref()),
            outpoints: Vec::new(), // Outpoints not yet confirmed
            reserves: crate::amm_pool::math::PoolReserves {
                r_yes: initial_r_yes,
                r_no: initial_r_no,
                r_lbtc: initial_r_lbtc,
            },
        };

        let event_id = self
            .discovery
            .announce_pool(&announcement)
            .await
            .map_err(NodeError::Discovery)?;

        // 3. Build DiscoveredPool for return + store persistence
        let pool_id = crate::amm_pool::params::PoolId::from_params(&result.pool_params);
        let pool = DiscoveredPool {
            id: event_id.to_hex(),
            market_id,
            pool_id: pool_id.to_hex(),
            yes_asset_id: bytes_to_hex(&result.pool_params.yes_asset_id),
            no_asset_id: bytes_to_hex(&result.pool_params.no_asset_id),
            lbtc_asset_id: bytes_to_hex(&result.pool_params.lbtc_asset_id),
            lp_asset_id: bytes_to_hex(&result.pool_params.lp_asset_id),
            lp_reissuance_token_id: bytes_to_hex(&result.pool_params.lp_reissuance_token_id),
            fee_bps: result.pool_params.fee_bps,
            cosigner_pubkey: bytes_to_hex(&result.pool_params.cosigner_pubkey),
            issued_lp: result.issued_lp,
            covenant_cmr: announcement.covenant_cmr,
            outpoints: Vec::new(),
            reserves: announcement.reserves,
            creator_pubkey: self.keys.public_key().to_hex(),
            created_at: nostr_sdk::Timestamp::now().as_u64(),
            nostr_event_json: None,
        };

        // 4. Persist to store
        self.persist_pool(&pool);

        Ok((pool, txid))
    }

    /// Execute a swap against an AMM pool: on-chain TX + update Nostr + persist.
    pub async fn pool_swap(
        &self,
        pool_params: crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        swap_pair: crate::amm_pool::math::SwapPair,
        delta_in: u64,
        sell_a: bool,
        fee_amount: u64,
        market_id: String,
    ) -> Result<crate::sdk::PoolSwapResult, NodeError> {
        let result = self
            .with_sdk(move |sdk| {
                sdk.pool_swap(&pool_params, issued_lp, swap_pair, delta_in, sell_a, fee_amount)
            })
            .await?;

        // Use SDK-computed reserves (derived from on-chain state) for Nostr + store
        let nr = &result.new_reserves;

        // Swap doesn't change issued_lp, but reserves change — update Nostr
        self.update_pool_announcement(
            &pool_params,
            issued_lp,
            nr.r_yes,
            nr.r_no,
            nr.r_lbtc,
            &market_id,
        )
        .await;

        // Persist updated reserves to store
        self.persist_pool_state(&pool_params, issued_lp, nr.r_yes, nr.r_no, nr.r_lbtc);

        Ok(result)
    }

    /// Deposit liquidity into an AMM pool: on-chain TX + update Nostr + persist.
    #[allow(clippy::too_many_arguments)]
    pub async fn pool_deposit(
        &self,
        pool_params: crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        new_r_yes: u64,
        new_r_no: u64,
        new_r_lbtc: u64,
        lp_mint_amount: u64,
        fee_amount: u64,
        market_id: String,
    ) -> Result<crate::sdk::PoolLpResult, NodeError> {
        let result = self
            .with_sdk(move |sdk| {
                sdk.pool_lp_deposit(
                    &pool_params,
                    issued_lp,
                    new_r_yes,
                    new_r_no,
                    new_r_lbtc,
                    lp_mint_amount,
                    fee_amount,
                )
            })
            .await?;

        // Use SDK-computed reserves for Nostr + store
        let nr = &result.new_reserves;
        self.update_pool_announcement(
            &pool_params,
            result.new_issued_lp,
            nr.r_yes,
            nr.r_no,
            nr.r_lbtc,
            &market_id,
        )
        .await;

        self.persist_pool_state(&pool_params, result.new_issued_lp, nr.r_yes, nr.r_no, nr.r_lbtc);

        Ok(result)
    }

    /// Withdraw liquidity from an AMM pool: on-chain TX + update Nostr + persist.
    pub async fn pool_withdraw(
        &self,
        pool_params: crate::amm_pool::params::AmmPoolParams,
        issued_lp: u64,
        lp_burn: u64,
        fee_amount: u64,
        market_id: String,
    ) -> Result<crate::sdk::PoolLpResult, NodeError> {
        let result = self
            .with_sdk(move |sdk| {
                sdk.pool_lp_withdraw(&pool_params, issued_lp, lp_burn, fee_amount)
            })
            .await?;

        // Use SDK-computed reserves (derived from on-chain state)
        let nr = &result.new_reserves;
        self.update_pool_announcement(
            &pool_params,
            result.new_issued_lp,
            nr.r_yes,
            nr.r_no,
            nr.r_lbtc,
            &market_id,
        )
        .await;

        self.persist_pool_state(&pool_params, result.new_issued_lp, nr.r_yes, nr.r_no, nr.r_lbtc);

        Ok(result)
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

    /// Fetch AMM pools from Nostr relays, optionally for a specific market.
    pub async fn fetch_pools(
        &self,
        market_id: Option<&str>,
    ) -> Result<Vec<crate::discovery::pool::DiscoveredPool>, NodeError> {
        self.discovery
            .fetch_pools(market_id)
            .await
            .map_err(NodeError::Discovery)
    }

    /// Publish an AMM pool announcement to Nostr relays.
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
        self.discovery
            .start()
            .await
            .map_err(NodeError::Discovery)
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

    /// Get the wallet balance by asset.
    pub async fn balance(&self) -> Result<HashMap<AssetId, u64>, NodeError> {
        self.with_sdk(|sdk| sdk.balance()).await
    }

    /// Get a wallet address.
    pub async fn address(&self, index: Option<u32>) -> Result<AddressResult, NodeError> {
        self.with_sdk(move |sdk| sdk.address(index)).await
    }

    /// Get unspent wallet outputs.
    pub async fn utxos(&self) -> Result<Vec<WalletTxOut>, NodeError> {
        self.with_sdk(|sdk| sdk.utxos()).await
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

    // ── Accessors ───────────────────────────────────────────────────────

    /// The Nostr identity keys.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// The network this node is configured for.
    pub fn network(&self) -> Network {
        self.network
    }

    /// A reference to the underlying discovery service.
    pub fn discovery(&self) -> &DiscoveryService<S> {
        &self.discovery
    }
}
