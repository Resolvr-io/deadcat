use diesel::prelude::*;

use crate::schema::markets;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = markets)]
#[allow(dead_code)]
pub struct MarketRow {
    pub market_id: Vec<u8>,
    pub candidate_id: i32,
    pub current_state: i32,
    pub created_at: String,
    pub updated_at: String,
    pub dormant_txid: Option<String>,
    pub unresolved_txid: Option<String>,
    pub resolved_yes_txid: Option<String>,
    pub resolved_no_txid: Option<String>,
    pub expired_txid: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = markets)]
pub struct NewMarketRow {
    pub market_id: Vec<u8>,
    pub candidate_id: i32,
    pub current_state: i32,
    pub dormant_txid: Option<String>,
    pub unresolved_txid: Option<String>,
    pub resolved_yes_txid: Option<String>,
    pub resolved_no_txid: Option<String>,
    pub expired_txid: Option<String>,
}
