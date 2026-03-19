use crate::maker_order::params::MakerOrderParams;
use crate::prediction_market::anchor::PredictionMarketAnchor;
use crate::prediction_market::params::PredictionMarketParams;

/// Human/provenance metadata passed alongside a prediction-market candidate.
#[derive(Debug, Clone)]
pub struct ContractMetadataInput {
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub creator_pubkey: Option<Vec<u8>>,
    /// Canonical dormant-anchor tuple for the market bootstrap.
    pub anchor: PredictionMarketAnchor,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

/// Level-2-valid prediction-market candidate persisted by discovery ingestion.
#[derive(Debug, Clone)]
pub struct PredictionMarketCandidateIngestInput {
    pub params: PredictionMarketParams,
    pub metadata: ContractMetadataInput,
    pub creation_tx: Vec<u8>,
}

/// LMSR pool metadata plus the latest known state snapshot.
///
/// Announcement ingests may seed missing rows and attach discovery provenance,
/// but they are advisory. Once a row has canonical scan state, later
/// announcement ingests must not replace the canonical `current_s_index`,
/// reserves, reserve outpoints, or transition provenance, and must not relabel
/// immutable pool identity such as `market_id`.
#[derive(Debug, Clone)]
pub struct LmsrPoolIngestInput {
    pub pool_id: String,
    pub market_id: String,
    pub yes_asset_id: [u8; 32],
    pub no_asset_id: [u8; 32],
    pub collateral_asset_id: [u8; 32],
    pub fee_bps: u64,
    pub cosigner_pubkey: [u8; 32],
    pub lmsr_table_root: [u8; 32],
    pub table_depth: u32,
    pub q_step_lots: u64,
    pub s_bias: u64,
    pub s_max_index: u64,
    pub half_payout_sats: u64,
    pub creation_txid: String,
    pub witness_schema_version: String,
    pub current_s_index: u64,
    pub reserve_outpoints: [String; 3],
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
    pub state_source: LmsrPoolStateSource,
    pub last_transition_txid: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LmsrPoolStateSource {
    Announcement,
    CanonicalScan,
}

impl LmsrPoolStateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            LmsrPoolStateSource::Announcement => "announcement",
            LmsrPoolStateSource::CanonicalScan => "canonical_scan",
        }
    }
}

/// Canonical LMSR live-state update produced by chain scan.
#[derive(Debug, Clone)]
pub struct LmsrPoolStateUpdateInput {
    pub pool_id: String,
    pub current_s_index: u64,
    pub reserve_outpoints: [String; 3],
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
    pub last_transition_txid: Option<String>,
}

/// Trait abstracting store operations needed by `DiscoveryService`.
///
/// This avoids a circular dependency between `deadcat-sdk` and `deadcat-store`.
/// The `deadcat-store` crate implements this trait for `DeadcatStore`.
pub trait DiscoveryStore: Send + 'static {
    /// Persist a level-2-valid prediction-market candidate.
    fn ingest_prediction_market_candidate(
        &mut self,
        input: &PredictionMarketCandidateIngestInput,
        seen_at_unix: u64,
    ) -> Result<(), String>;

    /// Persist a discovered maker order. If it already exists, this should be a no-op.
    fn ingest_maker_order(
        &mut self,
        params: &MakerOrderParams,
        maker_pubkey: Option<&[u8; 32]>,
        nonce: Option<&[u8; 32]>,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
    ) -> Result<(), String>;

    /// Persist a discovered LMSR pool/state snapshot.
    ///
    /// Store implementations must treat announcement snapshots as advisory:
    /// they may create a missing row and enrich provenance later, but they
    /// must not overwrite canonical scan state or immutable pool identity once
    /// present.
    fn ingest_lmsr_pool(&mut self, input: &LmsrPoolIngestInput) -> Result<(), String>;

    /// Persist canonical LMSR live-state produced by chain scan.
    fn upsert_lmsr_pool_state(&mut self, input: &LmsrPoolStateUpdateInput) -> Result<(), String>;
}
