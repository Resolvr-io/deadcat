ALTER TABLE maker_orders ADD COLUMN order_nonce BLOB;
ALTER TABLE maker_orders ADD COLUMN maker_receive_spk BLOB;

ALTER TABLE markets ADD COLUMN yes_issuance_entropy BLOB;
ALTER TABLE markets ADD COLUMN no_issuance_entropy BLOB;
ALTER TABLE markets ADD COLUMN yes_issuance_blinding_nonce BLOB;
ALTER TABLE markets ADD COLUMN no_issuance_blinding_nonce BLOB;
