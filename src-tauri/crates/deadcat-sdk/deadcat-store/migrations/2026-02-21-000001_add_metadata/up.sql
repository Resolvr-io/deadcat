ALTER TABLE markets ADD COLUMN question TEXT;
ALTER TABLE markets ADD COLUMN description TEXT;
ALTER TABLE markets ADD COLUMN category TEXT;
ALTER TABLE markets ADD COLUMN resolution_source TEXT;
ALTER TABLE markets ADD COLUMN starting_yes_price INTEGER;
ALTER TABLE markets ADD COLUMN creator_pubkey BLOB;
ALTER TABLE markets ADD COLUMN creation_txid TEXT;
ALTER TABLE markets ADD COLUMN nevent TEXT;
