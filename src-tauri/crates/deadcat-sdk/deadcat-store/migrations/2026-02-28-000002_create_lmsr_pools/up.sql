CREATE TABLE lmsr_pools (
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

CREATE INDEX idx_lmsr_pools_market_id ON lmsr_pools (market_id);
