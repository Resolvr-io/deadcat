//! Local notification store for inbound engagement events.
//!
//! A rolling Nostr subscription (started alongside the discovery
//! subscription on node init) watches for kind:7 reactions, kind:9735
//! zap receipts, and kind:1111 comments that tag the local pubkey
//! in a `p` slot. Each matching event gets parsed into a
//! `NotificationRecord`, deduped by event id, inserted into this
//! store, and flushed to disk as JSON so the bell count survives
//! app restarts.
//!
//! Read state is local-only for v1 — no NIP-51 read-receipt list
//! is published. Users who sign in on a second device will see the
//! notifications as unread there, which matches the mobile client
//! precedent (Amethyst / Primal both work this way).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Filename under `app_data_dir/<network>/` holding the persisted
/// notifications list. Same-name-per-network keeps testnet and
/// mainnet separate; swapping networks shouldn't carry notifications
/// across.
const NOTIFICATIONS_FILE: &str = "notifications.json";

/// Hard cap on retained notifications. Beyond this the oldest are
/// pruned. 500 covers weeks of normal use; preventing an unbounded
/// file is more important than perfect fidelity.
const MAX_NOTIFICATIONS: usize = 500;

/// What kind of inbound event produced the notification. The frontend
/// uses this to pick an icon and phrasing for each row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Reply,
    Mention,
    Zap,
    Reaction,
}

/// A single persisted notification. All ids are lower-case hex.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRecord {
    /// Event id of the triggering event (kind 1111 / 7 / 9735).
    pub event_id: String,
    pub kind: NotificationKind,
    /// Hex pubkey of the sender. Empty for synthetic entries.
    pub author_pubkey: String,
    /// Market id the interaction belongs to, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_id: Option<String>,
    /// Creator pubkey of the target market. Paired with `market_id`
    /// so the frontend can link directly without another lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_creator_pubkey: Option<String>,
    /// Event id of the comment that was reacted to / replied to /
    /// zapped / mentioned. Absent for profile-targeted events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    /// Emoji payload for kind:7 reactions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Amount in millisats for kind:9735 zaps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_msats: Option<u64>,
    /// Short preview of the triggering content (reply body, zap
    /// comment, mentioning comment). Truncated server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_preview: Option<String>,
    /// Unix seconds from the triggering event's `created_at`.
    pub created_at: u64,
    /// Local read flag. Defaults to false on insert; flipped via
    /// `mark_read` / `mark_all_read`.
    #[serde(default)]
    pub read: bool,
}

/// Disk-backed notifications store. Writes are debounced at the
/// boundary — callers insert/mutate, then `flush` persists. Loads
/// best-effort: a corrupt or missing file yields an empty store so
/// the app can still surface new notifications.
pub struct NotificationStore {
    path: PathBuf,
    entries: Mutex<Vec<NotificationRecord>>,
}

impl NotificationStore {
    /// Load a store rooted at `<app_data_dir>/<network>/notifications.json`.
    pub fn load(app_data_dir: &Path, network: &str) -> Self {
        let path = app_data_dir.join(network).join(NOTIFICATIONS_FILE);
        let entries = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<Vec<NotificationRecord>>(&raw).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Self {
            path,
            entries: Mutex::new(entries),
        }
    }

    /// Insert a notification if not already present. Returns `true`
    /// when the record was new (caller should flush + emit an event
    /// for the frontend).
    pub fn insert(&self, record: NotificationRecord) -> bool {
        let mut entries = match self.entries.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if entries.iter().any(|e| e.event_id == record.event_id) {
            return false;
        }
        entries.push(record);
        // Newest first, oldest last. Callers render the list as-is.
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        if entries.len() > MAX_NOTIFICATIONS {
            entries.truncate(MAX_NOTIFICATIONS);
        }
        true
    }

    /// Return the most recent `limit` entries, newest first.
    pub fn list(&self, limit: usize) -> Vec<NotificationRecord> {
        let entries = match self.entries.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        entries.iter().take(limit).cloned().collect()
    }

    /// Count of unread entries across the whole retained set.
    pub fn unread_count(&self) -> u32 {
        let entries = match self.entries.lock() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        entries.iter().filter(|e| !e.read).count() as u32
    }

    /// Flip a single entry to read. Returns `true` when something
    /// changed so the caller knows whether to flush.
    pub fn mark_read(&self, event_id: &str) -> bool {
        let mut entries = match self.entries.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let mut changed = false;
        for entry in entries.iter_mut() {
            if entry.event_id == event_id && !entry.read {
                entry.read = true;
                changed = true;
            }
        }
        changed
    }

    /// Flip every unread entry to read.
    pub fn mark_all_read(&self) -> bool {
        let mut entries = match self.entries.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        let mut changed = false;
        for entry in entries.iter_mut() {
            if !entry.read {
                entry.read = true;
                changed = true;
            }
        }
        changed
    }

    /// `since` for the next subscription: one second before the most
    /// recent entry's `created_at`, or 24 hours ago if the store is
    /// empty. Callers use this to bound the relay filter and avoid
    /// replaying everything from genesis on every session.
    pub fn resume_since(&self) -> u64 {
        let entries = match self.entries.lock() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        if let Some(top) = entries.first() {
            top.created_at.saturating_sub(1)
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            now.saturating_sub(24 * 60 * 60)
        }
    }

    /// Persist the current state to disk. Best-effort — logs on
    /// failure rather than propagating, since notifications aren't
    /// critical enough to block the main flow.
    pub fn flush(&self) {
        let entries = match self.entries.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match serde_json::to_string(&entries) {
            Ok(raw) => {
                if let Err(e) = fs::write(&self.path, raw) {
                    log::warn!("notifications: failed to write {}: {e}", self.path.display());
                }
            }
            Err(e) => log::warn!("notifications: failed to serialize: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_record(event_id: &str, created_at: u64) -> NotificationRecord {
        NotificationRecord {
            event_id: event_id.to_string(),
            kind: NotificationKind::Reply,
            author_pubkey: "aa".repeat(32),
            market_id: None,
            market_creator_pubkey: None,
            comment_id: None,
            emoji: None,
            amount_msats: None,
            body_preview: None,
            created_at,
            read: false,
        }
    }

    #[test]
    fn insert_deduplicates_by_event_id() {
        let dir = TempDir::new().unwrap();
        let store = NotificationStore::load(dir.path(), "testnet");
        assert!(store.insert(sample_record("aa", 100)));
        assert!(!store.insert(sample_record("aa", 100)));
        assert_eq!(store.list(10).len(), 1);
    }

    #[test]
    fn list_is_newest_first() {
        let dir = TempDir::new().unwrap();
        let store = NotificationStore::load(dir.path(), "testnet");
        store.insert(sample_record("old", 100));
        store.insert(sample_record("new", 200));
        store.insert(sample_record("mid", 150));
        let listed = store.list(10);
        assert_eq!(listed[0].event_id, "new");
        assert_eq!(listed[1].event_id, "mid");
        assert_eq!(listed[2].event_id, "old");
    }

    #[test]
    fn unread_count_matches_unread_entries() {
        let dir = TempDir::new().unwrap();
        let store = NotificationStore::load(dir.path(), "testnet");
        store.insert(sample_record("a", 100));
        store.insert(sample_record("b", 101));
        store.insert(sample_record("c", 102));
        assert_eq!(store.unread_count(), 3);
        store.mark_read("b");
        assert_eq!(store.unread_count(), 2);
        store.mark_all_read();
        assert_eq!(store.unread_count(), 0);
    }

    #[test]
    fn resume_since_backs_off_one_second_from_latest() {
        let dir = TempDir::new().unwrap();
        let store = NotificationStore::load(dir.path(), "testnet");
        store.insert(sample_record("a", 1_700_000_000));
        assert_eq!(store.resume_since(), 1_699_999_999);
    }

    #[test]
    fn resume_since_falls_back_to_24h_when_empty() {
        let dir = TempDir::new().unwrap();
        let store = NotificationStore::load(dir.path(), "testnet");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let resume = store.resume_since();
        assert!(resume <= now - 24 * 60 * 60 + 5);
        assert!(resume >= now - 24 * 60 * 60 - 5);
    }

    #[test]
    fn flush_persists_and_reload_round_trips() {
        let dir = TempDir::new().unwrap();
        {
            let store = NotificationStore::load(dir.path(), "testnet");
            store.insert(sample_record("a", 100));
            store.insert(sample_record("b", 200));
            store.mark_read("b");
            store.flush();
        }
        let reloaded = NotificationStore::load(dir.path(), "testnet");
        let list = reloaded.list(10);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].event_id, "b");
        assert!(list[0].read);
        assert_eq!(list[1].event_id, "a");
        assert!(!list[1].read);
    }

    #[test]
    fn insert_caps_at_max_and_drops_oldest() {
        let dir = TempDir::new().unwrap();
        let store = NotificationStore::load(dir.path(), "testnet");
        for i in 0..(MAX_NOTIFICATIONS + 5) {
            store.insert(sample_record(&format!("e{i}"), i as u64));
        }
        let list = store.list(MAX_NOTIFICATIONS + 10);
        assert_eq!(list.len(), MAX_NOTIFICATIONS);
        // Newest (highest created_at) stays; oldest drops.
        assert_eq!(list[0].event_id, format!("e{}", MAX_NOTIFICATIONS + 4));
    }
}
