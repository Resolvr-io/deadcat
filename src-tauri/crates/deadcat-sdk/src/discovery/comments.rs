//! NIP-22 (kind 1111) comments scoped to a deadcat market.
//!
//! A market is an addressable NIP-78 event (kind 30078 with a `d`-tag equal to
//! the hex market id). Comments form a tree rooted at that market using the
//! NIP-22 uppercase/lowercase tag split:
//!
//! * Uppercase `A` / `K` / `P` always point at the root (the market).
//! * Lowercase `a` / `k` / `p` point at the parent. For a top-level comment
//!   the parent is the market itself; for a reply the parent is another
//!   kind 1111 event, referenced via lowercase `e` + `k` + `p`.
//!
//! Network-scoping (mainnet/testnet/regtest) is carried via the same
//! `network` tag used by other deadcat events so that frontends can filter
//! cross-network noise.

use std::time::Duration;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::network::Network;

use super::APP_EVENT_KIND;

/// Kind used for NIP-22 comments.
pub const COMMENT_KIND: Kind = Kind::Custom(1111);

/// Soft maximum comment body length. The Nostr protocol places no hard
/// limit, but we reject anything above this client-side to keep relays
/// happy and the UI bounded.
pub const MAX_COMMENT_LEN: usize = 4096;

/// A parsed comment, flat — the caller assembles a tree from `parent_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketComment {
    /// Event id of this comment.
    pub id: String,
    /// Hex pubkey of the comment author.
    pub author_pubkey: String,
    /// Plaintext body.
    pub content: String,
    /// Unix seconds.
    pub created_at: u64,
    /// Hex market id the comment is scoped to (`A` tag `d` component).
    pub market_id: String,
    /// Event id of the parent comment, if this is a reply.
    /// `None` for a top-level comment (parent is the market).
    pub parent_id: Option<String>,
    /// Raw event JSON for clients that need access to additional tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_event_json: Option<String>,
}

fn parse_network_tag(network_tag: &str) -> Result<(), String> {
    network_tag
        .parse::<Network>()
        .map(|_| ())
        .map_err(|e| format!("unsupported network tag '{network_tag}': {e}"))
}

fn market_coordinate(creator_pubkey_hex: &str, market_id_hex: &str) -> String {
    format!(
        "{}:{}:{}",
        APP_EVENT_KIND.as_u16(),
        creator_pubkey_hex,
        market_id_hex
    )
}

fn tag_value(tags: &Tags, key: &str) -> Option<String> {
    tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == key {
            Some(fields[1].to_string())
        } else {
            None
        }
    })
}

fn event_tag_value(event: &Event, key: &str) -> Option<String> {
    tag_value(&event.tags, key)
}

fn event_network_tag(event: &Event) -> Option<String> {
    event_tag_value(event, "network")
}

/// Parameters identifying the root market that a comment belongs to.
#[derive(Debug, Clone)]
pub struct CommentRoot<'a> {
    pub market_id_hex: &'a str,
    pub creator_pubkey_hex: &'a str,
    pub market_event_id_hex: &'a str,
    pub relay_hint: Option<&'a str>,
}

/// Optional parent comment reference. `None` means the new comment is a
/// top-level reply to the market.
#[derive(Debug, Clone)]
pub struct CommentParent<'a> {
    pub event_id_hex: &'a str,
    pub author_pubkey_hex: &'a str,
    pub relay_hint: Option<&'a str>,
}

fn push_optional_hint(values: &mut Vec<String>, hint: Option<&str>) {
    if let Some(hint) = hint.filter(|h| !h.is_empty()) {
        values.push(hint.to_string());
    }
}

fn build_comment_tags(
    root: &CommentRoot<'_>,
    parent: Option<&CommentParent<'_>>,
    network_tag: &str,
) -> Result<Vec<Tag>, String> {
    let coordinate = market_coordinate(root.creator_pubkey_hex, root.market_id_hex);
    let kind_str = APP_EVENT_KIND.as_u16().to_string();

    // Root scope (uppercase): always the market.
    let mut root_a = vec![coordinate.clone()];
    push_optional_hint(&mut root_a, root.relay_hint);
    let mut root_p = vec![root.creator_pubkey_hex.to_string()];
    push_optional_hint(&mut root_p, root.relay_hint);

    let mut tags = vec![
        Tag::custom(TagKind::custom("A"), root_a),
        Tag::custom(TagKind::custom("K"), vec![kind_str.clone()]),
        Tag::custom(TagKind::custom("P"), root_p),
        // E tag on the root gives readers a direct event id fallback.
        Tag::custom(TagKind::custom("E"), {
            let mut v = vec![root.market_event_id_hex.to_string()];
            push_optional_hint(&mut v, root.relay_hint);
            v
        }),
    ];

    // Parent scope (lowercase).
    match parent {
        None => {
            // Top-level: parent is the market itself. Duplicate root coords
            // as lowercase per NIP-22.
            let mut parent_a = vec![coordinate];
            push_optional_hint(&mut parent_a, root.relay_hint);
            let mut parent_p = vec![root.creator_pubkey_hex.to_string()];
            push_optional_hint(&mut parent_p, root.relay_hint);
            tags.push(Tag::custom(TagKind::custom("a"), parent_a));
            tags.push(Tag::custom(TagKind::custom("k"), vec![kind_str]));
            tags.push(Tag::custom(TagKind::custom("p"), parent_p));
        }
        Some(parent) => {
            let mut parent_e = vec![parent.event_id_hex.to_string()];
            push_optional_hint(&mut parent_e, parent.relay_hint);
            let mut parent_p = vec![parent.author_pubkey_hex.to_string()];
            push_optional_hint(&mut parent_p, parent.relay_hint);
            tags.push(Tag::custom(TagKind::custom("e"), parent_e));
            tags.push(Tag::custom(
                TagKind::custom("k"),
                vec![COMMENT_KIND.as_u16().to_string()],
            ));
            tags.push(Tag::custom(TagKind::custom("p"), parent_p));
        }
    }

    tags.push(Tag::custom(
        TagKind::custom("network"),
        vec![network_tag.to_string()],
    ));

    // NIP-89 client tag — lets other Nostr clients show "posted via
    // Deadcat.live" on the comment. Handler coordinate + relay hint
    // are intentionally omitted until we publish a kind:31990
    // handler; the plain two-element form is accepted everywhere.
    tags.push(Tag::custom(
        TagKind::custom("client"),
        vec!["Deadcat.live".to_string()],
    ));

    Ok(tags)
}

/// Build an UNSIGNED NIP-22 comment event. The caller signs it with
/// whatever signer they have — local `Keys` via `.sign_with_keys(&keys)`,
/// or a NIP-46 remote signer via the `NostrSigner::sign_event` trait
/// method. This keeps the builder agnostic to where the secret lives.
pub fn build_comment_event(
    author: PublicKey,
    root: &CommentRoot<'_>,
    parent: Option<&CommentParent<'_>>,
    body: &str,
    network_tag: &str,
) -> Result<UnsignedEvent, String> {
    parse_network_tag(network_tag)?;

    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("comment body must not be empty".to_string());
    }
    if trimmed.len() > MAX_COMMENT_LEN {
        return Err(format!(
            "comment body exceeds {MAX_COMMENT_LEN} byte maximum"
        ));
    }

    let tags = build_comment_tags(root, parent, network_tag)?;

    Ok(EventBuilder::new(COMMENT_KIND, trimmed)
        .tags(tags)
        .build(author))
}

/// Build an UNSIGNED kind 5 deletion request (NIP-09) targeting a comment
/// authored by the signing key. Relays are expected to honour this only
/// when the request is signed by the comment's original author.
pub fn build_comment_deletion_event(
    author: PublicKey,
    comment_event_id_hex: &str,
) -> Result<UnsignedEvent, String> {
    let event_id = EventId::from_hex(comment_event_id_hex)
        .map_err(|e| format!("invalid comment event id: {e}"))?;
    let tags = vec![
        Tag::event(event_id),
        Tag::custom(
            TagKind::custom("k"),
            vec![COMMENT_KIND.as_u16().to_string()],
        ),
    ];
    Ok(EventBuilder::new(Kind::Custom(5), "delete comment")
        .tags(tags)
        .build(author))
}

/// Filter for fetching every comment rooted at a specific market.
pub fn build_comment_filter_for_market(creator_pubkey_hex: &str, market_id_hex: &str) -> Filter {
    let coordinate = market_coordinate(creator_pubkey_hex, market_id_hex);
    Filter::new()
        .kind(COMMENT_KIND)
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), [coordinate])
}

/// Filter for fetching deletion requests against a set of comment events.
pub fn build_comment_deletion_filter(comment_event_ids: &[EventId]) -> Filter {
    Filter::new()
        .kind(Kind::Custom(5))
        .events(comment_event_ids.to_vec())
}

/// Parse a raw kind 1111 event into a `MarketComment`, rejecting events
/// that don't match the expected network tag or don't carry a root `A`
/// coordinate.
pub fn parse_comment_event(
    event: &Event,
    expected_network_tag: &str,
) -> Result<MarketComment, String> {
    parse_network_tag(expected_network_tag)?;

    if event.kind != COMMENT_KIND {
        return Err(format!(
            "unexpected kind for comment: {}",
            event.kind.as_u16()
        ));
    }

    // Network tag is optional for comments fetched from third-party clients,
    // but when present it must match to avoid cross-network leakage.
    if let Some(network_tag) = event_network_tag(event).filter(|tag| tag != expected_network_tag) {
        return Err(format!(
            "unsupported network tag for comment event: {network_tag}"
        ));
    }

    let root_coord = event_tag_value(event, "A")
        .ok_or_else(|| "comment event missing root `A` tag".to_string())?;
    let parts: Vec<&str> = root_coord.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(format!("invalid `A` coordinate: {root_coord}"));
    }
    if parts[0] != APP_EVENT_KIND.as_u16().to_string() {
        return Err(format!("comment root kind mismatch: {}", parts[0]));
    }
    let market_id = parts[2].to_string();

    // Find the parent comment reference, if any. A lowercase `e` tag pointing
    // at another kind 1111 event is a reply; otherwise the comment is
    // top-level.
    let parent_id = event.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "e" {
            Some(fields[1].to_string())
        } else {
            None
        }
    });

    Ok(MarketComment {
        id: event.id.to_hex(),
        author_pubkey: event.pubkey.to_hex(),
        content: event.content.clone(),
        created_at: event.created_at.as_u64(),
        market_id,
        parent_id,
        nostr_event_json: serde_json::to_string(event).ok(),
    })
}

/// Fetch all comments for a market plus any deletion requests and return
/// the surviving set sorted oldest-first.
pub async fn fetch_comments(
    client: &Client,
    creator_pubkey_hex: &str,
    market_id_hex: &str,
    expected_network_tag: &str,
    timeout: Duration,
) -> Result<Vec<MarketComment>, String> {
    parse_network_tag(expected_network_tag)?;

    let filter = build_comment_filter_for_market(creator_pubkey_hex, market_id_hex);
    let events = client
        .fetch_events(vec![filter], timeout)
        .await
        .map_err(|e| format!("failed to fetch comment events: {e}"))?;

    let comment_events: Vec<Event> = events.iter().cloned().collect();
    let comment_ids: Vec<EventId> = comment_events.iter().map(|e| e.id).collect();

    // Second round-trip: fetch deletion requests for the returned ids so we
    // can hide author-requested tombstoned comments.
    let deletions = if comment_ids.is_empty() {
        Vec::new()
    } else {
        let filter = build_comment_deletion_filter(&comment_ids);
        client
            .fetch_events(vec![filter], timeout)
            .await
            .map_err(|e| format!("failed to fetch comment deletion events: {e}"))?
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    };

    let author_by_id: std::collections::HashMap<String, String> = comment_events
        .iter()
        .map(|e| (e.id.to_hex(), e.pubkey.to_hex()))
        .collect();
    let mut deleted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for deletion in &deletions {
        let deleter = deletion.pubkey.to_hex();
        for tag in deletion.tags.iter() {
            let fields = tag.as_slice();
            if fields.len() >= 2 && fields[0] == "e" {
                let target = fields[1].to_string();
                if author_by_id
                    .get(&target)
                    .is_some_and(|author| author == &deleter)
                {
                    deleted.insert(target);
                }
            }
        }
    }

    let mut out = Vec::new();
    for event in &comment_events {
        if deleted.contains(&event.id.to_hex()) {
            continue;
        }
        match parse_comment_event(event, expected_network_tag) {
            Ok(comment) => out.push(comment),
            Err(e) => log::warn!("skipping unparseable comment {}: {e}", event.id),
        }
    }

    out.sort_by_key(|c| c.created_at);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_root() -> (String, String, String) {
        // Fixed values so tag composition is easy to assert against.
        let creator = "11".repeat(32);
        let market_id = "aa".repeat(32);
        let event_id = "bb".repeat(32);
        (creator, market_id, event_id)
    }

    #[test]
    fn top_level_comment_carries_uppercase_and_lowercase_root_tags() {
        let keys = Keys::generate();
        let (creator, market_id, event_id) = sample_root();
        let root = CommentRoot {
            market_id_hex: &market_id,
            creator_pubkey_hex: &creator,
            market_event_id_hex: &event_id,
            relay_hint: Some("wss://relay.example"),
        };

        let unsigned = build_comment_event(
            keys.public_key(),
            &root,
            None,
            "hello market",
            "liquid-testnet",
        )
        .expect("build");

        let tag_kinds: Vec<&str> = unsigned
            .tags
            .iter()
            .map(|t| t.as_slice()[0].as_str())
            .collect();
        assert!(tag_kinds.contains(&"A"));
        assert!(tag_kinds.contains(&"K"));
        assert!(tag_kinds.contains(&"P"));
        assert!(tag_kinds.contains(&"a"));
        assert!(tag_kinds.contains(&"k"));
        assert!(tag_kinds.contains(&"p"));
        assert!(tag_kinds.contains(&"network"));

        let expected_coord = format!("{}:{creator}:{market_id}", APP_EVENT_KIND.as_u16());
        let a_tag = tag_value(&unsigned.tags, "A").unwrap();
        assert_eq!(a_tag, expected_coord);
        let a_lower = tag_value(&unsigned.tags, "a").unwrap();
        assert_eq!(a_lower, expected_coord);

        assert_eq!(unsigned.kind, COMMENT_KIND);
        assert_eq!(unsigned.content, "hello market");
    }

    #[test]
    fn reply_comment_uses_parent_event_and_keeps_root_scope() {
        let keys = Keys::generate();
        let (creator, market_id, event_id) = sample_root();
        let parent_id = "cc".repeat(32);
        let parent_author = "dd".repeat(32);
        let root = CommentRoot {
            market_id_hex: &market_id,
            creator_pubkey_hex: &creator,
            market_event_id_hex: &event_id,
            relay_hint: None,
        };
        let parent = CommentParent {
            event_id_hex: &parent_id,
            author_pubkey_hex: &parent_author,
            relay_hint: None,
        };
        let unsigned = build_comment_event(
            keys.public_key(),
            &root,
            Some(&parent),
            "reply",
            "liquid-testnet",
        )
        .unwrap();

        // Root scope is still the market.
        let expected_coord = format!("{}:{creator}:{market_id}", APP_EVENT_KIND.as_u16());
        assert_eq!(
            tag_value(&unsigned.tags, "A").as_deref(),
            Some(expected_coord.as_str())
        );

        // Parent scope references the parent comment as `e` with kind 1111.
        assert_eq!(
            tag_value(&unsigned.tags, "e").as_deref(),
            Some(parent_id.as_str())
        );
        assert_eq!(
            tag_value(&unsigned.tags, "p").as_deref(),
            Some(parent_author.as_str())
        );
        assert_eq!(
            tag_value(&unsigned.tags, "k").as_deref(),
            Some(COMMENT_KIND.as_u16().to_string().as_str())
        );
    }

    #[test]
    fn empty_body_is_rejected() {
        let keys = Keys::generate();
        let (creator, market_id, event_id) = sample_root();
        let root = CommentRoot {
            market_id_hex: &market_id,
            creator_pubkey_hex: &creator,
            market_event_id_hex: &event_id,
            relay_hint: None,
        };
        let err = build_comment_event(keys.public_key(), &root, None, "   ", "liquid-testnet")
            .unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn oversized_body_is_rejected() {
        let keys = Keys::generate();
        let (creator, market_id, event_id) = sample_root();
        let root = CommentRoot {
            market_id_hex: &market_id,
            creator_pubkey_hex: &creator,
            market_event_id_hex: &event_id,
            relay_hint: None,
        };
        let body = "x".repeat(MAX_COMMENT_LEN + 1);
        let err = build_comment_event(keys.public_key(), &root, None, &body, "liquid-testnet")
            .unwrap_err();
        assert!(err.contains("maximum"));
    }

    #[test]
    fn parse_roundtrip_top_level() {
        let keys = Keys::generate();
        let (creator, market_id, event_id) = sample_root();
        let root = CommentRoot {
            market_id_hex: &market_id,
            creator_pubkey_hex: &creator,
            market_event_id_hex: &event_id,
            relay_hint: None,
        };
        let unsigned =
            build_comment_event(keys.public_key(), &root, None, "hi", "liquid-testnet").unwrap();
        let event = unsigned.sign_with_keys(&keys).unwrap();
        let parsed = parse_comment_event(&event, "liquid-testnet").unwrap();
        assert_eq!(parsed.market_id, market_id);
        assert_eq!(parsed.parent_id, None);
        assert_eq!(parsed.content, "hi");
        assert_eq!(parsed.author_pubkey, keys.public_key().to_hex());
    }

    #[test]
    fn parse_rejects_network_mismatch() {
        let keys = Keys::generate();
        let (creator, market_id, event_id) = sample_root();
        let root = CommentRoot {
            market_id_hex: &market_id,
            creator_pubkey_hex: &creator,
            market_event_id_hex: &event_id,
            relay_hint: None,
        };
        let unsigned =
            build_comment_event(keys.public_key(), &root, None, "hi", "liquid-testnet").unwrap();
        let event = unsigned.sign_with_keys(&keys).unwrap();
        let err = parse_comment_event(&event, "liquid-regtest").unwrap_err();
        assert!(err.contains("unsupported network tag"));
    }

    #[test]
    fn parse_rejects_non_market_root() {
        let keys = Keys::generate();
        // Hand-roll an event whose `A` tag points at an addressable event of
        // some other kind (say 30023 long-form content).
        let tags = vec![
            Tag::custom(
                TagKind::custom("A"),
                vec![format!("30023:{}:some-d-tag", keys.public_key().to_hex())],
            ),
            Tag::custom(TagKind::custom("K"), vec!["30023".to_string()]),
            Tag::custom(TagKind::custom("P"), vec![keys.public_key().to_hex()]),
        ];
        let event = EventBuilder::new(COMMENT_KIND, "off-topic")
            .tags(tags)
            .sign_with_keys(&keys)
            .unwrap();
        let err = parse_comment_event(&event, "liquid-testnet").unwrap_err();
        assert!(err.contains("root kind mismatch"));
    }

    #[test]
    fn comment_filter_targets_kind_1111_and_coordinate() {
        let (creator, market_id, _) = sample_root();
        let filter = build_comment_filter_for_market(&creator, &market_id);
        let debug = format!("{filter:?}");
        assert!(debug.contains("1111"));
        assert!(debug.contains(&market_id));
    }

    #[test]
    fn deletion_event_targets_comment_id_and_kind_1111() {
        let keys = Keys::generate();
        let comment_id = "ee".repeat(32);
        let unsigned = build_comment_deletion_event(keys.public_key(), &comment_id).unwrap();
        assert_eq!(unsigned.kind, Kind::Custom(5));
        assert_eq!(
            tag_value(&unsigned.tags, "e").as_deref(),
            Some(comment_id.as_str())
        );
        assert_eq!(
            tag_value(&unsigned.tags, "k").as_deref(),
            Some(COMMENT_KIND.as_u16().to_string().as_str())
        );
    }
}
