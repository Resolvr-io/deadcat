-- Remove redundant reserve columns from amm_pools.
-- Current reserves are always recoverable from the latest pool_state_snapshots row.
ALTER TABLE amm_pools DROP COLUMN r_yes;
ALTER TABLE amm_pools DROP COLUMN r_no;
ALTER TABLE amm_pools DROP COLUMN r_lbtc;
