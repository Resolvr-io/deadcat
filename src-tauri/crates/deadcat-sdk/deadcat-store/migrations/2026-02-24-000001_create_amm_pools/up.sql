-- AMM pools table
CREATE TABLE amm_pools (
    pool_id BLOB NOT NULL PRIMARY KEY,               -- 32 bytes: SHA256(all params)
    yes_asset_id BLOB NOT NULL,                      -- 32 bytes
    no_asset_id BLOB NOT NULL,                       -- 32 bytes
    lbtc_asset_id BLOB NOT NULL,                     -- 32 bytes
    lp_asset_id BLOB NOT NULL,                       -- 32 bytes
    lp_reissuance_token_id BLOB NOT NULL,            -- 32 bytes
    fee_bps INTEGER NOT NULL,                        -- u64 stored as i32
    cosigner_pubkey BLOB NOT NULL,                   -- 32 bytes
    cmr BLOB NOT NULL,                               -- 32 bytes, Commitment Merkle Root
    issued_lp BIGINT NOT NULL DEFAULT 0,             -- current issued LP token supply
    r_yes BIGINT,                                    -- current YES reserve (nullable until known)
    r_no BIGINT,                                     -- current NO reserve
    r_lbtc BIGINT,                                   -- current L-BTC reserve
    covenant_spk BLOB NOT NULL,                      -- scriptPubKey at current issued_lp
    pool_status INTEGER NOT NULL DEFAULT 0,          -- 0=active, 1=inactive, 2=closed
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    nostr_event_id TEXT,
    nostr_event_json TEXT
);

CREATE INDEX idx_amm_pools_pool_status ON amm_pools (pool_status);
CREATE INDEX idx_amm_pools_yes_asset ON amm_pools (yes_asset_id);
CREATE INDEX idx_amm_pools_no_asset ON amm_pools (no_asset_id);

-- Add amm_pool_id FK column to utxos
ALTER TABLE utxos ADD COLUMN amm_pool_id BLOB REFERENCES amm_pools(pool_id);
CREATE INDEX idx_utxos_amm_pool_id ON utxos (amm_pool_id);
