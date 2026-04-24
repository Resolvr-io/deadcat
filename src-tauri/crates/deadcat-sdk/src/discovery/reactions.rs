//! NIP-25 (kind 7) reactions scoped to deadcat comment events.
//!
//! A reaction is a signed event whose content is a single unicode
//! emoji (or "+" / "-" for legacy like/dislike). The target event is
//! carried via an `e` tag + `k` tag; the target author via `p`. We
//! only emit single-emoji content for MVP — custom-emoji shortcodes
//! and the associated `emoji` tag are deferred.
//!
//! Aggregation mirrors NIP-57 zap receipts: subscribe to kind:7
//! events referencing a batch of comment event ids, group by emoji,
//! and return per-event lists the UI renders as pill rows. Unique
//! authors per (event, emoji) are counted once so duplicate reaction
//! publishes don't inflate the number.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Kind used for NIP-25 reactions.
pub const REACTION_KIND: Kind = Kind::Custom(7);

/// A single emoji's count across reactions referencing one event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionCount {
    /// The emoji character (or "+" / "-" for legacy reactions).
    pub emoji: String,
    /// Number of distinct authors that reacted with this emoji.
    pub count: u32,
    /// True when the viewer is one of the authors of this emoji's
    /// reaction list. `false` if no viewer pubkey was supplied.
    pub mine: bool,
    /// Event id of the viewer's own reaction event for this emoji,
    /// if `mine` is true. Lets the UI toggle the reaction off by
    /// publishing a deletion without a second round-trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_event_id: Option<String>,
}

/// Aggregated reactions for a single comment event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReactionSummary {
    /// Hex event id the kind:7 events referenced via `e`.
    pub event_id: String,
    /// Per-emoji counts, sorted by count descending, then emoji
    /// ascending — stable across calls so the UI doesn't reshuffle
    /// between invalidations.
    pub reactions: Vec<ReactionCount>,
}

/// Build an UNSIGNED kind 7 reaction event. Caller signs via their
/// own `NostrSigner` — local keys or NIP-46 remote signer.
pub fn build_reaction_event(
    author: PublicKey,
    target_event_id_hex: &str,
    target_author_pubkey_hex: &str,
    target_kind: u16,
    emoji: &str,
) -> Result<UnsignedEvent, String> {
    let trimmed = emoji.trim();
    if trimmed.is_empty() {
        return Err("reaction emoji must not be empty".to_string());
    }
    let event_id = EventId::from_hex(target_event_id_hex)
        .map_err(|e| format!("invalid target event id: {e}"))?;
    let target_author = PublicKey::from_hex(target_author_pubkey_hex)
        .map_err(|e| format!("invalid target author pubkey: {e}"))?;

    let tags = vec![
        Tag::event(event_id),
        Tag::public_key(target_author),
        Tag::custom(TagKind::custom("k"), vec![target_kind.to_string()]),
    ];

    Ok(EventBuilder::new(REACTION_KIND, trimmed)
        .tags(tags)
        .build(author))
}

/// Build an UNSIGNED kind 5 deletion request targeting a reaction
/// authored by the signing key. Relays honour this only when the
/// request is signed by the original reaction's author.
pub fn build_reaction_deletion_event(
    author: PublicKey,
    reaction_event_id_hex: &str,
) -> Result<UnsignedEvent, String> {
    let event_id = EventId::from_hex(reaction_event_id_hex)
        .map_err(|e| format!("invalid reaction event id: {e}"))?;
    let tags = vec![
        Tag::event(event_id),
        Tag::custom(
            TagKind::custom("k"),
            vec![REACTION_KIND.as_u16().to_string()],
        ),
    ];
    Ok(EventBuilder::new(Kind::Custom(5), "delete reaction")
        .tags(tags)
        .build(author))
}

/// Fetch kind:7 reactions referencing any of the given event ids and
/// return per-event, per-emoji summaries.
///
/// `viewer_pubkey_hex` is optional; when provided, each returned
/// `ReactionCount` carries a `mine: true` flag and the viewer's own
/// reaction event id so the UI can publish a deletion on toggle-off
/// without another fetch. Missing entries in the result mean no
/// reactions were seen on the relay pool within `timeout` — callers
/// should merge with a zero default so every comment renders.
pub async fn fetch_reaction_summaries_for_events(
    client: &Client,
    event_ids_hex: &[String],
    viewer_pubkey_hex: Option<&str>,
    timeout: Duration,
) -> Result<Vec<EventReactionSummary>, String> {
    if event_ids_hex.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<EventId> = event_ids_hex
        .iter()
        .filter_map(|hex| EventId::from_hex(hex).ok())
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let filter = Filter::new().kind(REACTION_KIND).events(ids);
    let events = client
        .fetch_events(vec![filter], timeout)
        .await
        .map_err(|e| format!("failed to fetch reaction events: {e}"))?;

    let viewer = viewer_pubkey_hex.and_then(|hex| PublicKey::from_hex(hex).ok());

    #[derive(Default)]
    struct Bucket {
        authors: HashSet<String>,
        my_event_id: Option<String>,
    }
    let mut buckets: HashMap<(String, String), Bucket> = HashMap::new();

    for reaction in events.iter() {
        let emoji = reaction.content.trim();
        if emoji.is_empty() {
            continue;
        }
        let author_hex = reaction.pubkey.to_hex();
        let is_viewer = viewer.is_some_and(|v| v == reaction.pubkey);
        let react_id = reaction.id.to_hex();
        for tag in reaction.tags.iter() {
            let fields = tag.as_slice();
            if fields.len() >= 2 && fields[0] == "e" {
                let event_id = fields[1].to_string();
                let bucket = buckets.entry((event_id, emoji.to_string())).or_default();
                bucket.authors.insert(author_hex.clone());
                if is_viewer && bucket.my_event_id.is_none() {
                    bucket.my_event_id = Some(react_id.clone());
                }
            }
        }
    }

    let mut by_event: HashMap<String, Vec<ReactionCount>> = HashMap::new();
    for ((event_id, emoji), bucket) in buckets {
        by_event.entry(event_id).or_default().push(ReactionCount {
            emoji,
            count: bucket.authors.len() as u32,
            mine: bucket.my_event_id.is_some(),
            my_event_id: bucket.my_event_id,
        });
    }

    let mut summaries: Vec<EventReactionSummary> = by_event
        .into_iter()
        .map(|(event_id, mut reactions)| {
            // Most-reacted first, then alphabetical for stability.
            reactions.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.emoji.cmp(&b.emoji)));
            EventReactionSummary {
                event_id,
                reactions,
            }
        })
        .collect();
    summaries.sort_by(|a, b| a.event_id.cmp(&b.event_id));
    Ok(summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_event_carries_e_p_k_tags_and_emoji_content() {
        let keys = Keys::generate();
        let target_event_id = "bb".repeat(32);
        let target_author = hex::encode([0xaa; 32]);
        let unsigned = build_reaction_event(
            keys.public_key(),
            &target_event_id,
            &target_author,
            1111,
            "🐱",
        )
        .unwrap();
        let signed = unsigned.sign_with_keys(&keys).unwrap();

        assert_eq!(signed.kind, REACTION_KIND);
        assert_eq!(signed.content, "🐱");

        let has_e = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "e" && f[1] == target_event_id
        });
        assert!(has_e, "missing e tag");

        let has_p = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "p" && f[1] == target_author
        });
        assert!(has_p, "missing p tag");

        let has_k = signed.tags.iter().any(|t| {
            let f = t.as_slice();
            f.len() >= 2 && f[0] == "k" && f[1] == "1111"
        });
        assert!(has_k, "missing k tag");
    }

    #[test]
    fn empty_emoji_is_rejected() {
        let keys = Keys::generate();
        let err = build_reaction_event(
            keys.public_key(),
            &"bb".repeat(32),
            &hex::encode([0xaa; 32]),
            1111,
            "   ",
        )
        .unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn deletion_event_targets_reaction_id_and_kind_7() {
        let keys = Keys::generate();
        let reaction_id = "cc".repeat(32);
        let unsigned = build_reaction_deletion_event(keys.public_key(), &reaction_id).unwrap();
        assert_eq!(unsigned.kind, Kind::Custom(5));

        let e_value = unsigned.tags.iter().find_map(|t| {
            let f = t.as_slice();
            if f.len() >= 2 && f[0] == "e" {
                Some(f[1].as_str())
            } else {
                None
            }
        });
        assert_eq!(e_value, Some(reaction_id.as_str()));

        let k_value = unsigned.tags.iter().find_map(|t| {
            let f = t.as_slice();
            if f.len() >= 2 && f[0] == "k" {
                Some(f[1].as_str())
            } else {
                None
            }
        });
        assert_eq!(k_value, Some("7"));
    }
}
