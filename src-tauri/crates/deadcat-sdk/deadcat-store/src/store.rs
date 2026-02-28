use diesel::prelude::*;
use diesel::sql_types::Integer;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use deadcat_sdk::{
    CompiledMakerOrder, CompiledPredictionMarket, DiscoveredMarketMetadata, MakerOrderParams,
    MarketId, MarketState, OrderDirection, PredictionMarketParams, UnblindedUtxo,
};

use crate::conversions::{
    direction_to_i32, new_amm_pool_row, new_maker_order_row, new_market_row, new_utxo_row,
    vec_to_array32,
};
use crate::error::StoreError;
use crate::models::{
    AmmPoolRow, MakerOrderRow, MarketRow, NewPoolStateSnapshotRow, NewUtxoRow,
    PoolStateSnapshotRow, UtxoRow,
};
use crate::schema::{amm_pools, maker_orders, markets, pool_state_snapshots, sync_state, utxos};
use crate::sync::{ChainSource, ChainUtxo, MarketStateChange, OrderStatusChange, SyncReport};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// SQL expression for SQLite's `datetime('now')`.
const DATETIME_NOW: &str = "datetime('now')";

// --- Public types ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending = 0,
    Active = 1,
    PartiallyFilled = 2,
    FullyFilled = 3,
    Cancelled = 4,
}

impl OrderStatus {
    pub fn from_i32(v: i32) -> std::result::Result<Self, StoreError> {
        match v {
            0 => Ok(OrderStatus::Pending),
            1 => Ok(OrderStatus::Active),
            2 => Ok(OrderStatus::PartiallyFilled),
            3 => Ok(OrderStatus::FullyFilled),
            4 => Ok(OrderStatus::Cancelled),
            other => Err(StoreError::InvalidData(format!(
                "invalid order status: {other}"
            ))),
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Issuance data needed by InitialIssuanceParams / SubsequentIssuanceParams.
/// Stored as market metadata; auto-extracted from chain during sync.
#[derive(Debug, Clone, Copy)]
pub struct IssuanceData {
    pub yes_entropy: [u8; 32],
    pub no_entropy: [u8; 32],
    pub yes_blinding_nonce: [u8; 32],
    pub no_blinding_nonce: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct MarketInfo {
    pub market_id: MarketId,
    pub params: PredictionMarketParams,
    pub state: MarketState,
    pub cmr: [u8; 32],
    pub issuance: Option<IssuanceData>,
    pub created_at: String,
    pub updated_at: String,
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub creator_pubkey: Option<Vec<u8>>,
    pub creation_txid: Option<String>,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MakerOrderInfo {
    pub id: i32,
    pub params: MakerOrderParams,
    pub status: OrderStatus,
    pub cmr: [u8; 32],
    pub maker_base_pubkey: Option<[u8; 32]>,
    pub order_nonce: Option<[u8; 32]>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolStatus {
    Active = 0,
    Inactive = 1,
    Closed = 2,
}

impl PoolStatus {
    pub fn from_i32(v: i32) -> std::result::Result<Self, StoreError> {
        match v {
            0 => Ok(PoolStatus::Active),
            1 => Ok(PoolStatus::Inactive),
            2 => Ok(PoolStatus::Closed),
            other => Err(StoreError::InvalidData(format!(
                "invalid pool status: {other}"
            ))),
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone)]
pub struct AmmPoolInfo {
    pub pool_id: deadcat_sdk::PoolId,
    pub params: deadcat_sdk::AmmPoolParams,
    pub status: PoolStatus,
    pub cmr: [u8; 32],
    pub issued_lp: u64,
    pub covenant_spk: Vec<u8>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub market_id: Option<[u8; 32]>,
    pub creation_txid: Option<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub struct PoolStateSnapshotInfo {
    pub id: i32,
    pub pool_id: [u8; 32],
    pub txid: [u8; 32],
    pub r_yes: u64,
    pub r_no: u64,
    pub r_lbtc: u64,
    pub issued_lp: u64,
    pub block_height: Option<i32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct MarketFilter {
    pub oracle_public_key: Option<[u8; 32]>,
    pub collateral_asset_id: Option<[u8; 32]>,
    pub current_state: Option<MarketState>,
    pub expiry_before: Option<u32>,
    pub expiry_after: Option<u32>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct OrderFilter {
    pub base_asset_id: Option<[u8; 32]>,
    pub quote_asset_id: Option<[u8; 32]>,
    pub direction: Option<OrderDirection>,
    pub order_status: Option<OrderStatus>,
    pub maker_base_pubkey: Option<[u8; 32]>,
    pub min_price: Option<u64>,
    pub max_price: Option<u64>,
    pub limit: Option<i64>,
}

// --- DeadcatStore ---

/// Persistent storage for deadcat prediction markets, maker orders, and UTXOs.
///
/// All methods take `&mut self` because Diesel's `SqliteConnection` requires
/// `&mut` for all operations, including reads.
pub struct DeadcatStore {
    conn: SqliteConnection,
}

impl DeadcatStore {
    /// Open (or create) a store at the given file path. Runs migrations automatically.
    pub fn open(path: &str) -> crate::Result<Self> {
        let mut conn = SqliteConnection::establish(path)?;
        diesel::sql_query("PRAGMA foreign_keys = ON").execute(&mut conn)?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        Ok(DeadcatStore { conn })
    }

    /// Open an in-memory store for tests.
    pub fn open_in_memory() -> crate::Result<Self> {
        let mut conn = SqliteConnection::establish(":memory:")?;
        diesel::sql_query("PRAGMA foreign_keys = ON").execute(&mut conn)?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| StoreError::Migration(e.to_string()))?;
        Ok(DeadcatStore { conn })
    }

    // ==================== Ingest ====================

    /// Ingest a market from its contract parameters. Compiles the contract to derive
    /// the CMR and 4 state scriptPubKeys. Returns the MarketId.
    /// If the market already exists, this is a no-op returning the existing ID.
    pub fn ingest_market(
        &mut self,
        params: &PredictionMarketParams,
        metadata: Option<&DiscoveredMarketMetadata>,
    ) -> crate::Result<MarketId> {
        let mid = params.market_id();
        let mid_bytes = mid.as_bytes().to_vec();

        let exists: bool = diesel::select(diesel::dsl::exists(
            markets::table.filter(markets::market_id.eq(&mid_bytes)),
        ))
        .get_result(&mut self.conn)?;

        if exists {
            // Update nostr_event_json if it was previously missing
            if let Some(meta) = metadata
                && let Some(ref json) = meta.nostr_event_json
            {
                diesel::update(markets::table.filter(markets::market_id.eq(&mid_bytes)))
                    .set(markets::nostr_event_json.eq(json))
                    .execute(&mut self.conn)?;
            }
            return Ok(mid);
        }

        let compiled = CompiledPredictionMarket::new(*params)?;
        let row = new_market_row(params, &compiled, metadata);

        diesel::insert_into(markets::table)
            .values(&row)
            .execute(&mut self.conn)?;

        Ok(mid)
    }

    /// Ingest a maker order. Compiles the covenant to derive the CMR and optionally
    /// the covenant scriptPubKey (if `maker_base_pubkey` is provided).
    /// Returns the row ID. If a matching (cmr, maker_base_pubkey) already exists,
    /// returns the existing ID.
    pub fn ingest_maker_order(
        &mut self,
        params: &MakerOrderParams,
        maker_pubkey: Option<&[u8; 32]>,
        order_nonce: Option<&[u8; 32]>,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
    ) -> crate::Result<i32> {
        let compiled = CompiledMakerOrder::new(*params)?;
        let cmr_bytes = compiled.cmr().as_ref().to_vec();
        let pk_bytes = maker_pubkey.map(|pk| pk.to_vec());

        let existing: Option<MakerOrderRow> = if let Some(ref pk) = pk_bytes {
            maker_orders::table
                .filter(
                    maker_orders::cmr
                        .eq(&cmr_bytes)
                        .and(maker_orders::maker_base_pubkey.eq(pk)),
                )
                .first(&mut self.conn)
                .optional()?
        } else {
            maker_orders::table
                .filter(
                    maker_orders::cmr
                        .eq(&cmr_bytes)
                        .and(maker_orders::maker_base_pubkey.is_null()),
                )
                .first(&mut self.conn)
                .optional()?
        };

        if let Some(row) = existing {
            return Ok(row.id);
        }

        let row = new_maker_order_row(
            params,
            &compiled,
            maker_pubkey,
            order_nonce,
            nostr_event_id,
            nostr_event_json,
        );

        diesel::insert_into(maker_orders::table)
            .values(&row)
            .execute(&mut self.conn)?;

        // Use SQLite's last_insert_rowid() for correctness
        let row_id: i32 = diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
            .get_result(&mut self.conn)?;

        Ok(row_id)
    }

    // ==================== Market Queries ====================

    pub fn get_market(&mut self, mid: &MarketId) -> crate::Result<Option<MarketInfo>> {
        let row: Option<MarketRow> = markets::table
            .filter(markets::market_id.eq(mid.as_bytes().to_vec()))
            .first(&mut self.conn)
            .optional()?;

        row.as_ref().map(MarketInfo::try_from).transpose()
    }

    pub fn list_markets(&mut self, filter: &MarketFilter) -> crate::Result<Vec<MarketInfo>> {
        let mut query = markets::table.into_boxed();

        if let Some(ref opk) = filter.oracle_public_key {
            query = query.filter(markets::oracle_public_key.eq(opk.to_vec()));
        }
        if let Some(ref caid) = filter.collateral_asset_id {
            query = query.filter(markets::collateral_asset_id.eq(caid.to_vec()));
        }
        if let Some(state) = filter.current_state {
            query = query.filter(markets::current_state.eq(state.as_u64() as i32));
        }
        if let Some(before) = filter.expiry_before {
            query = query.filter(markets::expiry_time.lt(before as i32));
        }
        if let Some(after) = filter.expiry_after {
            query = query.filter(markets::expiry_time.gt(after as i32));
        }
        if let Some(lim) = filter.limit {
            query = query.limit(lim);
        }

        let rows: Vec<MarketRow> = query.load(&mut self.conn)?;
        rows.iter().map(MarketInfo::try_from).collect()
    }

    // ==================== Maker Order Queries ====================

    pub fn get_maker_order(&mut self, order_id: i32) -> crate::Result<Option<MakerOrderInfo>> {
        let row: Option<MakerOrderRow> = maker_orders::table
            .filter(maker_orders::id.eq(order_id))
            .first(&mut self.conn)
            .optional()?;

        row.as_ref().map(MakerOrderInfo::try_from).transpose()
    }

    pub fn list_maker_orders(
        &mut self,
        filter: &OrderFilter,
    ) -> crate::Result<Vec<MakerOrderInfo>> {
        let mut query = maker_orders::table.into_boxed();

        if let Some(ref ba) = filter.base_asset_id {
            query = query.filter(maker_orders::base_asset_id.eq(ba.to_vec()));
        }
        if let Some(ref qa) = filter.quote_asset_id {
            query = query.filter(maker_orders::quote_asset_id.eq(qa.to_vec()));
        }
        if let Some(dir) = filter.direction {
            query = query.filter(maker_orders::direction.eq(direction_to_i32(dir)));
        }
        if let Some(status) = filter.order_status {
            query = query.filter(maker_orders::order_status.eq(status.as_i32()));
        }
        if let Some(ref pk) = filter.maker_base_pubkey {
            query = query.filter(maker_orders::maker_base_pubkey.eq(pk.to_vec()));
        }
        if let Some(min_p) = filter.min_price {
            query = query.filter(maker_orders::price.ge(min_p as i64));
        }
        if let Some(max_p) = filter.max_price {
            query = query.filter(maker_orders::price.le(max_p as i64));
        }
        if let Some(lim) = filter.limit {
            query = query.limit(lim);
        }

        let rows: Vec<MakerOrderRow> = query.load(&mut self.conn)?;
        rows.iter().map(MakerOrderInfo::try_from).collect()
    }

    // ==================== UTXO Queries ====================

    pub fn get_market_utxos(
        &mut self,
        mid: &MarketId,
        state: Option<MarketState>,
    ) -> crate::Result<Vec<UnblindedUtxo>> {
        let mid_bytes = mid.as_bytes().to_vec();
        let mut query = utxos::table
            .filter(utxos::market_id.eq(&mid_bytes).and(utxos::spent.eq(0)))
            .into_boxed();

        if let Some(s) = state {
            query = query.filter(utxos::market_state.eq(s.as_u64() as i32));
        }

        let rows: Vec<UtxoRow> = query.load(&mut self.conn)?;
        rows.iter().map(UnblindedUtxo::try_from).collect()
    }

    pub fn get_order_utxos(&mut self, order_id: i32) -> crate::Result<Vec<UnblindedUtxo>> {
        let rows: Vec<UtxoRow> = utxos::table
            .filter(utxos::maker_order_id.eq(order_id).and(utxos::spent.eq(0)))
            .load(&mut self.conn)?;

        rows.iter().map(UnblindedUtxo::try_from).collect()
    }

    // ==================== Manual UTXO Management ====================

    pub fn add_market_utxo(
        &mut self,
        mid: &MarketId,
        state: MarketState,
        utxo: &UnblindedUtxo,
        height: Option<u32>,
    ) -> crate::Result<()> {
        let row = new_utxo_row(utxo, Some(mid), Some(state), None, height);

        diesel::insert_or_ignore_into(utxos::table)
            .values(&row)
            .execute(&mut self.conn)?;

        Ok(())
    }

    pub fn add_order_utxo(
        &mut self,
        order_id: i32,
        utxo: &UnblindedUtxo,
        height: Option<u32>,
    ) -> crate::Result<()> {
        let row = new_utxo_row(utxo, None, None, Some(order_id), height);

        diesel::insert_or_ignore_into(utxos::table)
            .values(&row)
            .execute(&mut self.conn)?;

        Ok(())
    }

    pub fn mark_spent(
        &mut self,
        txid_bytes: &[u8; 32],
        vout_val: u32,
        spending_txid_bytes: &[u8; 32],
        spent_height: Option<u32>,
    ) -> crate::Result<()> {
        diesel::update(
            utxos::table.filter(
                utxos::txid
                    .eq(txid_bytes.to_vec())
                    .and(utxos::vout.eq(vout_val as i32)),
            ),
        )
        .set((
            utxos::spent.eq(1),
            utxos::spending_txid.eq(spending_txid_bytes.to_vec()),
            utxos::spent_block_height.eq(spent_height.map(|h| h as i32)),
        ))
        .execute(&mut self.conn)?;

        Ok(())
    }

    // ==================== State Updates ====================

    pub fn update_market_state(
        &mut self,
        mid: &MarketId,
        new_state: MarketState,
    ) -> crate::Result<()> {
        diesel::update(markets::table.filter(markets::market_id.eq(mid.as_bytes().to_vec())))
            .set((
                markets::current_state.eq(new_state.as_u64() as i32),
                markets::updated_at.eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            ))
            .execute(&mut self.conn)?;

        Ok(())
    }

    pub fn update_order_status(&mut self, order_id: i32, status: OrderStatus) -> crate::Result<()> {
        diesel::update(maker_orders::table.filter(maker_orders::id.eq(order_id)))
            .set((
                maker_orders::order_status.eq(status.as_i32()),
                maker_orders::updated_at
                    .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            ))
            .execute(&mut self.conn)?;

        Ok(())
    }

    // ==================== Issuance Data ====================

    /// Manually set issuance entropy for a market (fallback if not yet synced from chain).
    pub fn set_market_issuance_data(
        &mut self,
        mid: &MarketId,
        data: &IssuanceData,
    ) -> crate::Result<()> {
        diesel::update(markets::table.filter(markets::market_id.eq(mid.as_bytes().to_vec())))
            .set((
                markets::yes_issuance_entropy.eq(data.yes_entropy.to_vec()),
                markets::no_issuance_entropy.eq(data.no_entropy.to_vec()),
                markets::yes_issuance_blinding_nonce.eq(data.yes_blinding_nonce.to_vec()),
                markets::no_issuance_blinding_nonce.eq(data.no_blinding_nonce.to_vec()),
                markets::updated_at.eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            ))
            .execute(&mut self.conn)?;

        Ok(())
    }

    // ==================== Maker Receive SPKs ====================

    /// Returns all maker receive SPKs (for LWK registration).
    pub fn maker_receive_script_pubkeys(&mut self) -> crate::Result<Vec<Vec<u8>>> {
        let spks: Vec<Vec<u8>> = maker_orders::table
            .select(maker_orders::maker_receive_spk)
            .filter(maker_orders::maker_receive_spk.is_not_null())
            .load::<Option<Vec<u8>>>(&mut self.conn)?
            .into_iter()
            .flatten()
            .collect();

        Ok(spks)
    }

    // ==================== Chain Sync ====================

    /// Collect all watched scriptPubKeys: 4 per market, 1 per maker order with known pubkey.
    pub fn watched_script_pubkeys(&mut self) -> crate::Result<Vec<Vec<u8>>> {
        let mut spks = Vec::new();

        #[allow(clippy::type_complexity)]
        let market_rows: Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> = markets::table
            .select((
                markets::dormant_spk,
                markets::unresolved_spk,
                markets::resolved_yes_spk,
                markets::resolved_no_spk,
            ))
            .load(&mut self.conn)?;

        for (d, u, ry, rn) in market_rows {
            spks.push(d);
            spks.push(u);
            spks.push(ry);
            spks.push(rn);
        }

        let order_spks: Vec<Vec<u8>> = maker_orders::table
            .select(maker_orders::covenant_spk)
            .filter(maker_orders::covenant_spk.is_not_null())
            .load::<Option<Vec<u8>>>(&mut self.conn)?
            .into_iter()
            .flatten()
            .collect();

        spks.extend(order_spks);

        Ok(spks)
    }

    pub fn last_synced_height(&mut self) -> crate::Result<u32> {
        let height: i32 = sync_state::table
            .select(sync_state::last_block_height)
            .first(&mut self.conn)?;

        Ok(height as u32)
    }

    /// Run the sync algorithm against a chain source.
    ///
    /// 1. For each watched SPK, discover new UTXOs via `chain.list_unspent`
    /// 2. For each existing unspent UTXO, check if spent via `chain.is_spent`
    /// 3. Derive market states (highest-state address with unspent UTXOs)
    /// 4. Derive order statuses from UTXO presence/absence
    /// 5. Update sync_state with block height
    pub fn sync<C: ChainSource>(&mut self, chain: &C) -> crate::Result<SyncReport> {
        self.conn.transaction(|conn| {
            let mut report = SyncReport::default();

            let best_height = chain
                .best_block_height()
                .map_err(|e| StoreError::Sync(e.to_string()))?;
            report.block_height = best_height;

            sync_market_utxos(conn, chain, &mut report)?;
            sync_order_utxos(conn, chain, &mut report)?;
            sync_spent_utxos(conn, chain, &mut report)?;
            derive_market_states(conn, &mut report)?;
            derive_order_statuses(conn, &mut report)?;

            diesel::update(sync_state::table.filter(sync_state::id.eq(1)))
                .set(sync_state::last_block_height.eq(best_height as i32))
                .execute(conn)?;

            Ok(report)
        })
    }

    // ==================== AMM Pool Ingest ====================

    /// Ingest an AMM pool. Compiles the covenant to derive the CMR and covenant
    /// scriptPubKey. Returns the PoolId. If the pool already exists, updates
    /// the issued_lp and covenant_spk.
    #[allow(clippy::too_many_arguments)]
    pub fn ingest_amm_pool(
        &mut self,
        params: &deadcat_sdk::AmmPoolParams,
        issued_lp: u64,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
        market_id: Option<&[u8; 32]>,
        creation_txid: Option<&[u8; 32]>,
    ) -> crate::Result<deadcat_sdk::PoolId> {
        let pool_id = deadcat_sdk::PoolId::from_params(params);
        let pool_id_bytes = pool_id.0.to_vec();

        let exists: bool = diesel::select(diesel::dsl::exists(
            amm_pools::table.filter(amm_pools::pool_id.eq(&pool_id_bytes)),
        ))
        .get_result(&mut self.conn)?;

        if exists {
            // Update issued_lp, covenant_spk, and Nostr metadata for existing pool.
            // Reserves are tracked exclusively in pool_state_snapshots.
            let compiled = deadcat_sdk::CompiledAmmPool::new(*params)?;
            let update =
                diesel::update(amm_pools::table.filter(amm_pools::pool_id.eq(&pool_id_bytes)));
            let base_set = (
                amm_pools::issued_lp.eq(issued_lp as i64),
                amm_pools::covenant_spk.eq(compiled.script_pubkey(issued_lp).as_bytes().to_vec()),
                amm_pools::updated_at.eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            );
            update.set(base_set).execute(&mut self.conn)?;
            // Update Nostr metadata if provided
            if let Some(eid) = nostr_event_id {
                diesel::update(amm_pools::table.filter(amm_pools::pool_id.eq(&pool_id_bytes)))
                    .set(amm_pools::nostr_event_id.eq(Some(eid.to_string())))
                    .execute(&mut self.conn)?;
            }
            if let Some(ejson) = nostr_event_json {
                diesel::update(amm_pools::table.filter(amm_pools::pool_id.eq(&pool_id_bytes)))
                    .set(amm_pools::nostr_event_json.eq(Some(ejson.to_string())))
                    .execute(&mut self.conn)?;
            }
            // Update market_id and creation_txid if provided (fill in if not yet set)
            if let Some(mid) = market_id {
                diesel::update(
                    amm_pools::table
                        .filter(amm_pools::pool_id.eq(&pool_id_bytes))
                        .filter(amm_pools::market_id.is_null()),
                )
                .set(amm_pools::market_id.eq(Some(mid.to_vec())))
                .execute(&mut self.conn)?;
            }
            if let Some(ctxid) = creation_txid {
                diesel::update(
                    amm_pools::table
                        .filter(amm_pools::pool_id.eq(&pool_id_bytes))
                        .filter(amm_pools::creation_txid.is_null()),
                )
                .set(amm_pools::creation_txid.eq(Some(ctxid.to_vec())))
                .execute(&mut self.conn)?;
            }
            return Ok(pool_id);
        }

        let compiled = deadcat_sdk::CompiledAmmPool::new(*params)?;
        let row = new_amm_pool_row(
            params,
            &compiled,
            issued_lp,
            nostr_event_id,
            nostr_event_json,
            market_id,
            creation_txid,
        );

        diesel::insert_into(amm_pools::table)
            .values(&row)
            .execute(&mut self.conn)?;

        Ok(pool_id)
    }

    // ==================== AMM Pool Queries ====================

    pub fn get_amm_pool(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
    ) -> crate::Result<Option<AmmPoolInfo>> {
        let row: Option<AmmPoolRow> = amm_pools::table
            .filter(amm_pools::pool_id.eq(pool_id.0.to_vec()))
            .first(&mut self.conn)
            .optional()?;

        row.as_ref().map(AmmPoolInfo::try_from).transpose()
    }

    pub fn list_amm_pools(&mut self) -> crate::Result<Vec<AmmPoolInfo>> {
        let rows: Vec<AmmPoolRow> = amm_pools::table
            .order(amm_pools::created_at.desc())
            .load(&mut self.conn)?;

        rows.iter().map(AmmPoolInfo::try_from).collect()
    }

    pub fn update_pool_state(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
        params: &deadcat_sdk::AmmPoolParams,
        issued_lp: u64,
    ) -> crate::Result<()> {
        // Recompile to derive the new covenant scriptPubKey for the updated issued_lp.
        // Reserves are tracked exclusively in pool_state_snapshots.
        let compiled = deadcat_sdk::CompiledAmmPool::new(*params)?;
        diesel::update(amm_pools::table.filter(amm_pools::pool_id.eq(pool_id.0.to_vec())))
            .set((
                amm_pools::issued_lp.eq(issued_lp as i64),
                amm_pools::covenant_spk.eq(compiled.script_pubkey(issued_lp).as_bytes().to_vec()),
                amm_pools::updated_at.eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            ))
            .execute(&mut self.conn)?;
        Ok(())
    }

    pub fn update_pool_status(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
        status: PoolStatus,
    ) -> crate::Result<()> {
        diesel::update(amm_pools::table.filter(amm_pools::pool_id.eq(pool_id.0.to_vec())))
            .set((
                amm_pools::pool_status.eq(status.as_i32()),
                amm_pools::updated_at.eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            ))
            .execute(&mut self.conn)?;
        Ok(())
    }

    // ==================== Pool State Snapshots ====================

    /// Insert a pool state snapshot. Idempotent: INSERT OR IGNORE keyed on (pool_id, txid).
    #[allow(clippy::too_many_arguments)]
    pub fn insert_pool_snapshot(
        &mut self,
        pool_id: &[u8; 32],
        txid: &[u8; 32],
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
        issued_lp: u64,
        block_height: Option<i32>,
    ) -> crate::Result<()> {
        let row = NewPoolStateSnapshotRow {
            pool_id: pool_id.to_vec(),
            txid: txid.to_vec(),
            r_yes: r_yes as i64,
            r_no: r_no as i64,
            r_lbtc: r_lbtc as i64,
            issued_lp: issued_lp as i64,
            block_height,
        };

        diesel::insert_or_ignore_into(pool_state_snapshots::table)
            .values(&row)
            .execute(&mut self.conn)?;

        Ok(())
    }

    /// Get the most recent pool state snapshot (by id, which is insertion order).
    pub fn get_latest_pool_snapshot(
        &mut self,
        pool_id: &[u8; 32],
    ) -> crate::Result<Option<PoolStateSnapshotInfo>> {
        let row: Option<PoolStateSnapshotRow> = pool_state_snapshots::table
            .filter(pool_state_snapshots::pool_id.eq(pool_id.to_vec()))
            .order(pool_state_snapshots::id.desc())
            .first(&mut self.conn)
            .optional()?;

        row.as_ref().map(snapshot_row_to_info).transpose()
    }

    /// Get all pool state snapshots in chronological order (by id ASC).
    pub fn get_pool_snapshots(
        &mut self,
        pool_id: &[u8; 32],
    ) -> crate::Result<Vec<PoolStateSnapshotInfo>> {
        let rows: Vec<PoolStateSnapshotRow> = pool_state_snapshots::table
            .filter(pool_state_snapshots::pool_id.eq(pool_id.to_vec()))
            .order(pool_state_snapshots::id.asc())
            .load(&mut self.conn)?;

        rows.iter().map(snapshot_row_to_info).collect()
    }

    /// Find the best pool for a given market (prefer Active status).
    pub fn get_pool_for_market(
        &mut self,
        market_id: &[u8; 32],
    ) -> crate::Result<Option<AmmPoolInfo>> {
        let row: Option<AmmPoolRow> = amm_pools::table
            .filter(amm_pools::market_id.eq(market_id.to_vec()))
            .order(
                amm_pools::pool_status
                    .eq(PoolStatus::Active.as_i32())
                    .desc(),
            )
            .first(&mut self.conn)
            .optional()?;

        row.as_ref().map(AmmPoolInfo::try_from).transpose()
    }

    /// Set a per-state txid on a market row (for chain validation tracking).
    pub fn update_market_state_txid(
        &mut self,
        mid: &MarketId,
        state: MarketState,
        txid: &str,
    ) -> crate::Result<()> {
        let mid_bytes = mid.as_bytes().to_vec();
        match state {
            MarketState::Dormant => {
                diesel::update(markets::table.filter(markets::market_id.eq(&mid_bytes)))
                    .set(markets::dormant_txid.eq(Some(txid)))
                    .execute(&mut self.conn)?;
            }
            MarketState::Unresolved => {
                diesel::update(markets::table.filter(markets::market_id.eq(&mid_bytes)))
                    .set(markets::unresolved_txid.eq(Some(txid)))
                    .execute(&mut self.conn)?;
            }
            MarketState::ResolvedYes => {
                diesel::update(markets::table.filter(markets::market_id.eq(&mid_bytes)))
                    .set(markets::resolved_yes_txid.eq(Some(txid)))
                    .execute(&mut self.conn)?;
            }
            MarketState::ResolvedNo => {
                diesel::update(markets::table.filter(markets::market_id.eq(&mid_bytes)))
                    .set(markets::resolved_no_txid.eq(Some(txid)))
                    .execute(&mut self.conn)?;
            }
        }
        Ok(())
    }
}

fn snapshot_row_to_info(row: &PoolStateSnapshotRow) -> crate::Result<PoolStateSnapshotInfo> {
    Ok(PoolStateSnapshotInfo {
        id: row.id,
        pool_id: vec_to_array32(&row.pool_id, "pool_id")?,
        txid: vec_to_array32(&row.txid, "txid")?,
        r_yes: row.r_yes as u64,
        r_no: row.r_no as u64,
        r_lbtc: row.r_lbtc as u64,
        issued_lp: row.issued_lp as u64,
        block_height: row.block_height,
        created_at: row.created_at.clone(),
    })
}

// ==================== DiscoveryStore trait impl ====================

impl deadcat_sdk::DiscoveryStore for DeadcatStore {
    fn ingest_market(
        &mut self,
        params: &PredictionMarketParams,
        meta: Option<&DiscoveredMarketMetadata>,
    ) -> Result<(), String> {
        self.ingest_market(params, meta)
            .map(|_| ())
            .map_err(|e| format!("{e}"))
    }

    fn ingest_maker_order(
        &mut self,
        params: &MakerOrderParams,
        maker_pubkey: Option<&[u8; 32]>,
        nonce: Option<&[u8; 32]>,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
    ) -> Result<(), String> {
        self.ingest_maker_order(
            params,
            maker_pubkey,
            nonce,
            nostr_event_id,
            nostr_event_json,
        )
        .map(|_| ())
        .map_err(|e| format!("{e}"))
    }

    fn ingest_amm_pool(
        &mut self,
        params: &deadcat_sdk::AmmPoolParams,
        issued_lp: u64,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
        market_id: Option<&[u8; 32]>,
        creation_txid: Option<&[u8; 32]>,
    ) -> Result<(), String> {
        self.ingest_amm_pool(
            params,
            issued_lp,
            nostr_event_id,
            nostr_event_json,
            market_id,
            creation_txid,
        )
        .map(|_| ())
        .map_err(|e| format!("{e}"))
    }

    fn update_pool_state(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
        params: &deadcat_sdk::AmmPoolParams,
        issued_lp: u64,
    ) -> Result<(), String> {
        self.update_pool_state(pool_id, params, issued_lp)
            .map_err(|e| format!("{e}"))
    }

    fn get_pool_info(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
    ) -> Result<Option<deadcat_sdk::PoolInfo>, String> {
        let info = self.get_amm_pool(pool_id).map_err(|e| format!("{e}"))?;
        Ok(info.map(|i| deadcat_sdk::PoolInfo {
            params: i.params,
            pool_id: i.pool_id.0,
            creation_txid: i.creation_txid,
        }))
    }

    fn get_latest_pool_snapshot_resume(
        &mut self,
        pool_id: &[u8; 32],
    ) -> Result<Option<([u8; 32], u64)>, String> {
        let snap = self
            .get_latest_pool_snapshot(pool_id)
            .map_err(|e| format!("{e}"))?;
        Ok(snap.map(|s| (s.txid, s.issued_lp)))
    }

    fn insert_pool_snapshot(
        &mut self,
        pool_id: &[u8; 32],
        txid: &[u8; 32],
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
        issued_lp: u64,
        block_height: Option<i32>,
    ) -> Result<(), String> {
        self.insert_pool_snapshot(pool_id, txid, r_yes, r_no, r_lbtc, issued_lp, block_height)
            .map_err(|e| format!("{e}"))
    }

    fn get_pool_id_for_market(
        &mut self,
        market_id: &deadcat_sdk::MarketId,
    ) -> Result<Option<deadcat_sdk::PoolId>, String> {
        let pool = self
            .get_pool_for_market(market_id.as_bytes())
            .map_err(|e| format!("{e}"))?;
        Ok(pool.map(|p| p.pool_id))
    }

    fn get_latest_pool_snapshot(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
    ) -> Result<Option<deadcat_sdk::PoolSnapshot>, String> {
        let snap =
            DeadcatStore::get_latest_pool_snapshot(self, &pool_id.0).map_err(|e| format!("{e}"))?;
        Ok(snap.map(|s| deadcat_sdk::PoolSnapshot {
            reserves: deadcat_sdk::PoolReserves {
                r_yes: s.r_yes,
                r_no: s.r_no,
                r_lbtc: s.r_lbtc,
            },
            issued_lp: s.issued_lp,
            block_height: s.block_height,
        }))
    }

    fn get_pool_snapshot_history(
        &mut self,
        pool_id: &deadcat_sdk::PoolId,
    ) -> Result<Vec<deadcat_sdk::PoolSnapshot>, String> {
        let snaps = self
            .get_pool_snapshots(&pool_id.0)
            .map_err(|e| format!("{e}"))?;
        Ok(snaps
            .into_iter()
            .map(|s| deadcat_sdk::PoolSnapshot {
                reserves: deadcat_sdk::PoolReserves {
                    r_yes: s.r_yes,
                    r_no: s.r_no,
                    r_lbtc: s.r_lbtc,
                },
                issued_lp: s.issued_lp,
                block_height: s.block_height,
            })
            .collect())
    }

    #[allow(clippy::type_complexity)]
    fn get_all_market_spks(&mut self) -> Result<Vec<([u8; 32], Vec<Vec<u8>>)>, String> {
        let rows: Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> = markets::table
            .select((
                markets::market_id,
                markets::dormant_spk,
                markets::unresolved_spk,
                markets::resolved_yes_spk,
                markets::resolved_no_spk,
            ))
            .load(&mut self.conn)
            .map_err(|e| format!("get_all_market_spks: {e}"))?;

        rows.into_iter()
            .map(|(mid, d, u, ry, rn)| {
                let market_id: [u8; 32] = mid
                    .try_into()
                    .map_err(|_| "market_id not 32 bytes".to_string())?;
                Ok((market_id, vec![d, u, ry, rn]))
            })
            .collect()
    }

    fn get_all_pool_watch_info(&mut self) -> Result<Vec<(deadcat_sdk::PoolId, Vec<u8>)>, String> {
        let rows: Vec<AmmPoolRow> = amm_pools::table
            .load(&mut self.conn)
            .map_err(|e| format!("get_all_pool_watch_info: {e}"))?;

        rows.iter()
            .map(|row| {
                let info = AmmPoolInfo::try_from(row).map_err(|e| format!("{e}"))?;
                Ok((info.pool_id, info.covenant_spk))
            })
            .collect()
    }

    fn get_all_nostr_events(&mut self) -> Result<Vec<String>, String> {
        let market_events: Vec<Option<String>> = markets::table
            .select(markets::nostr_event_json)
            .filter(markets::nostr_event_json.is_not_null())
            .load(&mut self.conn)
            .map_err(|e| format!("get_all_nostr_events (markets): {e}"))?;
        let order_events: Vec<Option<String>> = maker_orders::table
            .select(maker_orders::nostr_event_json)
            .filter(maker_orders::nostr_event_json.is_not_null())
            .load(&mut self.conn)
            .map_err(|e| format!("get_all_nostr_events (orders): {e}"))?;
        let pool_events: Vec<Option<String>> = amm_pools::table
            .select(amm_pools::nostr_event_json)
            .filter(amm_pools::nostr_event_json.is_not_null())
            .load(&mut self.conn)
            .map_err(|e| format!("get_all_nostr_events (pools): {e}"))?;
        Ok(market_events
            .into_iter()
            .chain(order_events)
            .chain(pool_events)
            .flatten()
            .collect())
    }
}

// ==================== Sync internals (free functions taking &mut conn) ====================

fn sync_market_utxos<C: ChainSource>(
    conn: &mut SqliteConnection,
    chain: &C,
    report: &mut SyncReport,
) -> crate::Result<()> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Option<Vec<u8>>,
    )> = markets::table
        .select((
            markets::market_id,
            markets::dormant_spk,
            markets::unresolved_spk,
            markets::resolved_yes_spk,
            markets::resolved_no_spk,
            markets::yes_reissuance_token,
            markets::no_reissuance_token,
            markets::yes_issuance_entropy,
        ))
        .load(conn)?;

    for (
        mid_bytes,
        dormant,
        unresolved,
        resolved_yes,
        resolved_no,
        yes_reissuance_token,
        no_reissuance_token,
        yes_entropy_existing,
    ) in &rows
    {
        let spks_with_state = [
            (dormant, MarketState::Dormant),
            (unresolved, MarketState::Unresolved),
            (resolved_yes, MarketState::ResolvedYes),
            (resolved_no, MarketState::ResolvedNo),
        ];

        let mut needs_entropy = yes_entropy_existing.is_none();

        for (spk, state) in &spks_with_state {
            let chain_utxos = chain
                .list_unspent(spk)
                .map_err(|e| StoreError::Sync(e.to_string()))?;

            for cu in chain_utxos {
                let inserted =
                    insert_chain_utxo(conn, &cu, spk, Some(mid_bytes), Some(*state), None)?;
                if inserted {
                    report.new_utxos += 1;

                    // Try to extract issuance entropy from this UTXO's tx
                    if needs_entropy
                        && try_extract_issuance_entropy(
                            conn,
                            chain,
                            &cu.txid,
                            mid_bytes,
                            yes_reissuance_token,
                            no_reissuance_token,
                        )?
                    {
                        needs_entropy = false;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Try to extract issuance entropy from a transaction and store it in the market row.
/// Returns true if entropy was successfully extracted and stored.
fn try_extract_issuance_entropy<C: ChainSource>(
    conn: &mut SqliteConnection,
    chain: &C,
    txid: &[u8; 32],
    mid_bytes: &[u8],
    yes_reissuance_token: &[u8],
    no_reissuance_token: &[u8],
) -> crate::Result<bool> {
    use deadcat_sdk::elements::encode::deserialize as elements_deserialize;
    use deadcat_sdk::elements::hashes::Hash as _;
    use deadcat_sdk::elements::{self, AssetId};

    let raw_tx = match chain
        .get_transaction(txid)
        .map_err(|e| StoreError::Sync(e.to_string()))?
    {
        Some(raw) => raw,
        None => return Ok(false),
    };

    let tx: elements::Transaction = match elements_deserialize(&raw_tx) {
        Ok(tx) => tx,
        Err(_) => return Ok(false),
    };

    let yes_rt = vec_to_array32(yes_reissuance_token, "yes_reissuance_token")?;
    let no_rt = vec_to_array32(no_reissuance_token, "no_reissuance_token")?;

    let mut yes_entropy: Option<[u8; 32]> = None;
    let mut no_entropy: Option<[u8; 32]> = None;
    let mut yes_blinding_nonce: Option<[u8; 32]> = None;
    let mut no_blinding_nonce: Option<[u8; 32]> = None;

    for txin in &tx.input {
        if txin.asset_issuance.is_null() {
            continue;
        }

        let issuance = &txin.asset_issuance;
        let blinding_nonce_bytes: [u8; 32] = {
            let slice: &[u8] = issuance.asset_blinding_nonce.as_ref();
            let mut arr = [0u8; 32];
            arr.copy_from_slice(slice);
            arr
        };
        let is_initial = blinding_nonce_bytes == [0u8; 32];

        // Compute entropy
        let entropy_midstate = if is_initial {
            // Initial issuance: entropy derived from outpoint + contract hash
            let contract_hash = elements::ContractHash::from_byte_array(issuance.asset_entropy);
            AssetId::generate_asset_entropy(txin.previous_output, contract_hash)
        } else {
            // Reissuance: asset_entropy IS the entropy
            elements::hashes::sha256::Midstate::from_byte_array(issuance.asset_entropy)
        };

        // Compute token ID from entropy to match against YES/NO
        let token_id = AssetId::reissuance_token_from_entropy(entropy_midstate, false);
        let token_bytes = token_id.into_inner().to_byte_array();

        let entropy_bytes = entropy_midstate.to_byte_array();

        if token_bytes == yes_rt {
            yes_entropy = Some(entropy_bytes);
            yes_blinding_nonce = Some(blinding_nonce_bytes);
        } else if token_bytes == no_rt {
            no_entropy = Some(entropy_bytes);
            no_blinding_nonce = Some(blinding_nonce_bytes);
        }
    }

    // Only store if we found both YES and NO
    if let (Some(ye), Some(ne), Some(ybn), Some(nbn)) = (
        yes_entropy,
        no_entropy,
        yes_blinding_nonce,
        no_blinding_nonce,
    ) {
        diesel::update(markets::table.filter(markets::market_id.eq(mid_bytes)))
            .set((
                markets::yes_issuance_entropy.eq(ye.to_vec()),
                markets::no_issuance_entropy.eq(ne.to_vec()),
                markets::yes_issuance_blinding_nonce.eq(ybn.to_vec()),
                markets::no_issuance_blinding_nonce.eq(nbn.to_vec()),
            ))
            .execute(conn)?;
        return Ok(true);
    }

    // Also accept partial: store what we found (might find the other in a different tx)
    if yes_entropy.is_some() || no_entropy.is_some() {
        if let (Some(ye), Some(ybn)) = (yes_entropy, yes_blinding_nonce) {
            diesel::update(markets::table.filter(markets::market_id.eq(mid_bytes)))
                .set((
                    markets::yes_issuance_entropy.eq(ye.to_vec()),
                    markets::yes_issuance_blinding_nonce.eq(ybn.to_vec()),
                ))
                .execute(conn)?;
        }
        if let (Some(ne), Some(nbn)) = (no_entropy, no_blinding_nonce) {
            diesel::update(markets::table.filter(markets::market_id.eq(mid_bytes)))
                .set((
                    markets::no_issuance_entropy.eq(ne.to_vec()),
                    markets::no_issuance_blinding_nonce.eq(nbn.to_vec()),
                ))
                .execute(conn)?;
        }
    }

    Ok(false)
}

fn sync_order_utxos<C: ChainSource>(
    conn: &mut SqliteConnection,
    chain: &C,
    report: &mut SyncReport,
) -> crate::Result<()> {
    // covenant_spk is filtered NOT NULL, but Diesel still types the select as Option
    let rows: Vec<(i32, Vec<u8>)> = maker_orders::table
        .select((maker_orders::id, maker_orders::covenant_spk))
        .filter(maker_orders::covenant_spk.is_not_null())
        .load::<(i32, Option<Vec<u8>>)>(conn)?
        .into_iter()
        .filter_map(|(oid, spk)| spk.map(|s| (oid, s)))
        .collect();

    for (order_id, spk) in &rows {
        let chain_utxos = chain
            .list_unspent(spk)
            .map_err(|e| StoreError::Sync(e.to_string()))?;

        for cu in chain_utxos {
            let inserted = insert_chain_utxo(conn, &cu, spk, None, None, Some(*order_id))?;
            if inserted {
                report.new_utxos += 1;
            }
        }
    }

    Ok(())
}

fn sync_spent_utxos<C: ChainSource>(
    conn: &mut SqliteConnection,
    chain: &C,
    report: &mut SyncReport,
) -> crate::Result<()> {
    let unspent_rows: Vec<(Vec<u8>, i32)> = utxos::table
        .select((utxos::txid, utxos::vout))
        .filter(utxos::spent.eq(0))
        .load(conn)?;

    for (txid_bytes, vout_val) in &unspent_rows {
        let txid_arr = vec_to_array32(txid_bytes, "txid")?;
        if let Some(spending) = chain
            .is_spent(&txid_arr, *vout_val as u32)
            .map_err(|e| StoreError::Sync(e.to_string()))?
        {
            diesel::update(
                utxos::table.filter(utxos::txid.eq(txid_bytes).and(utxos::vout.eq(*vout_val))),
            )
            .set((
                utxos::spent.eq(1),
                utxos::spending_txid.eq(spending.to_vec()),
            ))
            .execute(conn)?;
            report.spent_utxos += 1;
        }
    }

    Ok(())
}

/// Derive market state from UTXOs.
///
/// The lifecycle is monotonic: Dormant(0) -> Unresolved(1) -> ResolvedYes(2) or ResolvedNo(3).
/// ResolvedYes and ResolvedNo are alternative terminal states, not ordered by progression.
/// If unspent UTXOs exist at a resolved state, that state wins. If UTXOs exist at multiple
/// resolved states simultaneously (should not happen), we report an error. If no unspent
/// UTXOs exist, the state is left unchanged (resolution is final, and a temporarily empty
/// Dormant/Unresolved market may be mid-transaction).
fn derive_market_states(conn: &mut SqliteConnection, report: &mut SyncReport) -> crate::Result<()> {
    let market_rows: Vec<(Vec<u8>, i32)> = markets::table
        .select((markets::market_id, markets::current_state))
        .load(conn)?;

    for (mid_bytes, old_state) in &market_rows {
        // Get all distinct market_state values with unspent UTXOs for this market
        let live_states: Vec<i32> = utxos::table
            .select(utxos::market_state)
            .filter(
                utxos::market_id
                    .eq(mid_bytes)
                    .and(utxos::spent.eq(0))
                    .and(utxos::market_state.is_not_null()),
            )
            .distinct()
            .load::<Option<i32>>(conn)?
            .into_iter()
            .flatten()
            .collect();

        if live_states.is_empty() {
            continue;
        }

        // Check for conflicting resolved states
        let has_resolved_yes = live_states.contains(&(MarketState::ResolvedYes.as_u64() as i32));
        let has_resolved_no = live_states.contains(&(MarketState::ResolvedNo.as_u64() as i32));
        if has_resolved_yes && has_resolved_no {
            return Err(StoreError::InvalidData(format!(
                "market {} has unspent UTXOs at both ResolvedYes and ResolvedNo",
                hex::encode(mid_bytes)
            )));
        }

        // Pick the highest live state (safe now that we've excluded the Yes/No conflict)
        let new_state = *live_states.iter().max().unwrap();

        if new_state != *old_state {
            diesel::update(markets::table.filter(markets::market_id.eq(mid_bytes)))
                .set((
                    markets::current_state.eq(new_state),
                    markets::updated_at
                        .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                ))
                .execute(conn)?;

            let old = MarketState::from_u64(*old_state as u64).ok_or_else(|| {
                StoreError::InvalidData(format!("invalid market state: {old_state}"))
            })?;
            let new = MarketState::from_u64(new_state as u64).ok_or_else(|| {
                StoreError::InvalidData(format!("invalid market state: {new_state}"))
            })?;
            report.market_state_changes.push(MarketStateChange {
                market_id: MarketId(vec_to_array32(mid_bytes, "market_id")?),
                old_state: old,
                new_state: new,
            });
        }
    }

    Ok(())
}

/// Derive order statuses from UTXO presence:
/// - No UTXOs at all -> Pending
/// - Unspent UTXOs exist, no spent -> Active
/// - Both unspent and spent UTXOs -> PartiallyFilled
/// - All UTXOs spent -> FullyFilled
///
/// Cancelled orders are excluded from derivation (cancellation is terminal).
fn derive_order_statuses(
    conn: &mut SqliteConnection,
    report: &mut SyncReport,
) -> crate::Result<()> {
    let order_rows: Vec<(i32, i32)> = maker_orders::table
        .select((maker_orders::id, maker_orders::order_status))
        .filter(maker_orders::covenant_spk.is_not_null())
        .filter(maker_orders::order_status.ne(OrderStatus::Cancelled as i32))
        .load(conn)?;

    for (oid, old_status) in &order_rows {
        // Single query: count unspent and total
        let unspent_count: i64 = utxos::table
            .filter(utxos::maker_order_id.eq(oid).and(utxos::spent.eq(0)))
            .count()
            .get_result(conn)?;

        let total_count: i64 = utxos::table
            .filter(utxos::maker_order_id.eq(oid))
            .count()
            .get_result(conn)?;

        let spent_count = total_count - unspent_count;

        let new_status = if total_count == 0 {
            OrderStatus::Pending
        } else if spent_count == 0 {
            OrderStatus::Active
        } else if unspent_count > 0 {
            OrderStatus::PartiallyFilled
        } else {
            OrderStatus::FullyFilled
        };

        let new_status_i32 = new_status.as_i32();
        if new_status_i32 != *old_status {
            diesel::update(maker_orders::table.filter(maker_orders::id.eq(oid)))
                .set((
                    maker_orders::order_status.eq(new_status_i32),
                    maker_orders::updated_at
                        .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                ))
                .execute(conn)?;
            report.order_status_changes.push(OrderStatusChange {
                order_id: *oid,
                old_status: OrderStatus::from_i32(*old_status)?,
                new_status,
            });
        }
    }

    Ok(())
}

/// Insert a chain-discovered UTXO. Blinding factors are zeros because covenant
/// outputs on Elements use explicit (non-confidential) values.
fn insert_chain_utxo(
    conn: &mut SqliteConnection,
    cu: &ChainUtxo,
    spk: &[u8],
    market_id_bytes: Option<&[u8]>,
    market_state: Option<MarketState>,
    maker_order_id: Option<i32>,
) -> crate::Result<bool> {
    let row = NewUtxoRow {
        txid: cu.txid.to_vec(),
        vout: cu.vout as i32,
        script_pubkey: spk.to_vec(),
        asset_id: cu.asset_id.to_vec(),
        value: cu.value as i64,
        asset_blinding_factor: [0u8; 32].to_vec(),
        value_blinding_factor: [0u8; 32].to_vec(),
        raw_txout: cu.raw_txout.clone(),
        market_id: market_id_bytes.map(|b| b.to_vec()),
        maker_order_id,
        market_state: market_state.map(|s| s.as_u64() as i32),
        block_height: cu.block_height.map(|h| h as i32),
        amm_pool_id: None,
    };

    let count = diesel::insert_or_ignore_into(utxos::table)
        .values(&row)
        .execute(conn)?;

    Ok(count > 0)
}

// Tiny hex helper for error messages (avoids pulling in the `hex` crate)
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}
