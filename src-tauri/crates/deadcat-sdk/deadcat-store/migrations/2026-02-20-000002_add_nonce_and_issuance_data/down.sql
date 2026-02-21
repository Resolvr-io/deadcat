ALTER TABLE maker_orders DROP COLUMN order_nonce;
ALTER TABLE maker_orders DROP COLUMN maker_receive_spk;

ALTER TABLE markets DROP COLUMN yes_issuance_entropy;
ALTER TABLE markets DROP COLUMN no_issuance_entropy;
ALTER TABLE markets DROP COLUMN yes_issuance_blinding_nonce;
ALTER TABLE markets DROP COLUMN no_issuance_blinding_nonce;
