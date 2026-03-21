ALTER TABLE maker_orders ADD COLUMN creation_txid TEXT;
ALTER TABLE maker_orders ADD COLUMN market_id TEXT;
ALTER TABLE maker_orders ADD COLUMN direction_label TEXT;
ALTER TABLE maker_orders ADD COLUMN offered_amount BIGINT;
