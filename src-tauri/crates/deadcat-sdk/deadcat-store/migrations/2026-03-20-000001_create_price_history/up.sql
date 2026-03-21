CREATE TABLE IF NOT EXISTS lmsr_price_history (
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
    block_height INTEGER
);
CREATE INDEX IF NOT EXISTS idx_price_history_market ON lmsr_price_history(market_id, recorded_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_price_history_txid ON lmsr_price_history(transition_txid);
