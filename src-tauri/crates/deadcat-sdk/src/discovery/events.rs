use super::attestation::AttestationContent;
use super::market::DiscoveredMarket;
use super::DiscoveredOrder;

/// Events emitted by the `DiscoveryService` subscription loop.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new market announcement was received.
    MarketDiscovered(DiscoveredMarket),
    /// A new limit order announcement was received.
    OrderDiscovered(DiscoveredOrder),
    /// An oracle attestation was received.
    AttestationDiscovered(AttestationContent),
}
