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

use nostr_sdk::prelude::*;
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
    /// Event id of the exact comment row the UI should focus. For
    /// replies this is the reply event itself; for reactions / zaps
    /// it's the target comment from the lowercase `e` tag. Absent
    /// for profile-targeted events.
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
                    log::warn!(
                        "notifications: failed to write {}: {e}",
                        self.path.display()
                    );
                }
            }
            Err(e) => log::warn!("notifications: failed to serialize: {e}"),
        }
    }
}

// ============================================================================
// Parsing
// ============================================================================

/// Soft cap on `body_preview` length — the full comment body is
/// typically already bounded to 4096 bytes but we only need a taste
/// for the notification row.
const PREVIEW_MAX_CHARS: usize = 140;

fn tag_value<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == key {
            Some(fields[1].as_str())
        } else {
            None
        }
    })
}

fn lowercase_p_targets_me(event: &Event, my_pubkey_hex: &str) -> bool {
    event.tags.iter().any(|tag| {
        let fields = tag.as_slice();
        fields.len() >= 2 && fields[0] == "p" && fields[1].eq_ignore_ascii_case(my_pubkey_hex)
    })
}

/// Market `id:creator` pair recovered from the comment's uppercase
/// `A` coordinate. Format in NIP-22 land: `30078:<creator>:<id>`.
fn parse_market_coordinate(coord: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = coord.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }
    if parts[0] != "30078" {
        return None;
    }
    Some((parts[2].to_string(), parts[1].to_string()))
}

fn truncated_preview(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= PREVIEW_MAX_CHARS {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(PREVIEW_MAX_CHARS).collect();
    out.push('…');
    out
}

/// Pull the declared amount (msats) from a kind:9735 receipt by
/// parsing the embedded kind:9734 request JSON in the `description`
/// tag. Same helper the zap-aggregation path uses — duplicated here
/// to keep the notifications module self-contained.
fn zap_receipt_amount_msats(event: &Event) -> Option<u64> {
    let description = tag_value(event, "description")?;
    let parsed: serde_json::Value = serde_json::from_str(description).ok()?;
    let tags = parsed.get("tags")?.as_array()?;
    for tag in tags {
        let arr = tag.as_array()?;
        if arr.len() >= 2 && arr[0].as_str() == Some("amount") {
            if let Some(s) = arr[1].as_str() {
                return s.parse::<u64>().ok();
            }
        }
    }
    None
}

/// Extract the `zap request` event (kind:9734) author + content from
/// a kind:9735 receipt's `description` tag. The author is the zapper,
/// the content is the optional zap message.
fn zap_receipt_sender(event: &Event) -> Option<(String, Option<String>)> {
    let description = tag_value(event, "description")?;
    let parsed: serde_json::Value = serde_json::from_str(description).ok()?;
    let pubkey = parsed.get("pubkey")?.as_str()?.to_string();
    let content = parsed
        .get("content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    Some((pubkey, content))
}

/// Turn a raw inbound Nostr event into a notification record. Returns
/// `None` when the event isn't notification-worthy — either it's the
/// viewer's own event, it doesn't actually target them, its shape
/// doesn't match a supported kind, or it targets an event the viewer
/// didn't author through Deadcat.
///
/// `own_deadcat_event_ids` is the set of event IDs the viewer has
/// published from this app (kind:1111 comments, kind:30078 markets,
/// etc., all carrying the NIP-89 client tag). Notifications targeting
/// any other event are dropped — keeps the bell from filling up with
/// reactions/zaps the user got on their non-Deadcat Nostr activity.
/// Pass an empty set to disable filtering (only useful while still
/// building the tracker on cold start).
pub fn parse_notification_event(
    event: &Event,
    my_pubkey_hex: &str,
    own_deadcat_event_ids: &std::collections::HashSet<String>,
) -> Option<NotificationRecord> {
    // Self-authored events never generate notifications — you don't
    // need to be told about your own reply / reaction / zap receipt.
    if event.pubkey.to_hex().eq_ignore_ascii_case(my_pubkey_hex) {
        return None;
    }

    /// Does this notification target an event the viewer authored
    /// through Deadcat? Empty `own_set` opts out of filtering — used
    /// while the tracker is still hydrating on cold start so
    /// notifications aren't silently dropped.
    fn targets_deadcat_event(
        target: Option<&str>,
        own_set: &std::collections::HashSet<String>,
    ) -> bool {
        if own_set.is_empty() {
            return true;
        }
        match target {
            Some(id) => own_set.contains(id),
            None => false,
        }
    }

    let market = tag_value(event, "A")
        .and_then(parse_market_coordinate)
        .map(|(id, creator)| (Some(id), Some(creator)));
    let (market_id, market_creator_pubkey) = market.unwrap_or((None, None));

    match event.kind.as_u16() {
        1111 => {
            // Only treat as a Reply when the lowercase `p` tag
            // matches us — that's the NIP-22 signal that the event
            // is a direct reply to our comment. The filter-level
            // `#p: [me]` also matches uppercase-P hits (we're the
            // market creator); those get filtered out here so we
            // don't notify on every comment on markets we created.
            // Mention-in-body detection is a follow-up.
            if !lowercase_p_targets_me(event, my_pubkey_hex) {
                return None;
            }
            // The lowercase `e` tag is the parent comment we
            // authored; if it isn't in our deadcat-published set,
            // this is a reply to a non-Deadcat comment of ours and
            // the user doesn't want it surfaced here.
            if !targets_deadcat_event(tag_value(event, "e"), own_deadcat_event_ids) {
                return None;
            }
            // Deep-link notifications to the incoming reply row,
            // not the parent comment being replied to. The parent
            // still lives in the lowercase `e` tag, but the scroll
            // target users expect is the reply event itself.
            let comment_id = Some(event.id.to_hex());
            Some(NotificationRecord {
                event_id: event.id.to_hex(),
                kind: NotificationKind::Reply,
                author_pubkey: event.pubkey.to_hex(),
                market_id,
                market_creator_pubkey,
                comment_id,
                emoji: None,
                amount_msats: None,
                body_preview: Some(truncated_preview(&event.content)),
                created_at: event.created_at.as_u64(),
                read: false,
            })
        }
        7 => {
            // NIP-25: target event in lowercase `e`, emoji in content.
            let comment_id = tag_value(event, "e").map(String::from)?;
            // Reactions on non-Deadcat content are out of scope for
            // the bell; the user can see those in their Nostr client
            // of choice.
            if !targets_deadcat_event(Some(&comment_id), own_deadcat_event_ids) {
                return None;
            }
            let emoji = event.content.trim();
            if emoji.is_empty() {
                return None;
            }
            Some(NotificationRecord {
                event_id: event.id.to_hex(),
                kind: NotificationKind::Reaction,
                author_pubkey: event.pubkey.to_hex(),
                market_id,
                market_creator_pubkey,
                comment_id: Some(comment_id),
                emoji: Some(emoji.to_string()),
                amount_msats: None,
                body_preview: None,
                created_at: event.created_at.as_u64(),
                read: false,
            })
        }
        9735 => {
            // NIP-57 receipt — sender lives in the embedded kind:9734
            // request's `pubkey` field, not the receipt's author
            // (which is the recipient's LNURL provider).
            let comment_id = tag_value(event, "e").map(String::from);
            // Profile-level zaps (no `e` tag) come through whatever
            // their target — they target the user directly, not a
            // specific comment, so it's fine to keep them. Event-
            // level zaps must hit one of our Deadcat events.
            if comment_id.is_some()
                && !targets_deadcat_event(comment_id.as_deref(), own_deadcat_event_ids)
            {
                return None;
            }
            let amount_msats = zap_receipt_amount_msats(event);
            let (sender_pubkey, zap_message) =
                zap_receipt_sender(event).unwrap_or((event.pubkey.to_hex(), None));
            // Skip zaps the recipient sent themselves.
            if sender_pubkey.eq_ignore_ascii_case(my_pubkey_hex) {
                return None;
            }
            Some(NotificationRecord {
                event_id: event.id.to_hex(),
                kind: NotificationKind::Zap,
                author_pubkey: sender_pubkey,
                market_id,
                market_creator_pubkey,
                comment_id,
                emoji: None,
                amount_msats,
                body_preview: zap_message.map(|m| truncated_preview(&m)),
                created_at: event.created_at.as_u64(),
                read: false,
            })
        }
        _ => None,
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

    #[test]
    fn parse_reply_notification_uses_reply_event_id_for_focus() {
        let viewer = Keys::generate();
        let replier = Keys::generate();
        let market_creator = Keys::generate();
        let viewer_pubkey_hex = viewer.public_key().to_hex();
        let market_creator_hex = market_creator.public_key().to_hex();
        let parent_comment_id = "11".repeat(32);
        let market_id = "22".repeat(32);
        let event = EventBuilder::new(Kind::Custom(1111), "reply body")
            .tags(vec![
                Tag::parse(vec!["p".to_string(), viewer_pubkey_hex.clone()]).unwrap(),
                Tag::parse(vec!["e".to_string(), parent_comment_id.clone()]).unwrap(),
                Tag::parse(vec![
                    "A".to_string(),
                    format!("30078:{market_creator_hex}:{market_id}"),
                ])
                .unwrap(),
            ])
            .sign_with_keys(&replier)
            .unwrap();

        // Treat the parent comment as one we authored through Deadcat
        // so the new own-events filter keeps the reply.
        let mut own_set = std::collections::HashSet::new();
        own_set.insert(parent_comment_id.clone());
        let parsed = parse_notification_event(&event, &viewer_pubkey_hex, &own_set).unwrap();
        let reply_event_id = event.id.to_hex();

        assert_eq!(parsed.kind, NotificationKind::Reply);
        assert_eq!(parsed.event_id.as_str(), reply_event_id.as_str());
        assert_eq!(parsed.comment_id.as_deref(), Some(reply_event_id.as_str()));
        assert_ne!(
            parsed.comment_id.as_deref(),
            Some(parent_comment_id.as_str())
        );
        assert_eq!(parsed.market_id.as_deref(), Some(market_id.as_str()));
        assert_eq!(
            parsed.market_creator_pubkey.as_deref(),
            Some(market_creator_hex.as_str())
        );
    }
}
