PRAGMA foreign_keys = OFF;

DROP TABLE utxos;
DROP TABLE markets;
DROP TABLE market_candidates;

CREATE TABLE markets (
    market_id BLOB NOT NULL PRIMARY KEY,                   -- 32 bytes: SHA256(yes_token || no_token)
    oracle_public_key BLOB NOT NULL,                       -- 32 bytes
    collateral_asset_id BLOB NOT NULL,                     -- 32 bytes
    yes_token_asset BLOB NOT NULL,                         -- 32 bytes
    no_token_asset BLOB NOT NULL,                          -- 32 bytes
    yes_reissuance_token BLOB NOT NULL,                    -- 32 bytes
    no_reissuance_token BLOB NOT NULL,                     -- 32 bytes
    collateral_per_token BIGINT NOT NULL,                  -- u64 stored as i64
    expiry_time INTEGER NOT NULL,                          -- u32
    cmr BLOB NOT NULL,                                     -- 32 bytes, Commitment Merkle Root
    dormant_yes_rt_spk BLOB NOT NULL,
    dormant_no_rt_spk BLOB NOT NULL,
    unresolved_yes_rt_spk BLOB NOT NULL,
    unresolved_no_rt_spk BLOB NOT NULL,
    unresolved_collateral_spk BLOB NOT NULL,
    resolved_yes_collateral_spk BLOB NOT NULL,
    resolved_no_collateral_spk BLOB NOT NULL,
    expired_collateral_spk BLOB NOT NULL,
    current_state INTEGER NOT NULL DEFAULT 0,              -- MarketState as u64
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    yes_issuance_entropy BLOB,
    no_issuance_entropy BLOB,
    yes_issuance_blinding_nonce BLOB,
    no_issuance_blinding_nonce BLOB,
    question TEXT,
    description TEXT,
    category TEXT,
    resolution_source TEXT,
    creator_pubkey BLOB,
    creation_txid TEXT NOT NULL,
    yes_dormant_asset_blinding_factor BLOB NOT NULL,
    yes_dormant_value_blinding_factor BLOB NOT NULL,
    no_dormant_asset_blinding_factor BLOB NOT NULL,
    no_dormant_value_blinding_factor BLOB NOT NULL,
    nevent TEXT,
    nostr_event_id TEXT,
    nostr_event_json TEXT,
    dormant_txid TEXT,
    unresolved_txid TEXT,
    resolved_yes_txid TEXT,
    resolved_no_txid TEXT,
    expired_txid TEXT
);

CREATE INDEX idx_markets_oracle_public_key ON markets (oracle_public_key);
CREATE INDEX idx_markets_collateral_asset_id ON markets (collateral_asset_id);
CREATE INDEX idx_markets_current_state ON markets (current_state);
CREATE INDEX idx_markets_expiry_time ON markets (expiry_time);

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
    market_slot INTEGER,
    spent INTEGER NOT NULL DEFAULT 0,
    spending_txid BLOB,
    block_height INTEGER,
    spent_block_height INTEGER,
    PRIMARY KEY (txid, vout)
);

CREATE INDEX idx_utxos_market_id ON utxos (market_id);
CREATE INDEX idx_utxos_maker_order_id ON utxos (maker_order_id);
CREATE INDEX idx_utxos_spent ON utxos (spent);
CREATE INDEX idx_utxos_script_pubkey ON utxos (script_pubkey);
CREATE INDEX idx_utxos_market_slot_spent ON utxos (market_id, market_slot, spent);

UPDATE maker_orders
SET
    order_status = 0,
    updated_at = datetime('now');

UPDATE sync_state
SET
    last_block_hash = NULL,
    last_block_height = 0,
    updated_at = datetime('now')
WHERE id = 1;

PRAGMA foreign_keys = ON;
