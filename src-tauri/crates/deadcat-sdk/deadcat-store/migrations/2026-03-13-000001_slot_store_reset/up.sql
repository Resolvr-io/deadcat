PRAGMA foreign_keys = OFF;

DROP TABLE utxos;
DROP TABLE IF EXISTS market_candidates;
DROP TABLE markets;

CREATE TABLE market_candidates (
    candidate_id INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id BLOB NOT NULL,
    oracle_public_key BLOB NOT NULL,
    collateral_asset_id BLOB NOT NULL,
    yes_token_asset BLOB NOT NULL,
    no_token_asset BLOB NOT NULL,
    yes_reissuance_token BLOB NOT NULL,
    no_reissuance_token BLOB NOT NULL,
    collateral_per_token BIGINT NOT NULL,
    expiry_time INTEGER NOT NULL,
    cmr BLOB NOT NULL,
    dormant_yes_rt_spk BLOB NOT NULL,
    dormant_no_rt_spk BLOB NOT NULL,
    unresolved_yes_rt_spk BLOB NOT NULL,
    unresolved_no_rt_spk BLOB NOT NULL,
    unresolved_collateral_spk BLOB NOT NULL,
    resolved_yes_collateral_spk BLOB NOT NULL,
    resolved_no_collateral_spk BLOB NOT NULL,
    expired_collateral_spk BLOB NOT NULL,
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
    creation_tx BLOB NOT NULL,
    nevent TEXT,
    nostr_event_id TEXT,
    nostr_event_json TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    expires_at TEXT,
    promoted_at TEXT,
    promotion_height INTEGER,
    promotion_block_hash BLOB,
    UNIQUE (
        market_id,
        creation_txid,
        yes_dormant_asset_blinding_factor,
        yes_dormant_value_blinding_factor,
        no_dormant_asset_blinding_factor,
        no_dormant_value_blinding_factor
    ),
    CHECK (
        (
            expires_at IS NOT NULL
            AND promoted_at IS NULL
            AND promotion_height IS NULL
            AND promotion_block_hash IS NULL
        ) OR (
            expires_at IS NULL
            AND promoted_at IS NOT NULL
            AND promotion_height IS NOT NULL
            AND promotion_block_hash IS NOT NULL
        )
    )
);

CREATE INDEX idx_market_candidates_market_id ON market_candidates (market_id);
CREATE INDEX idx_market_candidates_expires_at ON market_candidates (expires_at);
CREATE INDEX idx_market_candidates_promoted_at ON market_candidates (promoted_at);
CREATE INDEX idx_market_candidates_nostr_event_id ON market_candidates (nostr_event_id);

CREATE TABLE markets (
    market_id BLOB NOT NULL PRIMARY KEY,
    candidate_id INTEGER NOT NULL UNIQUE REFERENCES market_candidates(candidate_id),
    current_state INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    dormant_txid TEXT,
    unresolved_txid TEXT,
    resolved_yes_txid TEXT,
    resolved_no_txid TEXT,
    expired_txid TEXT
);

CREATE INDEX idx_markets_current_state ON markets (current_state);

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
