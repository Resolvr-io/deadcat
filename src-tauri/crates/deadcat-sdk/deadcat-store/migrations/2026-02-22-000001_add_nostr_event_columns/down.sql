-- SQLite does not support DROP COLUMN before 3.35.0.
-- For older versions this migration is not reversible.
ALTER TABLE markets DROP COLUMN nostr_event_id;
ALTER TABLE markets DROP COLUMN nostr_event_json;
ALTER TABLE maker_orders DROP COLUMN nostr_event_id;
ALTER TABLE maker_orders DROP COLUMN nostr_event_json;
