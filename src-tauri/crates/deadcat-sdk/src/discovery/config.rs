use std::time::Duration;

use nostr_sdk::PublicKey;

use super::DEFAULT_RELAYS;

/// Configuration for the `DiscoveryService`.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Nostr relay URLs to connect to.
    pub relays: Vec<String>,
    /// Network tag value (e.g. "liquid-testnet").
    pub network_tag: String,
    /// Timeout for relay connection attempts. Relays that don't respond
    /// within this window are skipped — the app proceeds with whichever
    /// relays connected in time.
    pub connect_timeout: Duration,
    /// Timeout for one-shot fetch operations.
    pub fetch_timeout: Duration,
    /// When set, only discover markets published by this author.
    pub source_author: Option<PublicKey>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            relays: DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
            network_tag: super::NETWORK_TAG.to_string(),
            connect_timeout: Duration::from_secs(3),
            fetch_timeout: Duration::from_secs(5),
            source_author: None,
        }
    }
}
