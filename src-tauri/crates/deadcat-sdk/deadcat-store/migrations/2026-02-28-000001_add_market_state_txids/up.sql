ALTER TABLE markets ADD COLUMN dormant_txid TEXT;
ALTER TABLE markets ADD COLUMN unresolved_txid TEXT;
ALTER TABLE markets ADD COLUMN resolved_yes_txid TEXT;
ALTER TABLE markets ADD COLUMN resolved_no_txid TEXT;
