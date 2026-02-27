DROP INDEX IF EXISTS idx_amm_pools_market_id;
DROP INDEX IF EXISTS idx_pool_snapshots_block_height;
DROP INDEX IF EXISTS idx_pool_snapshots_pool_id;
DROP TABLE IF EXISTS pool_state_snapshots;

-- SQLite does not support DROP COLUMN, so we cannot cleanly revert ALTER TABLE.
-- The added columns (market_id, creation_txid on amm_pools; *_txid on markets)
-- will persist but be unused after rollback.
