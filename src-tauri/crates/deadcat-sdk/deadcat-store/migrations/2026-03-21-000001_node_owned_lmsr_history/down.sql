DROP INDEX IF EXISTS idx_price_history_pool_txid;
DROP INDEX IF EXISTS idx_price_history_pool_height;
DROP INDEX IF EXISTS idx_price_history_market_height;

CREATE TABLE lmsr_price_history_old (
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

INSERT INTO lmsr_price_history_old (
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
FROM lmsr_price_history;

DROP TABLE lmsr_price_history;
ALTER TABLE lmsr_price_history_old RENAME TO lmsr_price_history;

CREATE INDEX IF NOT EXISTS idx_price_history_market
ON lmsr_price_history(market_id, recorded_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_price_history_txid
ON lmsr_price_history(transition_txid);

CREATE TABLE lmsr_pools_old (
    pool_id TEXT NOT NULL PRIMARY KEY,
    market_id TEXT NOT NULL,
    creation_txid TEXT NOT NULL,
    witness_schema_version TEXT NOT NULL,
    current_s_index BIGINT NOT NULL,
    reserve_yes BIGINT NOT NULL,
    reserve_no BIGINT NOT NULL,
    reserve_collateral BIGINT NOT NULL,
    reserve_yes_outpoint TEXT NOT NULL,
    reserve_no_outpoint TEXT NOT NULL,
    reserve_collateral_outpoint TEXT NOT NULL,
    state_source TEXT NOT NULL DEFAULT 'announcement',
    last_transition_txid TEXT,
    params_json TEXT NOT NULL,
    nostr_event_id TEXT,
    nostr_event_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO lmsr_pools_old (
    pool_id,
    market_id,
    creation_txid,
    witness_schema_version,
    current_s_index,
    reserve_yes,
    reserve_no,
    reserve_collateral,
    reserve_yes_outpoint,
    reserve_no_outpoint,
    reserve_collateral_outpoint,
    state_source,
    last_transition_txid,
    params_json,
    nostr_event_id,
    nostr_event_json,
    created_at,
    updated_at
)
SELECT
    pool_id,
    market_id,
    creation_txid,
    witness_schema_version,
    current_s_index,
    reserve_yes,
    reserve_no,
    reserve_collateral,
    reserve_yes_outpoint,
    reserve_no_outpoint,
    reserve_collateral_outpoint,
    state_source,
    last_transition_txid,
    params_json,
    nostr_event_id,
    nostr_event_json,
    created_at,
    updated_at
FROM lmsr_pools;

DROP TABLE lmsr_pools;
ALTER TABLE lmsr_pools_old RENAME TO lmsr_pools;

CREATE INDEX idx_lmsr_pools_market_id ON lmsr_pools (market_id);
