use crate::state::MarketState;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("contract compilation failed: {0}")]
    Compilation(String),

    #[error("insufficient collateral for requested operation")]
    InsufficientCollateral,

    #[error("invalid market state")]
    InvalidState,

    #[error("token amount must be a multiple of collateral_per_token")]
    InvalidCollateralMultiple,

    #[error("invalid oracle outcome (must be YES or NO)")]
    InvalidOracleOutcome,

    #[error("collateral calculation overflow")]
    CollateralOverflow,

    #[error("PSET construction error: {0}")]
    Pset(String),

    #[error("full cancellation requires reissuance token UTXOs")]
    MissingReissuanceUtxos,

    #[error("insufficient fee: defining UTXOs don't cover fee_amount")]
    InsufficientFee,

    #[error("fill amount below minimum fill lots")]
    FillBelowMinimum,

    #[error("remainder amount below minimum remainder lots")]
    RemainderBelowMinimum,

    #[error("order amount is zero")]
    ZeroOrderAmount,

    #[error("price must be non-zero")]
    ZeroPrice,

    #[error("conservation check failed: payment does not match expected")]
    ConservationViolation,

    #[error("only the last order in a batch may be partially filled")]
    PartialFillNotLast,

    #[error("arithmetic overflow in maker order calculation")]
    MakerOrderOverflow,

    #[error("excess UTXO value with no change destination provided")]
    MissingChangeDestination,

    #[error("signer error: {0}")]
    Signer(String),

    #[error("descriptor error: {0}")]
    Descriptor(String),

    #[error("wallet initialization error: {0}")]
    WalletInit(String),

    #[error("electrum error: {0}")]
    Electrum(String),

    #[error("query error: {0}")]
    Query(String),

    #[error("finalize error: {0}")]
    Finalize(String),

    #[error("broadcast error: {0}")]
    Broadcast(String),

    #[error("blinding error: {0}")]
    Blinding(String),

    #[error("insufficient UTXOs: {0}")]
    InsufficientUtxos(String),

    #[error("covenant UTXO scanning failed: {0}")]
    CovenantScan(String),

    #[error("cannot unblind covenant UTXO: {0}")]
    Unblind(String),

    #[error("market not in issuable state (found {0:?})")]
    NotIssuable(MarketState),

    #[error("market not in redeemable state (found {0:?})")]
    NotRedeemable(MarketState),

    #[error("market not in cancellable state (found {0:?})")]
    NotCancellable(MarketState),

    #[error("market not in resolvable state (found {0:?})")]
    NotResolvable(MarketState),

    #[error("witness satisfaction failed: {0}")]
    Witness(String),

    #[error("maker order error: {0}")]
    MakerOrder(String),
}

pub type Result<T> = std::result::Result<T, Error>;
