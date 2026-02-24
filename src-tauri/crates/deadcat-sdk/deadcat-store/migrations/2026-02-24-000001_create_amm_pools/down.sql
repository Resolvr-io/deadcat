-- SQLite doesn't support DROP COLUMN, so we recreate the utxos table without amm_pool_id.
-- This is only used in development/testing; production migrations are forward-only.

CREATE TABLE utxos_backup AS SELECT
    txid, vout, script_pubkey, asset_id, value,
    asset_blinding_factor, value_blinding_factor, raw_txout,
    market_id, maker_order_id, market_state,
    spent, spending_txid, block_height, spent_block_height
FROM utxos;

DROP TABLE utxos;

CREATE TABLE utxos (
    txid BLOB NOT NULL,
    vout INTEGER NOT NULL,
    script_pubkey BLOB NOT NULL,
    asset_id BLOB NOT NULL,
    value BIGINT NOT NULL,
    asset_blinding_factor BLOB NOT NULL,
    value_blinding_factor BLOB NOT NULL,
    raw_txout BLOB NOT NULL,
    market_id BLOB REFERENCES markets(market_id),
    maker_order_id INTEGER REFERENCES maker_orders(id),
    market_state INTEGER,
    spent INTEGER NOT NULL DEFAULT 0,
    spending_txid BLOB,
    block_height INTEGER,
    spent_block_height INTEGER,
    PRIMARY KEY (txid, vout)
);

INSERT INTO utxos SELECT * FROM utxos_backup;
DROP TABLE utxos_backup;

-- Recreate indexes that existed before this migration
CREATE INDEX IF NOT EXISTS idx_utxos_market_id ON utxos (market_id);
CREATE INDEX IF NOT EXISTS idx_utxos_maker_order_id ON utxos (maker_order_id);
CREATE INDEX IF NOT EXISTS idx_utxos_spent ON utxos (spent);
CREATE INDEX IF NOT EXISTS idx_utxos_script_pubkey ON utxos (script_pubkey);
CREATE INDEX IF NOT EXISTS idx_utxos_market_state_spent ON utxos (market_id, market_state, spent);

DROP TABLE IF EXISTS amm_pools;
