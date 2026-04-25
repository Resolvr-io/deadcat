//! NIP-02 follow lists (kind:3) and NIP-51 mute lists (kind:10000).
//!
//! Both kinds are *replaceable* events — every write overwrites the
//! previous event. Losing the prior state before we publish means
//! silently erasing entries the user never chose to remove. Every
//! write path must therefore fetch the current event first, mutate
//! its contents, and publish the merged result. The command layer
//! enforces this via `fetch_*_list` → mutate → `build_*_event`.
//!
//! Legacy kind:3 `content` holds a deprecated relay-list JSON. NIP-65
//! (kind:10002) superseded it, but many clients still read kind:3
//! content as a fallback relay hint. We never edit it — the builder
//! below preserves whatever was there verbatim, so a write from
//! deadcat can never blow away someone's relay setup.
//!
//! Private mute entries are NIP-44 encrypted with the user's own key
//! into the event `content`. The SDK parses the ciphertext out and
//! returns it to the caller; decryption lives in the command layer so
//! we can support both local `Keys` and NIP-46 remote signers through
//! `NostrSigner::nip44_decrypt`.

use std::time::Duration;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// NIP-02 contact list.
pub const FOLLOW_LIST_KIND: Kind = Kind::Custom(3);

/// NIP-51 mute list.
pub const MUTE_LIST_KIND: Kind = Kind::Custom(10000);

/// A single mute entry. Matches the tag shape used in both public
/// tags and the decrypted private-entry JSON (so the same enum
/// serializes into either location).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum MuteEntry {
    /// Mute a specific author pubkey (hex).
    #[serde(rename = "p")]
    Pubkey(String),
    /// Mute a specific event id (hex).
    #[serde(rename = "e")]
    Event(String),
    /// Mute a hashtag (the `#` is NOT part of the value).
    #[serde(rename = "t")]
    Hashtag(String),
    /// Mute any comment containing this word/phrase (case-insensitive
    /// substring match at render time — relays don't filter for us).
    #[serde(rename = "word")]
    Word(String),
}

impl MuteEntry {
    fn tag(&self) -> Tag {
        match self {
            MuteEntry::Pubkey(hex) => Tag::custom(TagKind::p(), vec![hex.clone()]),
            MuteEntry::Event(hex) => Tag::custom(TagKind::e(), vec![hex.clone()]),
            MuteEntry::Hashtag(tag) => Tag::custom(TagKind::t(), vec![tag.clone()]),
            MuteEntry::Word(word) => Tag::custom(TagKind::custom("word"), vec![word.clone()]),
        }
    }

    fn from_tag_slice(fields: &[String]) -> Option<Self> {
        if fields.len() < 2 {
            return None;
        }
        match fields[0].as_str() {
            "p" => Some(MuteEntry::Pubkey(fields[1].clone())),
            "e" => Some(MuteEntry::Event(fields[1].clone())),
            "t" => Some(MuteEntry::Hashtag(fields[1].clone())),
            "word" => Some(MuteEntry::Word(fields[1].clone())),
            _ => None,
        }
    }
}

/// Parsed follow list — pubkeys + the legacy `content` blob we must
/// preserve on rewrite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowList {
    /// Hex pubkeys, one per `p` tag, deduplicated in read order.
    pub follows: Vec<String>,
    /// Original `content` field (legacy relay list JSON). Empty when
    /// absent. Callers MUST pass this back to the builder verbatim on
    /// rewrite — overwriting it would clobber the user's
    /// relay-discovery hints that other clients still read.
    pub legacy_content: String,
    /// created_at of the source event, for staleness reasoning by the
    /// caller.
    pub created_at: u64,
}

/// Parsed mute list. The `private_ciphertext` is the raw NIP-44
/// ciphertext from `content` — the command layer decrypts it to
/// `Vec<MuteEntry>` using the active signer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuteList {
    /// Entries carried on `tags` (publicly visible to relays).
    pub public: Vec<MuteEntry>,
    /// NIP-44 ciphertext of private entries. Empty when the user's
    /// mute list has no private portion. Decrypt in the command
    /// layer via the active `NostrSigner` so both local keys and
    /// NIP-46 signers are supported.
    pub private_ciphertext: String,
    /// created_at of the source event.
    pub created_at: u64,
}

/// Fetch the latest kind:3 follow list for a pubkey. Returns
/// `Ok(None)` when no event arrived within `timeout` — the caller
/// must decide whether to treat that as "new user, safe to create
/// fresh" or "relay timeout, refuse to write" based on connection
/// state.
pub async fn fetch_follow_list_event(
    client: &Client,
    author_hex: &str,
    timeout: Duration,
) -> Result<Option<Event>, String> {
    fetch_latest_replaceable(client, FOLLOW_LIST_KIND, author_hex, timeout).await
}

/// Fetch the latest kind:10000 mute list for a pubkey.
pub async fn fetch_mute_list_event(
    client: &Client,
    author_hex: &str,
    timeout: Duration,
) -> Result<Option<Event>, String> {
    fetch_latest_replaceable(client, MUTE_LIST_KIND, author_hex, timeout).await
}

async fn fetch_latest_replaceable(
    client: &Client,
    kind: Kind,
    author_hex: &str,
    timeout: Duration,
) -> Result<Option<Event>, String> {
    let author =
        PublicKey::from_hex(author_hex).map_err(|e| format!("invalid author pubkey: {e}"))?;
    let filter = Filter::new().kind(kind).author(author).limit(1);
    let events = client
        .fetch_events(vec![filter], timeout)
        .await
        .map_err(|e| format!("failed to fetch replaceable event: {e}"))?;
    // Relays may still return multiple copies during replication
    // catch-up; newest `created_at` wins (ties broken by event id
    // per NIP-16 replaceable event rules).
    let latest = events.iter().max_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(latest.cloned())
}

/// Extract the follow list from a kind:3 event. Duplicate `p` tags
/// collapse to a single entry in read order.
pub fn parse_follow_list(event: &Event) -> FollowList {
    let mut follows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tag in event.tags.iter() {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "p" {
            let hex = fields[1].clone();
            if seen.insert(hex.clone()) {
                follows.push(hex);
            }
        }
    }
    FollowList {
        follows,
        legacy_content: event.content.clone(),
        created_at: event.created_at.as_u64(),
    }
}

/// Extract the public portion + ciphertext from a kind:10000 event.
pub fn parse_mute_list(event: &Event) -> MuteList {
    let mut public = Vec::new();
    for tag in event.tags.iter() {
        let fields: Vec<String> = tag.as_slice().iter().map(|s| s.to_string()).collect();
        if let Some(entry) = MuteEntry::from_tag_slice(&fields) {
            public.push(entry);
        }
    }
    MuteList {
        public,
        private_ciphertext: event.content.clone(),
        created_at: event.created_at.as_u64(),
    }
}

/// Build an UNSIGNED kind:3 event. `legacy_content` must be the
/// `content` field from the prior event (or empty for first-time
/// publishers) — overwriting it would clobber the user's legacy
/// relay-list hints.
pub fn build_follow_list_event(
    author: PublicKey,
    follows: &[String],
    legacy_content: &str,
) -> Result<UnsignedEvent, String> {
    let mut tags = Vec::with_capacity(follows.len() + 1);
    for hex in follows {
        // Validate each pubkey so we never persist a malformed tag
        // — a single bad tag can make some relays reject the whole
        // event, losing the entire follow list.
        PublicKey::from_hex(hex).map_err(|e| format!("invalid follow pubkey {hex}: {e}"))?;
        tags.push(Tag::custom(TagKind::p(), vec![hex.clone()]));
    }
    tags.push(super::client_tag());
    Ok(EventBuilder::new(FOLLOW_LIST_KIND, legacy_content)
        .tags(tags)
        .build(author))
}

/// Build an UNSIGNED kind:10000 event. `encrypted_content` is the
/// NIP-44 ciphertext from the caller (empty when no private entries).
pub fn build_mute_list_event(
    author: PublicKey,
    public: &[MuteEntry],
    encrypted_content: &str,
) -> UnsignedEvent {
    let mut tags: Vec<Tag> = public.iter().map(|entry| entry.tag()).collect();
    tags.push(super::client_tag());
    EventBuilder::new(MUTE_LIST_KIND, encrypted_content)
        .tags(tags)
        .build(author)
}

/// Serialize a list of mute entries to the NIP-04/NIP-51 private-
/// payload JSON shape: `[["p","<hex>"], ["word","foo"], ...]`. The
/// result is passed to `NostrSigner::nip44_encrypt` by the caller.
pub fn serialize_private_mutes(entries: &[MuteEntry]) -> String {
    let arrays: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| match entry {
            MuteEntry::Pubkey(hex) => vec!["p".to_string(), hex.clone()],
            MuteEntry::Event(hex) => vec!["e".to_string(), hex.clone()],
            MuteEntry::Hashtag(tag) => vec!["t".to_string(), tag.clone()],
            MuteEntry::Word(word) => vec!["word".to_string(), word.clone()],
        })
        .collect();
    serde_json::to_string(&arrays).unwrap_or_else(|_| "[]".to_string())
}

/// Parse the decrypted private-payload JSON back into mute entries.
/// Unknown shapes are skipped rather than rejected so a single bad
/// line doesn't blank the whole list.
pub fn deserialize_private_mutes(plaintext: &str) -> Vec<MuteEntry> {
    let trimmed = plaintext.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let arrays: Vec<Vec<String>> = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    arrays
        .into_iter()
        .filter_map(|fields| MuteEntry::from_tag_slice(&fields))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_follow_list_extracts_p_tags_and_preserves_content() {
        let keys = Keys::generate();
        let a = hex::encode([0x01; 32]);
        let b = hex::encode([0x02; 32]);
        let legacy = r#"{"wss://relay.example":{"read":true,"write":true}}"#;
        let event = EventBuilder::new(FOLLOW_LIST_KIND, legacy)
            .tags(vec![
                Tag::custom(TagKind::p(), vec![a.clone()]),
                Tag::custom(TagKind::p(), vec![b.clone()]),
                // Duplicate — should collapse.
                Tag::custom(TagKind::p(), vec![a.clone()]),
                // Unrelated tag — should be ignored.
                Tag::custom(TagKind::t(), vec!["bitcoin".to_string()]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let parsed = parse_follow_list(&event);
        assert_eq!(parsed.follows, vec![a, b]);
        assert_eq!(parsed.legacy_content, legacy);
    }

    #[test]
    fn build_follow_list_event_preserves_legacy_content_and_rewrites_p_tags() {
        let author = Keys::generate().public_key();
        let follows = vec![hex::encode([0xaa; 32]), hex::encode([0xbb; 32])];
        let legacy = r#"{"wss://relay.example":{"read":true}}"#;
        let unsigned = build_follow_list_event(author, &follows, legacy).unwrap();
        assert_eq!(unsigned.kind, FOLLOW_LIST_KIND);
        assert_eq!(unsigned.content, legacy);
        let p_tags: Vec<String> = unsigned
            .tags
            .iter()
            .filter_map(|tag| {
                let fields = tag.as_slice();
                if fields.len() >= 2 && fields[0] == "p" {
                    Some(fields[1].to_string())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(p_tags, follows);
    }

    #[test]
    fn build_follow_list_event_rejects_invalid_pubkey() {
        let author = Keys::generate().public_key();
        let err = build_follow_list_event(author, &["not-a-hex".to_string()], "").unwrap_err();
        assert!(err.contains("invalid follow pubkey"));
    }

    #[test]
    fn parse_mute_list_separates_public_tags_and_ciphertext() {
        let keys = Keys::generate();
        let muted_hex = hex::encode([0x33; 32]);
        let ciphertext = "AAAAciphertextBBBBBB"; // placeholder
        let event = EventBuilder::new(MUTE_LIST_KIND, ciphertext)
            .tags(vec![
                Tag::custom(TagKind::p(), vec![muted_hex.clone()]),
                Tag::custom(TagKind::custom("word"), vec!["foo".to_string()]),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let parsed = parse_mute_list(&event);
        assert_eq!(parsed.private_ciphertext, ciphertext);
        assert_eq!(parsed.public.len(), 2);
        assert!(parsed.public.contains(&MuteEntry::Pubkey(muted_hex)));
        assert!(parsed.public.contains(&MuteEntry::Word("foo".to_string())));
    }

    #[test]
    fn serialize_and_deserialize_private_mutes_roundtrip() {
        let entries = vec![
            MuteEntry::Pubkey(hex::encode([0x01; 32])),
            MuteEntry::Event(hex::encode([0x02; 32])),
            MuteEntry::Hashtag("spam".to_string()),
            MuteEntry::Word("blocked phrase".to_string()),
        ];
        let json = serialize_private_mutes(&entries);
        let parsed = deserialize_private_mutes(&json);
        assert_eq!(parsed, entries);
    }

    #[test]
    fn deserialize_private_mutes_skips_malformed_entries() {
        // Second entry has only one field — should be skipped, first
        // one still returns.
        let json = r#"[["p","abc"],["word"]]"#;
        let parsed = deserialize_private_mutes(json);
        assert_eq!(parsed, vec![MuteEntry::Pubkey("abc".to_string())]);
    }

    #[test]
    fn build_mute_list_event_carries_public_tags_and_content() {
        let author = Keys::generate().public_key();
        let public = vec![
            MuteEntry::Pubkey(hex::encode([0x55; 32])),
            MuteEntry::Hashtag("scam".to_string()),
        ];
        let ciphertext = "CIPHERTEXT";
        let unsigned = build_mute_list_event(author, &public, ciphertext);
        assert_eq!(unsigned.kind, MUTE_LIST_KIND);
        assert_eq!(unsigned.content, ciphertext);
        let tag_kinds: Vec<String> = unsigned
            .tags
            .iter()
            .filter_map(|tag| tag.as_slice().first().cloned())
            .collect();
        assert_eq!(tag_kinds, vec!["p".to_string(), "t".to_string()]);
    }
}
