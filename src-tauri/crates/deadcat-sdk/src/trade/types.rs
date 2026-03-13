use serde::{Deserialize, Serialize};

use crate::lmsr_pool::math::LmsrTradeKind;
use crate::lmsr_pool::params::LmsrPoolParams;
use crate::maker_order::params::MakerOrderParams;
use crate::pool::PoolReserves;
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
    LmsrPool {
        pool_id: String,
        old_s_index: u64,
        new_s_index: u64,
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
    /// LMSR pool swap leg (None if route does not include an LMSR pool).
    pub lmsr_pool_leg: Option<LmsrPoolSwapLeg>,
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

/// The LMSR pool swap leg of the execution plan.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LmsrPoolSwapLeg {
    pub primary_path: LmsrPrimaryPath,
    pub pool_params: LmsrPoolParams,
    pub pool_id: String,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub old_path_bits: u64,
    pub new_path_bits: u64,
    pub old_siblings: Vec<[u8; 32]>,
    pub new_siblings: Vec<[u8; 32]>,
    pub in_base: u32,
    pub out_base: u32,
    pub pool_utxos: LmsrPoolUtxos,
    pub trade_kind: LmsrTradeKind,
    pub old_f: u64,
    pub new_f: u64,
    /// Amount the taker deposits into the pool.
    pub delta_in: u64,
    /// Amount the taker receives from the pool.
    pub delta_out: u64,
    /// BIP340 signature for PATH_PRIMARY=admin; ignored on swap path.
    pub admin_signature: [u8; 64],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum LmsrPrimaryPath {
    Swap,
    AdminAdjust,
}

/// The three reserve UTXOs that make up an LMSR pool bundle.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LmsrPoolUtxos {
    pub yes: UnblindedUtxo,
    pub no: UnblindedUtxo,
    pub collateral: UnblindedUtxo,
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
