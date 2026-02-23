use crate::maker_order::params::MakerOrderParams;
use crate::params::ContractParams;

/// Metadata passed alongside a market when persisting to the store.
#[derive(Debug, Clone, Default)]
pub struct ContractMetadataInput {
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub starting_yes_price: Option<u8>,
    pub creator_pubkey: Option<Vec<u8>>,
    pub creation_txid: Option<String>,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

/// Trait abstracting store operations needed by `DiscoveryService`.
///
/// This avoids a circular dependency between `deadcat-sdk` and `deadcat-store`.
/// The `deadcat-store` crate implements this trait for `DeadcatStore`.
pub trait DiscoveryStore: Send + 'static {
    /// Persist a discovered market. If it already exists, this should be a no-op.
    fn ingest_market(
        &mut self,
        params: &ContractParams,
        meta: Option<&ContractMetadataInput>,
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
}
