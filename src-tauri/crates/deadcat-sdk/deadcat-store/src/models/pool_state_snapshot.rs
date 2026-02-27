use diesel::prelude::*;

use crate::schema::pool_state_snapshots;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = pool_state_snapshots)]
pub struct PoolStateSnapshotRow {
    pub id: i32,
    pub pool_id: Vec<u8>,
    pub txid: Vec<u8>,
    pub r_yes: i64,
    pub r_no: i64,
    pub r_lbtc: i64,
    pub issued_lp: i64,
    pub block_height: Option<i32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = pool_state_snapshots)]
pub struct NewPoolStateSnapshotRow {
    pub pool_id: Vec<u8>,
    pub txid: Vec<u8>,
    pub r_yes: i64,
    pub r_no: i64,
    pub r_lbtc: i64,
    pub issued_lp: i64,
    pub block_height: Option<i32>,
}
