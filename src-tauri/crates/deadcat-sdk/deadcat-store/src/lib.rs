mod conversions;
mod error;
mod models;
mod schema;
mod store;
mod sync;

pub use deadcat_sdk::DiscoveredMarketMetadata;
pub use error::StoreError;
pub use store::{
    AmmPoolInfo, DeadcatStore, IssuanceData, MakerOrderInfo, MarketFilter, MarketInfo, OrderFilter,
    OrderStatus, PoolStateSnapshotInfo, PoolStatus,
};
pub use sync::{ChainSource, ChainUtxo, MarketStateChange, OrderStatusChange, SyncReport};

pub type Result<T> = std::result::Result<T, StoreError>;
