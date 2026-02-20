-- Markets table
CREATE TABLE markets (
    market_id BLOB NOT NULL PRIMARY KEY,          -- 32 bytes: SHA256(yes_token || no_token)
    oracle_public_key BLOB NOT NULL,              -- 32 bytes
    collateral_asset_id BLOB NOT NULL,            -- 32 bytes
    yes_token_asset BLOB NOT NULL,                -- 32 bytes
    no_token_asset BLOB NOT NULL,                 -- 32 bytes
    yes_reissuance_token BLOB NOT NULL,           -- 32 bytes
    no_reissuance_token BLOB NOT NULL,            -- 32 bytes
    collateral_per_token BIGINT NOT NULL,         -- u64 stored as i64
    expiry_time INTEGER NOT NULL,                 -- u32
    cmr BLOB NOT NULL,                            -- 32 bytes, Commitment Merkle Root
    dormant_spk BLOB NOT NULL,                    -- scriptPubKey for state 0
    unresolved_spk BLOB NOT NULL,                 -- scriptPubKey for state 1
    resolved_yes_spk BLOB NOT NULL,               -- scriptPubKey for state 2
    resolved_no_spk BLOB NOT NULL,                -- scriptPubKey for state 3
    current_state INTEGER NOT NULL DEFAULT 0,     -- MarketState as u64
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_markets_oracle_public_key ON markets (oracle_public_key);
CREATE INDEX idx_markets_collateral_asset_id ON markets (collateral_asset_id);
CREATE INDEX idx_markets_current_state ON markets (current_state);
CREATE INDEX idx_markets_expiry_time ON markets (expiry_time);

-- Maker orders table
CREATE TABLE maker_orders (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    base_asset_id BLOB NOT NULL,                  -- 32 bytes
    quote_asset_id BLOB NOT NULL,                 -- 32 bytes
    price BIGINT NOT NULL,                        -- u64
    min_fill_lots BIGINT NOT NULL,                -- u64
    min_remainder_lots BIGINT NOT NULL,           -- u64
    direction INTEGER NOT NULL,                   -- 0 = SellBase, 1 = SellQuote
    maker_receive_spk_hash BLOB NOT NULL,         -- 32 bytes
    cosigner_pubkey BLOB NOT NULL,                -- 32 bytes
    cmr BLOB NOT NULL,                            -- 32 bytes
    maker_base_pubkey BLOB,                       -- 32 bytes, nullable (only if we are the maker)
    covenant_spk BLOB,                            -- nullable, requires maker_base_pubkey
    order_status INTEGER NOT NULL DEFAULT 0,      -- 0=pending, 1=active, 2=partial, 3=filled, 4=cancelled
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (cmr, maker_base_pubkey)
);

CREATE INDEX idx_maker_orders_base_asset_id ON maker_orders (base_asset_id);
CREATE INDEX idx_maker_orders_quote_asset_id ON maker_orders (quote_asset_id);
CREATE INDEX idx_maker_orders_direction ON maker_orders (direction);
CREATE INDEX idx_maker_orders_order_status ON maker_orders (order_status);
CREATE INDEX idx_maker_orders_price ON maker_orders (price);

-- UTXOs table
CREATE TABLE utxos (
    txid BLOB NOT NULL,                           -- 32 bytes
    vout INTEGER NOT NULL,                        -- u32
    script_pubkey BLOB NOT NULL,
    asset_id BLOB NOT NULL,                       -- 32 bytes
    value BIGINT NOT NULL,                        -- u64
    asset_blinding_factor BLOB NOT NULL,          -- 32 bytes
    value_blinding_factor BLOB NOT NULL,          -- 32 bytes
    raw_txout BLOB NOT NULL,                      -- serialized TxOut
    market_id BLOB REFERENCES markets(market_id),
    maker_order_id INTEGER REFERENCES maker_orders(id),
    market_state INTEGER,                         -- which state address for markets
    spent INTEGER NOT NULL DEFAULT 0,             -- boolean
    spending_txid BLOB,                           -- 32 bytes
    block_height INTEGER,
    spent_block_height INTEGER,
    PRIMARY KEY (txid, vout)
);

CREATE INDEX idx_utxos_market_id ON utxos (market_id);
CREATE INDEX idx_utxos_maker_order_id ON utxos (maker_order_id);
CREATE INDEX idx_utxos_spent ON utxos (spent);
CREATE INDEX idx_utxos_script_pubkey ON utxos (script_pubkey);
CREATE INDEX idx_utxos_market_state_spent ON utxos (market_id, market_state, spent);

-- Sync state singleton
CREATE TABLE sync_state (
    id INTEGER NOT NULL PRIMARY KEY CHECK(id = 1),
    last_block_hash BLOB,                         -- 32 bytes
    last_block_height INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO sync_state (id) VALUES (1);
