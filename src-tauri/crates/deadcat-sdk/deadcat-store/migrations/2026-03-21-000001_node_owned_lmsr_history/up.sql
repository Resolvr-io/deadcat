ALTER TABLE lmsr_pools
ADD COLUMN lmsr_table_values_json TEXT;

ALTER TABLE lmsr_pools
ADD COLUMN initial_reserve_yes_outpoint TEXT;

ALTER TABLE lmsr_pools
ADD COLUMN initial_reserve_no_outpoint TEXT;

ALTER TABLE lmsr_pools
ADD COLUMN initial_reserve_collateral_outpoint TEXT;

CREATE TABLE lmsr_price_history_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pool_id TEXT NOT NULL,
    market_id TEXT NOT NULL,
    transition_txid TEXT NOT NULL,
    old_s_index INTEGER NOT NULL,
    new_s_index INTEGER NOT NULL,
    reserve_yes INTEGER NOT NULL,
    reserve_no INTEGER NOT NULL,
    reserve_collateral INTEGER NOT NULL,
    implied_yes_price_bps INTEGER NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    block_height INTEGER NOT NULL
);

INSERT INTO lmsr_price_history_new (
    id,
    pool_id,
    market_id,
    transition_txid,
    old_s_index,
    new_s_index,
    reserve_yes,
    reserve_no,
    reserve_collateral,
    implied_yes_price_bps,
    recorded_at,
    block_height
)
SELECT
    id,
    pool_id,
    market_id,
    transition_txid,
    old_s_index,
    new_s_index,
    reserve_yes,
    reserve_no,
    reserve_collateral,
    implied_yes_price_bps,
    recorded_at,
    block_height
FROM lmsr_price_history
WHERE block_height IS NOT NULL;

DROP TABLE lmsr_price_history;
ALTER TABLE lmsr_price_history_new RENAME TO lmsr_price_history;

CREATE INDEX IF NOT EXISTS idx_price_history_market_height
ON lmsr_price_history(market_id, block_height);

CREATE INDEX IF NOT EXISTS idx_price_history_pool_height
ON lmsr_price_history(pool_id, block_height);

CREATE UNIQUE INDEX IF NOT EXISTS idx_price_history_pool_txid
ON lmsr_price_history(pool_id, transition_txid);
