ALTER TABLE markets ADD COLUMN nostr_event_id TEXT;
ALTER TABLE markets ADD COLUMN nostr_event_json TEXT;
ALTER TABLE maker_orders ADD COLUMN nostr_event_id TEXT;
ALTER TABLE maker_orders ADD COLUMN nostr_event_json TEXT;
