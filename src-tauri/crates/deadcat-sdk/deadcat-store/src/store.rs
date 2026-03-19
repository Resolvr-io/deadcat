use chrono::{TimeZone, Utc};
use diesel::prelude::*;
use diesel::sql_types::Integer;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use deadcat_sdk::{
    CompiledMakerOrder, CompiledPredictionMarket, LmsrPoolIngestInput, MakerOrderParams, MarketId,
    MarketSlot, MarketState, OrderDirection, PredictionMarketAnchor,
    PredictionMarketCandidateIngestInput, PredictionMarketParams, UnblindedUtxo,
    parse_prediction_market_anchor,
    prediction_market_scan::{
        CanonicalMarketScan, PredictionMarketScanBackend, scan_prediction_market_canonical,
        validate_prediction_market_creation_tx,
    },
};

use crate::conversions::{
    DecodedDormantOpenings, direction_to_i32, new_maker_order_row, new_market_candidate_row,
    new_utxo_row, vec_to_array32,
};
use crate::error::StoreError;
use crate::models::{MakerOrderRow, MarketCandidateRow, MarketRow, NewUtxoRow, UtxoRow};
use crate::schema::{maker_orders, market_candidates, markets, sync_state, utxos};
use crate::sync::{ChainSource, ChainUtxo, MarketStateChange, OrderStatusChange, SyncReport};

use deadcat_sdk::elements::Txid;
use deadcat_sdk::elements::hashes::Hash as _;

// Keep this const near migration changes so deadcat-store rebuilds when
// embedded migration sets are updated.
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// SQL expression for SQLite's `datetime('now')`.
const DATETIME_NOW: &str = "datetime('now')";
const CANDIDATE_TTL_SECS: u64 = 6 * 60 * 60;

fn sqlite_datetime_from_unix(now_unix: u64) -> crate::Result<String> {
    let ts = i64::try_from(now_unix)
        .map_err(|_| StoreError::InvalidData(format!("timestamp out of range: {now_unix}")))?;
    let dt = Utc
        .timestamp_opt(ts, 0)
        .single()
        .ok_or_else(|| StoreError::InvalidData(format!("invalid timestamp: {now_unix}")))?;
    Ok(dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

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
    pub anchor: PredictionMarketAnchor,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MarketCandidateInfo {
    pub candidate_id: i32,
    pub market_id: MarketId,
    pub params: PredictionMarketParams,
    pub cmr: [u8; 32],
    pub issuance: Option<IssuanceData>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub expires_at: Option<String>,
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub creator_pubkey: Option<Vec<u8>>,
    pub anchor: PredictionMarketAnchor,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

fn issuance_data_complete(row: &MarketCandidateRow) -> bool {
    row.yes_issuance_entropy.is_some()
        && row.no_issuance_entropy.is_some()
        && row.yes_issuance_blinding_nonce.is_some()
        && row.no_issuance_blinding_nonce.is_some()
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
pub struct MarketCandidateFilter {
    pub market_id: Option<MarketId>,
    pub oracle_public_key: Option<[u8; 32]>,
    pub collateral_asset_id: Option<[u8; 32]>,
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

    /// Ingest a level-2-valid prediction-market candidate discovered off-chain.
    ///
    /// This crate does not treat ingestion as chain validation. Candidates remain
    /// off-chain until a higher-level sync service promotes them after 2 confirmations.
    pub fn ingest_prediction_market_candidate(
        &mut self,
        input: &PredictionMarketCandidateIngestInput,
        seen_at_unix: u64,
    ) -> crate::Result<i32> {
        let mut input = input.clone();
        input.metadata.anchor = input
            .metadata
            .anchor
            .canonicalized()
            .map_err(StoreError::InvalidData)?;
        let creation_tx: deadcat_sdk::elements::Transaction =
            deadcat_sdk::elements::encode::deserialize(&input.creation_tx)
                .map_err(|e| StoreError::InvalidData(format!("invalid creation_tx bytes: {e}")))?;
        if creation_tx.txid().to_string() != input.metadata.anchor.creation_txid {
            return Err(StoreError::InvalidData(
                "creation_tx bytes txid must equal anchor.creation_txid".to_string(),
            ));
        }
        if !validate_prediction_market_creation_tx(
            &input.params,
            &creation_tx,
            &input.metadata.anchor,
        )
        .map_err(StoreError::InvalidData)?
        {
            return Err(StoreError::InvalidData(
                "creation_tx is not a canonical prediction-market creation bootstrap".into(),
            ));
        }

        let seen_at = sqlite_datetime_from_unix(seen_at_unix)?;
        let expires_at = sqlite_datetime_from_unix(seen_at_unix + CANDIDATE_TTL_SECS)?;

        let mid = input.params.market_id();
        let mid_bytes = mid.as_bytes().to_vec();
        let yes_abf = ::hex::decode(
            &input
                .metadata
                .anchor
                .yes_dormant_opening
                .asset_blinding_factor,
        )
        .map_err(|e| StoreError::InvalidData(format!("invalid yes dormant ABF: {e}")))?;
        let yes_vbf = ::hex::decode(
            &input
                .metadata
                .anchor
                .yes_dormant_opening
                .value_blinding_factor,
        )
        .map_err(|e| StoreError::InvalidData(format!("invalid yes dormant VBF: {e}")))?;
        let no_abf = ::hex::decode(
            &input
                .metadata
                .anchor
                .no_dormant_opening
                .asset_blinding_factor,
        )
        .map_err(|e| StoreError::InvalidData(format!("invalid no dormant ABF: {e}")))?;
        let no_vbf = ::hex::decode(
            &input
                .metadata
                .anchor
                .no_dormant_opening
                .value_blinding_factor,
        )
        .map_err(|e| StoreError::InvalidData(format!("invalid no dormant VBF: {e}")))?;

        let existing: Option<MarketCandidateRow> = market_candidates::table
            .filter(
                market_candidates::market_id
                    .eq(&mid_bytes)
                    .and(market_candidates::creation_txid.eq(&input.metadata.anchor.creation_txid))
                    .and(market_candidates::yes_dormant_asset_blinding_factor.eq(&yes_abf))
                    .and(market_candidates::yes_dormant_value_blinding_factor.eq(&yes_vbf))
                    .and(market_candidates::no_dormant_asset_blinding_factor.eq(&no_abf))
                    .and(market_candidates::no_dormant_value_blinding_factor.eq(&no_vbf)),
            )
            .first(&mut self.conn)
            .optional()?;

        if let Some(existing) = existing {
            let expires_value = if existing.promoted_at.is_none() {
                Some(expires_at.clone())
            } else {
                None
            };
            diesel::update(
                market_candidates::table
                    .filter(market_candidates::candidate_id.eq(existing.candidate_id)),
            )
            .set((
                market_candidates::last_seen_at.eq(&seen_at),
                market_candidates::expires_at.eq(expires_value),
                market_candidates::nevent.eq(input.metadata.nevent.clone()),
                market_candidates::nostr_event_id.eq(input.metadata.nostr_event_id.clone()),
                market_candidates::nostr_event_json.eq(input.metadata.nostr_event_json.clone()),
            ))
            .execute(&mut self.conn)?;
            return Ok(existing.candidate_id);
        }

        let compiled = CompiledPredictionMarket::new(input.params)?;
        let row = new_market_candidate_row(
            &input,
            &compiled,
            DecodedDormantOpenings {
                yes_abf,
                yes_vbf,
                no_abf,
                no_vbf,
            },
            &seen_at,
            &expires_at,
        );

        diesel::insert_into(market_candidates::table)
            .values(&row)
            .execute(&mut self.conn)?;

        let row_id: i32 = diesel::select(diesel::dsl::sql::<Integer>("last_insert_rowid()"))
            .get_result(&mut self.conn)?;
        Ok(row_id)
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

    /// Ingest or update an LMSR pool discovery snapshot.
    pub fn ingest_lmsr_pool(&mut self, input: &LmsrPoolIngestInput) -> crate::Result<()> {
        use diesel::sql_types::{BigInt, Nullable, Text};

        let params_json = serde_json::json!({
            "yes_asset_id": hex::encode(&input.yes_asset_id),
            "no_asset_id": hex::encode(&input.no_asset_id),
            "collateral_asset_id": hex::encode(&input.collateral_asset_id),
            "fee_bps": input.fee_bps,
            "cosigner_pubkey": hex::encode(&input.cosigner_pubkey),
            "lmsr_table_root": hex::encode(&input.lmsr_table_root),
            "table_depth": input.table_depth,
            "q_step_lots": input.q_step_lots,
            "s_bias": input.s_bias,
            "s_max_index": input.s_max_index,
            "half_payout_sats": input.half_payout_sats
        })
        .to_string();
        let canonical_state_source = deadcat_sdk::LmsrPoolStateSource::CanonicalScan.as_str();
        let announcement_state_source = deadcat_sdk::LmsrPoolStateSource::Announcement.as_str();
        let query = format!(
            "INSERT INTO lmsr_pools (
                pool_id,
                market_id,
                creation_txid,
                witness_schema_version,
                current_s_index,
                reserve_yes,
                reserve_no,
                reserve_collateral,
                reserve_yes_outpoint,
                reserve_no_outpoint,
                reserve_collateral_outpoint,
                state_source,
                last_transition_txid,
                params_json,
                nostr_event_id,
                nostr_event_json,
                created_at,
                updated_at
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now')
            )
            ON CONFLICT(pool_id) DO UPDATE SET
                market_id = lmsr_pools.market_id,
                creation_txid = lmsr_pools.creation_txid,
                witness_schema_version = lmsr_pools.witness_schema_version,
                current_s_index = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.current_s_index
                    ELSE excluded.current_s_index
                END,
                reserve_yes = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.reserve_yes
                    ELSE excluded.reserve_yes
                END,
                reserve_no = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.reserve_no
                    ELSE excluded.reserve_no
                END,
                reserve_collateral = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.reserve_collateral
                    ELSE excluded.reserve_collateral
                END,
                reserve_yes_outpoint = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.reserve_yes_outpoint
                    ELSE excluded.reserve_yes_outpoint
                END,
                reserve_no_outpoint = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.reserve_no_outpoint
                    ELSE excluded.reserve_no_outpoint
                END,
                reserve_collateral_outpoint = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.reserve_collateral_outpoint
                    ELSE excluded.reserve_collateral_outpoint
                END,
                state_source = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.state_source
                    ELSE excluded.state_source
                END,
                last_transition_txid = CASE
                    WHEN lmsr_pools.state_source = '{canonical_state_source}'
                        AND excluded.state_source = '{announcement_state_source}'
                    THEN lmsr_pools.last_transition_txid
                    ELSE excluded.last_transition_txid
                END,
                params_json = lmsr_pools.params_json,
                nostr_event_id = COALESCE(excluded.nostr_event_id, lmsr_pools.nostr_event_id),
                nostr_event_json = COALESCE(excluded.nostr_event_json, lmsr_pools.nostr_event_json),
                updated_at = datetime('now')"
        );

        diesel::sql_query(query)
            .bind::<Text, _>(&input.pool_id)
            .bind::<Text, _>(&input.market_id)
            .bind::<Text, _>(&input.creation_txid)
            .bind::<Text, _>(&input.witness_schema_version)
            .bind::<BigInt, _>(input.current_s_index as i64)
            .bind::<BigInt, _>(input.reserve_yes as i64)
            .bind::<BigInt, _>(input.reserve_no as i64)
            .bind::<BigInt, _>(input.reserve_collateral as i64)
            .bind::<Text, _>(&input.reserve_outpoints[0])
            .bind::<Text, _>(&input.reserve_outpoints[1])
            .bind::<Text, _>(&input.reserve_outpoints[2])
            .bind::<Text, _>(input.state_source.as_str())
            .bind::<Nullable<Text>, _>(input.last_transition_txid.as_deref())
            .bind::<Text, _>(&params_json)
            .bind::<Nullable<Text>, _>(input.nostr_event_id.as_deref())
            .bind::<Nullable<Text>, _>(input.nostr_event_json.as_deref())
            .execute(&mut self.conn)?;

        Ok(())
    }

    /// Update canonical LMSR live state derived from chain scan.
    pub fn upsert_lmsr_pool_state(
        &mut self,
        input: &deadcat_sdk::LmsrPoolStateUpdateInput,
    ) -> crate::Result<()> {
        use diesel::sql_types::{BigInt, Nullable, Text};

        let rows = diesel::sql_query(
            "UPDATE lmsr_pools
             SET current_s_index = ?,
                 reserve_yes = ?,
                 reserve_no = ?,
                 reserve_collateral = ?,
                 reserve_yes_outpoint = ?,
                 reserve_no_outpoint = ?,
                 reserve_collateral_outpoint = ?,
                 state_source = ?,
                 last_transition_txid = ?,
                 updated_at = datetime('now')
             WHERE pool_id = ?",
        )
        .bind::<BigInt, _>(input.current_s_index as i64)
        .bind::<BigInt, _>(input.reserve_yes as i64)
        .bind::<BigInt, _>(input.reserve_no as i64)
        .bind::<BigInt, _>(input.reserve_collateral as i64)
        .bind::<Text, _>(&input.reserve_outpoints[0])
        .bind::<Text, _>(&input.reserve_outpoints[1])
        .bind::<Text, _>(&input.reserve_outpoints[2])
        .bind::<Text, _>(deadcat_sdk::LmsrPoolStateSource::CanonicalScan.as_str())
        .bind::<Nullable<Text>, _>(input.last_transition_txid.as_deref())
        .bind::<Text, _>(&input.pool_id)
        .execute(&mut self.conn)?;

        if rows == 0 {
            return Err(StoreError::InvalidData(format!(
                "cannot update canonical LMSR state for unknown pool_id {}",
                input.pool_id
            )));
        }
        Ok(())
    }

    // ==================== Market Queries ====================

    fn load_candidate(&mut self, candidate_id: i32) -> crate::Result<MarketCandidateRow> {
        market_candidates::table
            .filter(market_candidates::candidate_id.eq(candidate_id))
            .first(&mut self.conn)
            .map_err(Into::into)
    }

    pub fn get_market(&mut self, mid: &MarketId) -> crate::Result<Option<MarketInfo>> {
        let market: Option<MarketRow> = markets::table
            .filter(markets::market_id.eq(mid.as_bytes().to_vec()))
            .first(&mut self.conn)
            .optional()?;

        let Some(market) = market else {
            return Ok(None);
        };
        let candidate = self.load_candidate(market.candidate_id)?;
        Ok(Some(crate::conversions::market_info_from_rows(
            &market, &candidate,
        )?))
    }

    pub fn list_markets(&mut self, filter: &MarketFilter) -> crate::Result<Vec<MarketInfo>> {
        let mut query = markets::table.into_boxed();

        if let Some(state) = filter.current_state {
            query = query.filter(markets::current_state.eq(state.as_u64() as i32));
        }
        if let Some(lim) = filter.limit {
            query = query.limit(lim);
        }

        let rows: Vec<MarketRow> = query.load(&mut self.conn)?;
        let mut markets_info = Vec::new();
        for market in rows {
            let candidate = self.load_candidate(market.candidate_id)?;
            if let Some(ref opk) = filter.oracle_public_key
                && candidate.oracle_public_key != opk.to_vec()
            {
                continue;
            }
            if let Some(ref caid) = filter.collateral_asset_id
                && candidate.collateral_asset_id != caid.to_vec()
            {
                continue;
            }
            if let Some(before) = filter.expiry_before
                && candidate.expiry_time >= before as i32
            {
                continue;
            }
            if let Some(after) = filter.expiry_after
                && candidate.expiry_time <= after as i32
            {
                continue;
            }
            markets_info.push(crate::conversions::market_info_from_rows(
                &market, &candidate,
            )?);
        }
        Ok(markets_info)
    }

    /// Return a visible, unpromoted candidate if it has not yet hit its TTL.
    ///
    /// Callers pass `now_unix` explicitly so candidate visibility flips exactly
    /// at the requested read time; the background cleanup pass is separate.
    pub fn get_prediction_market_candidate(
        &mut self,
        candidate_id: i32,
        now_unix: u64,
    ) -> crate::Result<Option<MarketCandidateInfo>> {
        let now = sqlite_datetime_from_unix(now_unix)?;
        let row: Option<MarketCandidateRow> = market_candidates::table
            .filter(
                market_candidates::candidate_id
                    .eq(candidate_id)
                    .and(market_candidates::promoted_at.is_null())
                    .and(market_candidates::expires_at.gt(now)),
            )
            .first(&mut self.conn)
            .optional()?;
        row.as_ref().map(MarketCandidateInfo::try_from).transpose()
    }

    /// List visible, unpromoted candidates that have not yet expired at
    /// `now_unix`.
    pub fn list_prediction_market_candidates(
        &mut self,
        filter: &MarketCandidateFilter,
        now_unix: u64,
    ) -> crate::Result<Vec<MarketCandidateInfo>> {
        let now = sqlite_datetime_from_unix(now_unix)?;
        let mut query = market_candidates::table
            .filter(
                market_candidates::promoted_at
                    .is_null()
                    .and(market_candidates::expires_at.gt(now)),
            )
            .into_boxed();

        if let Some(ref mid) = filter.market_id {
            query = query.filter(market_candidates::market_id.eq(mid.as_bytes().to_vec()));
        }
        if let Some(ref opk) = filter.oracle_public_key {
            query = query.filter(market_candidates::oracle_public_key.eq(opk.to_vec()));
        }
        if let Some(ref caid) = filter.collateral_asset_id {
            query = query.filter(market_candidates::collateral_asset_id.eq(caid.to_vec()));
        }
        if let Some(before) = filter.expiry_before {
            query = query.filter(market_candidates::expiry_time.lt(before as i32));
        }
        if let Some(after) = filter.expiry_after {
            query = query.filter(market_candidates::expiry_time.gt(after as i32));
        }
        if let Some(lim) = filter.limit {
            query = query.limit(lim);
        }

        let rows: Vec<MarketCandidateRow> = query.load(&mut self.conn)?;
        rows.iter().map(MarketCandidateInfo::try_from).collect()
    }

    /// List all unpromoted candidates, including rows whose TTL has passed but
    /// have not yet been purged.
    ///
    /// This is intended for higher-level promotion/cleanup services rather than
    /// public candidate views.
    pub fn list_unpromoted_prediction_market_candidates(
        &mut self,
    ) -> crate::Result<Vec<MarketCandidateInfo>> {
        let rows: Vec<MarketCandidateRow> = market_candidates::table
            .filter(market_candidates::promoted_at.is_null())
            .load(&mut self.conn)?;
        rows.iter().map(MarketCandidateInfo::try_from).collect()
    }

    /// Delete unpromoted candidates whose TTL has expired at `now_unix`.
    ///
    /// Cleanup is explicit; the store never schedules purge work on its own.
    pub fn purge_expired_prediction_market_candidates(
        &mut self,
        now_unix: u64,
    ) -> crate::Result<usize> {
        let now = sqlite_datetime_from_unix(now_unix)?;
        diesel::delete(
            market_candidates::table.filter(
                market_candidates::promoted_at
                    .is_null()
                    .and(market_candidates::expires_at.le(now)),
            ),
        )
        .execute(&mut self.conn)
        .map_err(Into::into)
    }

    /// Promote a candidate into the canonical `markets` table.
    ///
    /// The caller is responsible for ensuring the candidate's anchor tx is
    /// irreversible on Liquid before calling this method. `deadcat-store`
    /// treats promotion as one-way and does not handle reorgs.
    pub fn promote_prediction_market_candidate(
        &mut self,
        candidate_id: i32,
        promoted_at_unix: u64,
        promotion_height: u32,
        promotion_block_hash: [u8; 32],
    ) -> crate::Result<()> {
        let promoted_at = sqlite_datetime_from_unix(promoted_at_unix)?;
        self.conn.transaction(|conn| {
            let candidate: MarketCandidateRow = market_candidates::table
                .filter(market_candidates::candidate_id.eq(candidate_id))
                .first(conn)?;

            let existing: Option<MarketRow> = markets::table
                .filter(markets::market_id.eq(&candidate.market_id))
                .first(conn)
                .optional()?;

            if let Some(existing) = existing {
                if existing.candidate_id == candidate_id {
                    return Ok(());
                }
                return Err(StoreError::InvalidData(format!(
                    "market_id {} already has a canonical candidate",
                    hex::encode(&candidate.market_id)
                )));
            }

            diesel::update(
                market_candidates::table.filter(market_candidates::candidate_id.eq(candidate_id)),
            )
            .set((
                market_candidates::expires_at.eq(Option::<String>::None),
                market_candidates::promoted_at.eq(Some(promoted_at.clone())),
                market_candidates::promotion_height.eq(Some(promotion_height as i32)),
                market_candidates::promotion_block_hash.eq(Some(promotion_block_hash.to_vec())),
            ))
            .execute(conn)?;

            diesel::insert_into(markets::table)
                .values((
                    markets::market_id.eq(candidate.market_id.clone()),
                    markets::candidate_id.eq(candidate_id),
                    markets::current_state.eq(MarketState::Dormant.as_u64() as i32),
                ))
                .execute(conn)?;

            diesel::delete(
                market_candidates::table.filter(
                    market_candidates::market_id
                        .eq(&candidate.market_id)
                        .and(market_candidates::candidate_id.ne(candidate_id)),
                ),
            )
            .execute(conn)?;

            Ok(())
        })
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
            let slots: Vec<i32> = s
                .live_slots()
                .iter()
                .map(|slot| slot.as_u8() as i32)
                .collect();
            query = query.filter(utxos::market_slot.eq_any(slots));
        }

        let rows: Vec<UtxoRow> = query.load(&mut self.conn)?;
        rows.iter().map(UnblindedUtxo::try_from).collect()
    }

    pub fn get_market_slot_utxos(
        &mut self,
        mid: &MarketId,
        slot: MarketSlot,
    ) -> crate::Result<Vec<UnblindedUtxo>> {
        let rows: Vec<UtxoRow> = utxos::table
            .filter(
                utxos::market_id
                    .eq(mid.as_bytes().to_vec())
                    .and(utxos::spent.eq(0))
                    .and(utxos::market_slot.eq(slot.as_u8() as i32)),
            )
            .load(&mut self.conn)?;

        rows.iter().map(UnblindedUtxo::try_from).collect()
    }

    pub fn get_order_utxos(&mut self, order_id: i32) -> crate::Result<Vec<UnblindedUtxo>> {
        let rows: Vec<UtxoRow> = utxos::table
            .filter(utxos::maker_order_id.eq(order_id).and(utxos::spent.eq(0)))
            .load(&mut self.conn)?;

        rows.iter().map(UnblindedUtxo::try_from).collect()
    }

    // ==================== Manual UTXO Management ====================

    pub fn add_market_slot_utxo(
        &mut self,
        mid: &MarketId,
        slot: MarketSlot,
        utxo: &UnblindedUtxo,
        height: Option<u32>,
    ) -> crate::Result<()> {
        let row = new_utxo_row(utxo, Some(mid), Some(slot), None, height);

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
        let canonical: MarketRow = markets::table
            .filter(markets::market_id.eq(mid.as_bytes().to_vec()))
            .first(&mut self.conn)?;
        diesel::update(
            market_candidates::table
                .filter(market_candidates::candidate_id.eq(canonical.candidate_id)),
        )
        .set((
            market_candidates::yes_issuance_entropy.eq(data.yes_entropy.to_vec()),
            market_candidates::no_issuance_entropy.eq(data.no_entropy.to_vec()),
            market_candidates::yes_issuance_blinding_nonce.eq(data.yes_blinding_nonce.to_vec()),
            market_candidates::no_issuance_blinding_nonce.eq(data.no_blinding_nonce.to_vec()),
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

    /// Collect all watched scriptPubKeys: 8 per market, 1 per maker order with known pubkey.
    pub fn watched_script_pubkeys(&mut self) -> crate::Result<Vec<Vec<u8>>> {
        let mut spks = Vec::new();

        let market_rows: Vec<MarketRow> = markets::table.load(&mut self.conn)?;

        for row in market_rows {
            let candidate = self.load_candidate(row.candidate_id)?;
            spks.push(candidate.dormant_yes_rt_spk);
            spks.push(candidate.dormant_no_rt_spk);
            spks.push(candidate.unresolved_yes_rt_spk);
            spks.push(candidate.unresolved_no_rt_spk);
            spks.push(candidate.unresolved_collateral_spk);
            spks.push(candidate.resolved_yes_collateral_spk);
            spks.push(candidate.resolved_no_collateral_spk);
            spks.push(candidate.expired_collateral_spk);
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

    /// Run canonical market/order sync against a chain source.
    ///
    /// Only promoted canonical markets participate in this sync.
    ///
    /// 1. Rebuild each market's canonical live slot bundle from its promoted anchor
    /// 2. For each watched order SPK, discover new UTXOs via `chain.list_unspent`
    /// 3. For each existing unspent UTXO, check if spent via `chain.is_spent`
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
            derive_order_statuses(conn, &mut report)?;

            diesel::update(sync_state::table.filter(sync_state::id.eq(1)))
                .set(sync_state::last_block_height.eq(best_height as i32))
                .execute(conn)?;

            Ok(report)
        })
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
            MarketState::Expired => {
                diesel::update(markets::table.filter(markets::market_id.eq(&mid_bytes)))
                    .set(markets::expired_txid.eq(Some(txid)))
                    .execute(&mut self.conn)?;
            }
        }
        Ok(())
    }
}

// ==================== DiscoveryStore trait impl ====================

impl deadcat_sdk::DiscoveryStore for DeadcatStore {
    fn ingest_prediction_market_candidate(
        &mut self,
        input: &PredictionMarketCandidateIngestInput,
        seen_at_unix: u64,
    ) -> Result<(), String> {
        self.ingest_prediction_market_candidate(input, seen_at_unix)
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

    fn ingest_lmsr_pool(&mut self, input: &LmsrPoolIngestInput) -> Result<(), String> {
        self.ingest_lmsr_pool(input).map_err(|e| format!("{e}"))
    }

    fn upsert_lmsr_pool_state(
        &mut self,
        input: &deadcat_sdk::LmsrPoolStateUpdateInput,
    ) -> Result<(), String> {
        self.upsert_lmsr_pool_state(input)
            .map_err(|e| format!("{e}"))
    }
}

// ==================== Sync internals (free functions taking &mut conn) ====================

struct StorePredictionMarketScanBackend<'a, C> {
    chain: &'a C,
}

impl<C: ChainSource> PredictionMarketScanBackend for StorePredictionMarketScanBackend<'_, C> {
    fn fetch_transaction(
        &self,
        txid: &Txid,
    ) -> std::result::Result<deadcat_sdk::elements::Transaction, String> {
        let raw = self
            .chain
            .get_transaction(&txid.to_byte_array())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("transaction {txid} not found"))?;
        deadcat_sdk::elements::encode::deserialize(&raw)
            .map_err(|e| format!("failed to decode transaction {txid}: {e}"))
    }

    fn spending_txid(
        &self,
        outpoint: &deadcat_sdk::elements::OutPoint,
        _script_pubkey: &deadcat_sdk::elements::Script,
    ) -> std::result::Result<Option<Txid>, String> {
        self.chain
            .is_spent(&outpoint.txid.to_byte_array(), outpoint.vout)
            .map(|spent| spent.map(Txid::from_byte_array))
            .map_err(|e| e.to_string())
    }
}

fn market_slot_script_pubkey(row: &MarketCandidateRow, slot: MarketSlot) -> &[u8] {
    match slot {
        MarketSlot::DormantYesRt => &row.dormant_yes_rt_spk,
        MarketSlot::DormantNoRt => &row.dormant_no_rt_spk,
        MarketSlot::UnresolvedYesRt => &row.unresolved_yes_rt_spk,
        MarketSlot::UnresolvedNoRt => &row.unresolved_no_rt_spk,
        MarketSlot::UnresolvedCollateral => &row.unresolved_collateral_spk,
        MarketSlot::ResolvedYesCollateral => &row.resolved_yes_collateral_spk,
        MarketSlot::ResolvedNoCollateral => &row.resolved_no_collateral_spk,
        MarketSlot::ExpiredCollateral => &row.expired_collateral_spk,
    }
}

fn clear_market_utxo_tags(conn: &mut SqliteConnection, market_id: &[u8]) -> crate::Result<()> {
    diesel::update(utxos::table.filter(utxos::market_id.eq(market_id)))
        .set((
            utxos::market_id.eq(Option::<Vec<u8>>::None),
            utxos::market_slot.eq(Option::<i32>::None),
        ))
        .execute(conn)?;
    Ok(())
}

fn upsert_market_chain_utxo(
    conn: &mut SqliteConnection,
    cu: &ChainUtxo,
    spk: &[u8],
    market_id_bytes: &[u8],
    market_slot: MarketSlot,
) -> crate::Result<bool> {
    let inserted = insert_chain_utxo(
        conn,
        cu,
        spk,
        Some(market_id_bytes),
        Some(market_slot),
        None,
    )?;

    if !inserted {
        diesel::update(
            utxos::table.filter(
                utxos::txid
                    .eq(cu.txid.to_vec())
                    .and(utxos::vout.eq(cu.vout as i32)),
            ),
        )
        .set((
            utxos::script_pubkey.eq(spk.to_vec()),
            utxos::asset_id.eq(cu.asset_id.to_vec()),
            utxos::value.eq(cu.value as i64),
            utxos::asset_blinding_factor.eq([0u8; 32].to_vec()),
            utxos::value_blinding_factor.eq([0u8; 32].to_vec()),
            utxos::raw_txout.eq(cu.raw_txout.clone()),
            utxos::market_id.eq(Some(market_id_bytes.to_vec())),
            utxos::maker_order_id.eq(Option::<i32>::None),
            utxos::market_slot.eq(Some(market_slot.as_u8() as i32)),
            utxos::spent.eq(0),
            utxos::spending_txid.eq(Option::<Vec<u8>>::None),
            utxos::block_height.eq(cu.block_height.map(|height| height as i32)),
            utxos::spent_block_height.eq(Option::<i32>::None),
        ))
        .execute(conn)?;
    }

    Ok(inserted)
}

fn update_market_state_from_scan(
    conn: &mut SqliteConnection,
    row: &MarketRow,
    scan: &CanonicalMarketScan,
    report: &mut SyncReport,
) -> crate::Result<()> {
    let new_state = scan.state;
    let new_state_i32 = new_state.as_u64() as i32;
    let old_state = MarketState::from_u64(row.current_state as u64).ok_or_else(|| {
        StoreError::InvalidData(format!("invalid market state: {}", row.current_state))
    })?;

    if scan.utxos.is_empty() {
        diesel::update(markets::table.filter(markets::market_id.eq(&row.market_id)))
            .set((
                markets::current_state.eq(new_state_i32),
                markets::updated_at.eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
            ))
            .execute(conn)?;
    } else {
        let transition_txid = scan.last_transition_txid.to_string();
        match new_state {
            MarketState::Dormant => {
                diesel::update(markets::table.filter(markets::market_id.eq(&row.market_id)))
                    .set((
                        markets::current_state.eq(new_state_i32),
                        markets::dormant_txid.eq(Some(transition_txid)),
                        markets::updated_at
                            .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                    ))
                    .execute(conn)?;
            }
            MarketState::Unresolved => {
                diesel::update(markets::table.filter(markets::market_id.eq(&row.market_id)))
                    .set((
                        markets::current_state.eq(new_state_i32),
                        markets::unresolved_txid.eq(Some(transition_txid)),
                        markets::updated_at
                            .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                    ))
                    .execute(conn)?;
            }
            MarketState::ResolvedYes => {
                diesel::update(markets::table.filter(markets::market_id.eq(&row.market_id)))
                    .set((
                        markets::current_state.eq(new_state_i32),
                        markets::resolved_yes_txid.eq(Some(transition_txid)),
                        markets::updated_at
                            .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                    ))
                    .execute(conn)?;
            }
            MarketState::ResolvedNo => {
                diesel::update(markets::table.filter(markets::market_id.eq(&row.market_id)))
                    .set((
                        markets::current_state.eq(new_state_i32),
                        markets::resolved_no_txid.eq(Some(transition_txid)),
                        markets::updated_at
                            .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                    ))
                    .execute(conn)?;
            }
            MarketState::Expired => {
                diesel::update(markets::table.filter(markets::market_id.eq(&row.market_id)))
                    .set((
                        markets::current_state.eq(new_state_i32),
                        markets::expired_txid.eq(Some(transition_txid)),
                        markets::updated_at
                            .eq(diesel::dsl::sql::<diesel::sql_types::Text>(DATETIME_NOW)),
                    ))
                    .execute(conn)?;
            }
        }
    }

    if old_state != new_state {
        report.market_state_changes.push(MarketStateChange {
            market_id: MarketId(vec_to_array32(&row.market_id, "market_id")?),
            old_state,
            new_state,
        });
    }

    Ok(())
}

fn sync_market_utxos<C: ChainSource>(
    conn: &mut SqliteConnection,
    chain: &C,
    report: &mut SyncReport,
) -> crate::Result<()> {
    let rows: Vec<MarketRow> = markets::table.load(conn)?;
    let backend = StorePredictionMarketScanBackend { chain };

    for row in &rows {
        let candidate: MarketCandidateRow = market_candidates::table
            .filter(market_candidates::candidate_id.eq(row.candidate_id))
            .first(conn)?;
        let params = PredictionMarketParams::try_from(&candidate)?;
        let anchor = PredictionMarketAnchor {
            creation_txid: candidate.creation_txid.clone(),
            yes_dormant_opening: deadcat_sdk::DormantOutputOpening::from_bytes(
                vec_to_array32(
                    &candidate.yes_dormant_asset_blinding_factor,
                    "yes_dormant_asset_blinding_factor",
                )?,
                vec_to_array32(
                    &candidate.yes_dormant_value_blinding_factor,
                    "yes_dormant_value_blinding_factor",
                )?,
            ),
            no_dormant_opening: deadcat_sdk::DormantOutputOpening::from_bytes(
                vec_to_array32(
                    &candidate.no_dormant_asset_blinding_factor,
                    "no_dormant_asset_blinding_factor",
                )?,
                vec_to_array32(
                    &candidate.no_dormant_value_blinding_factor,
                    "no_dormant_value_blinding_factor",
                )?,
            ),
        };
        let parsed_anchor =
            parse_prediction_market_anchor(&anchor).map_err(StoreError::InvalidData)?;
        let scan = scan_prediction_market_canonical(&backend, &params, &anchor)
            .map_err(StoreError::Sync)?;

        let needs_entropy = !issuance_data_complete(&candidate);
        let mut candidate_txids = vec![parsed_anchor.creation_txid.to_byte_array()];

        clear_market_utxo_tags(conn, &row.market_id)?;

        for canonical_utxo in &scan.utxos {
            let spk = market_slot_script_pubkey(&candidate, canonical_utxo.slot);
            let chain_utxo = chain
                .list_unspent(spk)
                .map_err(|e| StoreError::Sync(e.to_string()))?
                .into_iter()
                .find(|cu| {
                    cu.txid == canonical_utxo.outpoint.txid.to_byte_array()
                        && cu.vout == canonical_utxo.outpoint.vout
                })
                .ok_or_else(|| {
                    StoreError::Sync(format!(
                        "canonical market outpoint {}:{} missing from chain view",
                        canonical_utxo.outpoint.txid, canonical_utxo.outpoint.vout
                    ))
                })?;

            if needs_entropy && !candidate_txids.contains(&chain_utxo.txid) {
                candidate_txids.push(chain_utxo.txid);
            }

            let inserted = upsert_market_chain_utxo(
                conn,
                &chain_utxo,
                spk,
                &row.market_id,
                canonical_utxo.slot,
            )?;
            if inserted {
                report.new_utxos += 1;
            }
        }

        update_market_state_from_scan(conn, row, &scan, report)?;

        if needs_entropy {
            for txid in candidate_txids {
                if try_extract_issuance_entropy(
                    conn,
                    chain,
                    &txid,
                    &row.market_id,
                    &candidate.yes_reissuance_token,
                    &candidate.no_reissuance_token,
                )? {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Try to extract issuance entropy from a transaction and store it in the market row.
/// Returns true if the full YES/NO issuance tuple is present after this call.
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

    let canonical: MarketRow = markets::table
        .filter(markets::market_id.eq(mid_bytes))
        .first(conn)?;

    // Only store if we found both YES and NO
    if let (Some(ye), Some(ne), Some(ybn), Some(nbn)) = (
        yes_entropy,
        no_entropy,
        yes_blinding_nonce,
        no_blinding_nonce,
    ) {
        diesel::update(
            market_candidates::table
                .filter(market_candidates::candidate_id.eq(canonical.candidate_id)),
        )
        .set((
            market_candidates::yes_issuance_entropy.eq(ye.to_vec()),
            market_candidates::no_issuance_entropy.eq(ne.to_vec()),
            market_candidates::yes_issuance_blinding_nonce.eq(ybn.to_vec()),
            market_candidates::no_issuance_blinding_nonce.eq(nbn.to_vec()),
        ))
        .execute(conn)?;
        return Ok(true);
    }

    // Also accept partial: store what we found (might find the other in a different tx)
    if yes_entropy.is_some() || no_entropy.is_some() {
        if let (Some(ye), Some(ybn)) = (yes_entropy, yes_blinding_nonce) {
            diesel::update(
                market_candidates::table
                    .filter(market_candidates::candidate_id.eq(canonical.candidate_id)),
            )
            .set((
                market_candidates::yes_issuance_entropy.eq(ye.to_vec()),
                market_candidates::yes_issuance_blinding_nonce.eq(ybn.to_vec()),
            ))
            .execute(conn)?;
        }
        if let (Some(ne), Some(nbn)) = (no_entropy, no_blinding_nonce) {
            diesel::update(
                market_candidates::table
                    .filter(market_candidates::candidate_id.eq(canonical.candidate_id)),
            )
            .set((
                market_candidates::no_issuance_entropy.eq(ne.to_vec()),
                market_candidates::no_issuance_blinding_nonce.eq(nbn.to_vec()),
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

/// Derive market state from the exact live slot set.
///
/// If no unspent market UTXOs exist, the stored state is left unchanged because
/// a market may be temporarily empty mid-transaction.
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
    market_slot: Option<MarketSlot>,
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
        market_slot: market_slot.map(|slot| slot.as_u8() as i32),
        block_height: cu.block_height.map(|h| h as i32),
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

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::QueryableByName;
    use diesel::sql_types::{BigInt, Nullable, Text};

    #[derive(Debug, QueryableByName)]
    struct PoolRow {
        #[diesel(sql_type = Text)]
        market_id: String,
        #[diesel(sql_type = Text)]
        creation_txid: String,
        #[diesel(sql_type = Text)]
        witness_schema_version: String,
        #[diesel(sql_type = BigInt)]
        current_s_index: i64,
        #[diesel(sql_type = BigInt)]
        reserve_yes: i64,
        #[diesel(sql_type = BigInt)]
        reserve_no: i64,
        #[diesel(sql_type = BigInt)]
        reserve_collateral: i64,
        #[diesel(sql_type = Text)]
        reserve_yes_outpoint: String,
        #[diesel(sql_type = Text)]
        reserve_no_outpoint: String,
        #[diesel(sql_type = Text)]
        reserve_collateral_outpoint: String,
        #[diesel(sql_type = Text)]
        state_source: String,
        #[diesel(sql_type = Nullable<Text>)]
        last_transition_txid: Option<String>,
        #[diesel(sql_type = Nullable<Text>)]
        nostr_event_id: Option<String>,
        #[diesel(sql_type = Nullable<Text>)]
        nostr_event_json: Option<String>,
    }

    fn sample_lmsr_pool_ingest() -> LmsrPoolIngestInput {
        LmsrPoolIngestInput {
            pool_id: "11".repeat(32),
            market_id: "22".repeat(32),
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            fee_bps: 30,
            cosigner_pubkey: [0x04; 32],
            lmsr_table_root: [0x05; 32],
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            creation_txid: "aa".repeat(32),
            witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".to_string(),
            current_s_index: 4,
            reserve_outpoints: [
                format!("{}:0", "aa".repeat(32)),
                format!("{}:1", "aa".repeat(32)),
                format!("{}:2", "aa".repeat(32)),
            ],
            reserve_yes: 500,
            reserve_no: 400,
            reserve_collateral: 1_000,
            state_source: deadcat_sdk::LmsrPoolStateSource::Announcement,
            last_transition_txid: None,
            nostr_event_id: Some("evt-1".to_string()),
            nostr_event_json: Some(r#"{"id":"evt-1"}"#.to_string()),
        }
    }

    fn sample_canonical_lmsr_pool_ingest() -> LmsrPoolIngestInput {
        let mut canonical = sample_lmsr_pool_ingest();
        canonical.current_s_index = 5;
        canonical.reserve_outpoints = [
            format!("{}:0", "bb".repeat(32)),
            format!("{}:1", "bb".repeat(32)),
            format!("{}:2", "bb".repeat(32)),
        ];
        canonical.reserve_yes = 450;
        canonical.reserve_no = 430;
        canonical.reserve_collateral = 1_020;
        canonical.state_source = deadcat_sdk::LmsrPoolStateSource::CanonicalScan;
        canonical.last_transition_txid = Some("bb".repeat(32));
        canonical.nostr_event_id = None;
        canonical.nostr_event_json = None;
        canonical
    }

    fn fetch_pool_row(store: &mut DeadcatStore, pool_id: &str) -> PoolRow {
        diesel::sql_query(
            "SELECT
                market_id,
                creation_txid,
                witness_schema_version,
                current_s_index,
                reserve_yes,
                reserve_no,
                reserve_collateral,
                reserve_yes_outpoint,
                reserve_no_outpoint,
                reserve_collateral_outpoint,
                state_source,
                last_transition_txid,
                nostr_event_id,
                nostr_event_json
             FROM lmsr_pools
             WHERE pool_id = ?",
        )
        .bind::<Text, _>(pool_id)
        .get_result(&mut store.conn)
        .unwrap()
    }

    #[test]
    fn ingest_lmsr_pool_preserves_existing_nostr_provenance_when_scan_omits_it() {
        let mut store = DeadcatStore::open_in_memory().unwrap();
        let initial = sample_lmsr_pool_ingest();
        store.ingest_lmsr_pool(&initial).unwrap();

        let canonical_scan = sample_canonical_lmsr_pool_ingest();

        store.ingest_lmsr_pool(&canonical_scan).unwrap();

        let row = fetch_pool_row(&mut store, &initial.pool_id);

        assert_eq!(row.market_id, initial.market_id);
        assert_eq!(row.creation_txid, initial.creation_txid);
        assert_eq!(row.witness_schema_version, initial.witness_schema_version);
        assert_eq!(row.current_s_index, canonical_scan.current_s_index as i64);
        assert_eq!(row.reserve_yes, canonical_scan.reserve_yes as i64);
        assert_eq!(row.reserve_no, canonical_scan.reserve_no as i64);
        assert_eq!(
            row.reserve_collateral,
            canonical_scan.reserve_collateral as i64
        );
        assert_eq!(
            row.reserve_yes_outpoint,
            canonical_scan.reserve_outpoints[0]
        );
        assert_eq!(row.reserve_no_outpoint, canonical_scan.reserve_outpoints[1]);
        assert_eq!(
            row.reserve_collateral_outpoint,
            canonical_scan.reserve_outpoints[2]
        );
        assert_eq!(
            row.state_source,
            deadcat_sdk::LmsrPoolStateSource::CanonicalScan.as_str()
        );
        assert_eq!(
            row.last_transition_txid.as_deref(),
            canonical_scan.last_transition_txid.as_deref()
        );
        assert_eq!(row.nostr_event_id.as_deref(), Some("evt-1"));
        assert_eq!(row.nostr_event_json.as_deref(), Some(r#"{"id":"evt-1"}"#));
    }

    #[test]
    fn ingest_lmsr_pool_updates_announcement_snapshot_with_new_announcement_state() {
        let mut store = DeadcatStore::open_in_memory().unwrap();
        let initial = sample_lmsr_pool_ingest();
        store.ingest_lmsr_pool(&initial).unwrap();

        let mut refreshed = sample_lmsr_pool_ingest();
        refreshed.current_s_index = 6;
        refreshed.reserve_yes = 490;
        refreshed.reserve_no = 410;
        refreshed.reserve_collateral = 1_010;
        refreshed.reserve_outpoints = [
            format!("{}:0", "cc".repeat(32)),
            format!("{}:1", "cc".repeat(32)),
            format!("{}:2", "cc".repeat(32)),
        ];
        refreshed.nostr_event_id = Some("evt-2".to_string());
        refreshed.nostr_event_json = Some(r#"{"id":"evt-2"}"#.to_string());

        store.ingest_lmsr_pool(&refreshed).unwrap();

        let row = fetch_pool_row(&mut store, &initial.pool_id);
        assert_eq!(row.market_id, initial.market_id);
        assert_eq!(row.creation_txid, initial.creation_txid);
        assert_eq!(row.witness_schema_version, initial.witness_schema_version);
        assert_eq!(row.current_s_index, refreshed.current_s_index as i64);
        assert_eq!(row.reserve_yes, refreshed.reserve_yes as i64);
        assert_eq!(row.reserve_no, refreshed.reserve_no as i64);
        assert_eq!(row.reserve_collateral, refreshed.reserve_collateral as i64);
        assert_eq!(row.reserve_yes_outpoint, refreshed.reserve_outpoints[0]);
        assert_eq!(row.reserve_no_outpoint, refreshed.reserve_outpoints[1]);
        assert_eq!(
            row.reserve_collateral_outpoint,
            refreshed.reserve_outpoints[2]
        );
        assert_eq!(
            row.state_source,
            deadcat_sdk::LmsrPoolStateSource::Announcement.as_str()
        );
        assert_eq!(row.last_transition_txid, None);
        assert_eq!(row.nostr_event_id.as_deref(), Some("evt-2"));
        assert_eq!(row.nostr_event_json.as_deref(), Some(r#"{"id":"evt-2"}"#));
    }

    #[test]
    fn ingest_lmsr_pool_preserves_canonical_state_when_later_announcement_arrives() {
        let mut store = DeadcatStore::open_in_memory().unwrap();
        let initial = sample_lmsr_pool_ingest();
        let canonical_scan = sample_canonical_lmsr_pool_ingest();
        let mut later_announcement = sample_lmsr_pool_ingest();
        later_announcement.nostr_event_id = Some("evt-2".to_string());
        later_announcement.nostr_event_json = Some(r#"{"id":"evt-2"}"#.to_string());

        store.ingest_lmsr_pool(&initial).unwrap();
        store.ingest_lmsr_pool(&canonical_scan).unwrap();
        store.ingest_lmsr_pool(&later_announcement).unwrap();

        let row = fetch_pool_row(&mut store, &initial.pool_id);
        assert_eq!(row.market_id, initial.market_id);
        assert_eq!(row.creation_txid, initial.creation_txid);
        assert_eq!(row.witness_schema_version, initial.witness_schema_version);
        assert_eq!(row.current_s_index, canonical_scan.current_s_index as i64);
        assert_eq!(row.reserve_yes, canonical_scan.reserve_yes as i64);
        assert_eq!(row.reserve_no, canonical_scan.reserve_no as i64);
        assert_eq!(
            row.reserve_collateral,
            canonical_scan.reserve_collateral as i64
        );
        assert_eq!(
            row.reserve_yes_outpoint,
            canonical_scan.reserve_outpoints[0]
        );
        assert_eq!(row.reserve_no_outpoint, canonical_scan.reserve_outpoints[1]);
        assert_eq!(
            row.reserve_collateral_outpoint,
            canonical_scan.reserve_outpoints[2]
        );
        assert_eq!(
            row.state_source,
            deadcat_sdk::LmsrPoolStateSource::CanonicalScan.as_str()
        );
        assert_eq!(
            row.last_transition_txid.as_deref(),
            canonical_scan.last_transition_txid.as_deref()
        );
        assert_eq!(row.nostr_event_id.as_deref(), Some("evt-2"));
        assert_eq!(row.nostr_event_json.as_deref(), Some(r#"{"id":"evt-2"}"#));
    }

    #[test]
    fn ingest_lmsr_pool_preserves_identity_when_later_announcement_has_different_market_id() {
        let mut store = DeadcatStore::open_in_memory().unwrap();
        let initial = sample_lmsr_pool_ingest();
        let mut conflicting = sample_lmsr_pool_ingest();
        conflicting.market_id = "33".repeat(32);
        conflicting.nostr_event_id = Some("evt-3".to_string());
        conflicting.nostr_event_json = Some(r#"{"id":"evt-3"}"#.to_string());

        store.ingest_lmsr_pool(&initial).unwrap();
        store.ingest_lmsr_pool(&conflicting).unwrap();

        let row = fetch_pool_row(&mut store, &initial.pool_id);
        assert_eq!(row.market_id, initial.market_id);
        assert_eq!(row.creation_txid, initial.creation_txid);
        assert_eq!(row.witness_schema_version, initial.witness_schema_version);
        assert_eq!(row.current_s_index, conflicting.current_s_index as i64);
        assert_eq!(row.nostr_event_id.as_deref(), Some("evt-3"));
    }

    #[test]
    fn ingest_lmsr_pool_preserves_canonical_identity_when_later_announcement_has_different_market_id()
     {
        let mut store = DeadcatStore::open_in_memory().unwrap();
        let initial = sample_lmsr_pool_ingest();
        let canonical_scan = sample_canonical_lmsr_pool_ingest();
        let mut conflicting = sample_lmsr_pool_ingest();
        conflicting.market_id = "33".repeat(32);
        conflicting.nostr_event_id = Some("evt-3".to_string());
        conflicting.nostr_event_json = Some(r#"{"id":"evt-3"}"#.to_string());

        store.ingest_lmsr_pool(&initial).unwrap();
        store.ingest_lmsr_pool(&canonical_scan).unwrap();
        store.ingest_lmsr_pool(&conflicting).unwrap();

        let row = fetch_pool_row(&mut store, &initial.pool_id);
        assert_eq!(row.market_id, initial.market_id);
        assert_eq!(row.creation_txid, initial.creation_txid);
        assert_eq!(row.witness_schema_version, initial.witness_schema_version);
        assert_eq!(row.current_s_index, canonical_scan.current_s_index as i64);
        assert_eq!(
            row.state_source,
            deadcat_sdk::LmsrPoolStateSource::CanonicalScan.as_str()
        );
        assert_eq!(row.nostr_event_id.as_deref(), Some("evt-3"));
    }
}
