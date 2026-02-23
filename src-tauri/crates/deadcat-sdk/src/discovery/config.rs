use std::time::Duration;

use super::DEFAULT_RELAYS;

/// Configuration for the `DiscoveryService`.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Nostr relay URLs to connect to.
    pub relays: Vec<String>,
    /// Network tag value (e.g. "liquid-testnet").
    pub network_tag: String,
    /// Timeout for one-shot fetch operations.
    pub fetch_timeout: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            relays: DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
            network_tag: super::NETWORK_TAG.to_string(),
            fetch_timeout: Duration::from_secs(15),
        }
    }
}
