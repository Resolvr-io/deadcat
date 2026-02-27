-- Pool state history: one row per on-chain state transition
CREATE TABLE pool_state_snapshots (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    pool_id       BLOB    NOT NULL REFERENCES amm_pools(pool_id),
    txid          BLOB    NOT NULL,      -- 32 bytes, the tx that produced this state
    r_yes         BIGINT  NOT NULL,
    r_no          BIGINT  NOT NULL,
    r_lbtc        BIGINT  NOT NULL,
    issued_lp     BIGINT  NOT NULL,
    block_height  INTEGER,               -- NULL until confirmed
    created_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE(pool_id, txid)
);

CREATE INDEX idx_pool_snapshots_pool_id ON pool_state_snapshots(pool_id);
CREATE INDEX idx_pool_snapshots_block_height ON pool_state_snapshots(pool_id, block_height);

-- Add market_id + creation_txid to amm_pools (links pool to market)
ALTER TABLE amm_pools ADD COLUMN market_id BLOB;
ALTER TABLE amm_pools ADD COLUMN creation_txid BLOB;

CREATE INDEX idx_amm_pools_market_id ON amm_pools(market_id);

-- Add per-state txids to markets for validation
ALTER TABLE markets ADD COLUMN dormant_txid TEXT;
ALTER TABLE markets ADD COLUMN unresolved_txid TEXT;
ALTER TABLE markets ADD COLUMN resolved_yes_txid TEXT;
ALTER TABLE markets ADD COLUMN resolved_no_txid TEXT;
