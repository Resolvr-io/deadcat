use diesel::prelude::*;

use crate::schema::utxos;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = utxos)]
pub struct UtxoRow {
    pub txid: Vec<u8>,
    pub vout: i32,
    pub script_pubkey: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub value: i64,
    pub asset_blinding_factor: Vec<u8>,
    pub value_blinding_factor: Vec<u8>,
    pub raw_txout: Vec<u8>,
    pub market_id: Option<Vec<u8>>,
    pub maker_order_id: Option<i32>,
    pub market_state: Option<i32>,
    pub spent: i32,
    pub spending_txid: Option<Vec<u8>>,
    pub block_height: Option<i32>,
    pub spent_block_height: Option<i32>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = utxos)]
pub struct NewUtxoRow {
    pub txid: Vec<u8>,
    pub vout: i32,
    pub script_pubkey: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub value: i64,
    pub asset_blinding_factor: Vec<u8>,
    pub value_blinding_factor: Vec<u8>,
    pub raw_txout: Vec<u8>,
    pub market_id: Option<Vec<u8>>,
    pub maker_order_id: Option<i32>,
    pub market_state: Option<i32>,
    pub block_height: Option<i32>,
}
