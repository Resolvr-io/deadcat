use serde::{Deserialize, Serialize};

use crate::amm_pool::math::{PoolReserves, SwapPair};
use crate::amm_pool::params::AmmPoolParams;
use crate::maker_order::params::MakerOrderParams;
use crate::pset::UnblindedUtxo;

// ── Public trade request types ──────────────────────────────────────────

/// Which outcome token the trade targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeSide {
    Yes,
    No,
}

/// Whether the taker is buying or selling the outcome token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeDirection {
    /// Taker sends collateral (L-BTC), receives outcome tokens.
    Buy,
    /// Taker sends outcome tokens, receives collateral (L-BTC).
    Sell,
}

/// How the trade amount is specified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeAmount {
    /// Taker specifies the exact amount they send.
    ///
    /// Buy: exact collateral to spend. Sell: exact tokens to sell.
    ExactInput(u64),
    /// Taker specifies the exact amount they want to receive.
    ///
    /// Not yet supported — returns an error.
    ExactOutput(u64),
}

// ── Quote types (inspectable, not externally constructable) ─────────────

/// A fully-routed trade quote.
///
/// All display fields are `pub` for inspection. The execution plan is
/// `pub(crate)` — callers cannot construct a `TradeQuote` directly;
/// it can only be obtained from [`crate::node::DeadcatNode::quote_trade`] and consumed
/// by [`crate::node::DeadcatNode::execute_trade`].
#[derive(Debug, Clone)]
pub struct TradeQuote {
    pub side: TradeSide,
    pub direction: TradeDirection,
    pub amount: TradeAmount,
    /// Total amount the taker sends (collateral for Buy, tokens for Sell).
    pub total_input: u64,
    /// Total amount the taker receives (tokens for Buy, collateral for Sell).
    pub total_output: u64,
    /// Effective price: total_input / total_output.
    pub effective_price: f64,
    /// Route breakdown for display.
    pub legs: Vec<RouteLeg>,

    /// Internal execution plan (only visible within the crate).
    pub(crate) plan: ExecutionPlan,
}

/// A single leg of a routed trade, for display purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteLeg {
    pub source: LiquiditySource,
    /// Amount consumed from the taker's send asset.
    pub input_amount: u64,
    /// Amount delivered to the taker's receive asset.
    pub output_amount: u64,
}

/// Where a route leg sources its liquidity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LiquiditySource {
    AmmPool {
        pool_id: String,
    },
    LimitOrder {
        order_id: String,
        price: u64,
        lots: u64,
    },
}

// ── Execution plan (crate-internal) ─────────────────────────────────────

/// Complete plan for executing a routed trade. Contains all the data
/// needed to build and broadcast the combined PSET.
#[derive(Debug, Clone)]
pub(crate) struct ExecutionPlan {
    /// Limit orders to fill, ordered by price (cheapest first for Buy,
    /// most-expensive first for Sell).
    pub order_legs: Vec<OrderFillLeg>,
    /// AMM pool swap leg (None if the entire trade is filled by orders).
    pub pool_leg: Option<PoolSwapLeg>,
    /// Asset the taker sends.
    pub taker_send_asset: [u8; 32],
    /// Asset the taker receives.
    pub taker_receive_asset: [u8; 32],
    /// Total amount the taker sends.
    pub total_taker_input: u64,
    /// Total amount the taker receives.
    pub total_taker_output: u64,
    /// Pool reserves at quote time, for optional staleness checking at execution.
    #[allow(dead_code)] // reserved for future staleness check
    pub quoted_reserves: Option<PoolReserves>,
}

/// A single limit order to fill as part of the execution plan.
#[derive(Debug, Clone)]
pub(crate) struct OrderFillLeg {
    pub params: MakerOrderParams,
    pub maker_base_pubkey: [u8; 32],
    pub order_nonce: [u8; 32],
    pub order_utxo: UnblindedUtxo,
    /// Number of lots to fill from this order.
    pub lots: u64,
    /// Amount the taker pays into this order (taker's send asset).
    pub taker_pays: u64,
    /// Amount the taker receives from this order (taker's receive asset).
    pub taker_receives: u64,
    /// Amount sent to the maker's receive address.
    pub maker_receive_amount: u64,
    /// Whether this fill consumes only part of the order.
    pub is_partial: bool,
    /// Remaining value in the covenant after a partial fill.
    pub remainder_value: u64,
}

/// The AMM pool swap leg of the execution plan.
#[derive(Debug, Clone)]
pub(crate) struct PoolSwapLeg {
    pub pool_params: AmmPoolParams,
    pub issued_lp: u64,
    pub pool_utxos: PoolUtxos,
    pub swap_pair: SwapPair,
    #[allow(dead_code)] // kept for API completeness; not consumed by witness path
    pub sell_a: bool,
    /// Amount the taker deposits into the pool.
    pub delta_in: u64,
    /// Amount the taker receives from the pool.
    pub delta_out: u64,
    /// Reserves after the swap.
    pub new_reserves: PoolReserves,
}

/// The four UTXOs that make up the AMM pool covenant.
#[derive(Debug, Clone)]
pub(crate) struct PoolUtxos {
    pub yes: UnblindedUtxo,
    pub no: UnblindedUtxo,
    pub lbtc: UnblindedUtxo,
    pub rt: UnblindedUtxo,
}

// ── Result types ────────────────────────────────────────────────────────

/// Result of executing a trade.
#[derive(Debug, Clone)]
pub struct TradeResult {
    pub txid: lwk_wollet::elements::Txid,
    pub total_input: u64,
    pub total_output: u64,
    pub num_orders_filled: usize,
    pub pool_used: bool,
    pub new_reserves: Option<PoolReserves>,
}
