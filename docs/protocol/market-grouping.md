# Market Grouping Spec

> Status: **draft** | Phase: next

## Problem

Some prediction markets are naturally multi-outcome: "2026 NBA Champion" has 30
sub-markets (one per team), each with its own YES/NO pricing. Today every
deadcat market is a standalone entity. There is no way to present a set of
related markets under a single heading, aggregate their stats, or enforce that
their probabilities are linked.

## Goals

1. Group related markets under a parent **event** visible in the UI.
2. Preserve the existing per-market covenant model (no protocol changes).
3. Use standard Nostr primitives so the grouping is discoverable by any client.
4. Support both "creator publishes group + children together" and "curator
   groups existing third-party markets after the fact".

## Non-goals (v1)

- Enforcing that grouped market probabilities sum to 100% on-chain.
- Cross-market hedging or portfolio orders.
- Nested groups (group of groups).

---

## Nostr Data Model

### Group Announcement Event

A new NIP-78 addressable event (kind `30078`) published by the group creator.

```
kind:    30078
d-tag:   "group:<group_id>"          // deterministic or random 32-byte hex
tags:
  ["d",       "group:<group_id>"]
  ["t",       "deadcat-group"]        // hashtag — filterable
  ["t",       "<category>"]           // e.g. "sports"
  ["network", "liquid-testnet"]
  ["a",       "30078:<child_pubkey>:<child_market_d_tag>", "<relay>"]   // one per child
  ["a",       "30078:<child_pubkey>:<child_market_d_tag>", "<relay>"]
  ...
content: JSON GroupAnnouncement (see below)
```

Using `a`-tags (addressable event references) instead of `e`-tags means the
reference survives if the child market is republished (replaceable events keep
the same coordinate but get a new event ID).

#### GroupAnnouncement JSON

```json
{
  "version": 1,
  "title": "2026 NBA Champion",
  "description": "Which team will win the 2026 NBA Finals?",
  "category": "sports",
  "image_url": null,
  "outcomes": [
    {
      "label": "Oklahoma City Thunder",
      "market_coordinate": "30078:<pubkey>:<market_d_tag>"
    },
    {
      "label": "San Antonio Spurs",
      "market_coordinate": "30078:<pubkey>:<market_d_tag>"
    }
  ]
}
```

`outcomes` is ordered — the UI renders sub-markets in this order. Each entry
carries a human label and the Nostr coordinate of the child market so the
client can resolve it.

### Child Market Back-Reference (optional)

Child contract announcements MAY include a tag pointing back to their parent
group:

```
["a", "30078:<group_pubkey>:<group_d_tag>", "<relay>"]
```

This is optional because:
- The group author may not be the market author (curator case).
- Existing markets should not need republishing to join a group.

If present, clients can discover the group from any child market.

---

## Discovery

### Fetching Groups

```
Filter {
  kinds: [30078],
  #t:    ["deadcat-group"],
  // optionally: authors, #network
}
```

### Resolving Child Markets

For each `a`-tag on the group event, fetch the referenced addressable event:

```
Filter {
  kinds:   [30078],
  authors: [<child_pubkey>],
  #d:      [<child_market_d_tag>]
}
```

Clients already do this for individual market discovery — group resolution is
just N parallel fetches.

---

## SDK Changes (Rust)

### New Constants

```rust
pub const GROUP_TAG: &str = "deadcat-group";
```

### New Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAnnouncement {
    pub version: u8,
    pub title: String,
    pub description: String,
    pub category: String,
    pub image_url: Option<String>,
    pub outcomes: Vec<GroupOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupOutcome {
    pub label: String,
    pub market_coordinate: String, // "30078:<pubkey>:<d-tag>"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredGroup {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub image_url: Option<String>,
    pub outcomes: Vec<GroupOutcome>,
    pub creator_pubkey: String,
    pub created_at: u64,
}
```

### New Functions

```rust
pub fn build_group_event(
    keys: &Keys,
    announcement: &GroupAnnouncement,
    group_id: &str,
    network_tag: &str,
) -> Result<Event, String>;

pub fn build_group_filter(source_author: Option<&PublicKey>) -> Filter;

pub fn parse_group_event(
    event: &Event,
    expected_network_tag: &str,
) -> Result<DiscoveredGroup, String>;
```

---

## Frontend Changes (TypeScript)

### New Types

```typescript
export type MarketGroup = {
  id: string;
  groupId: string;
  title: string;
  description: string;
  category: MarketCategory;
  imageUrl: string | null;
  outcomes: GroupOutcome[];
  creatorPubkey: string;
  createdAt: number;
  // Aggregated from resolved child markets:
  markets: Market[];
  aggregateVolumeBtc: number;
  aggregateTraderCount: number;
};

export type GroupOutcome = {
  label: string;
  marketCoordinate: string;
  market: Market | null; // resolved or null if not yet fetched
};
```

### UI Components

| Component | Description |
|-----------|-------------|
| `GroupCard` | Collapsed card on home page showing title, top 3 outcomes with prices, aggregate volume. Click expands or navigates to detail. |
| `GroupDetail` | Full-page view listing all outcomes sorted by probability (descending), with per-outcome Buy Yes/No buttons. |
| `GroupOutcomeRow` | Single row: label, probability %, price, Buy Yes, Buy No. Clicking the label navigates to the individual market detail. |

### Home Page Integration

Groups and standalone markets coexist in the same feed. The home page fetches
both `deadcat-contract` and `deadcat-group` events. Markets that belong to a
group are hidden from the top-level list and shown only under their group card.

---

## Tauri Commands

```rust
#[tauri::command]
async fn discover_groups(...) -> Result<Vec<DiscoveredGroup>, String>;

#[tauri::command]
async fn create_group(...) -> Result<DiscoveredGroup, String>;

#[tauri::command]
async fn update_group(...) -> Result<DiscoveredGroup, String>;
```

`discover_contracts` continues to return all markets. The frontend is
responsible for partitioning them into grouped vs. standalone based on the
group data.

---

## Oracle Resolution

No change. Each sub-market has its own oracle pubkey and expiry height. The
oracle resolves each sub-market independently. A future enhancement could add
a "group oracle" that attests to all sub-markets in one batch, but this is out
of scope for v1.

---

## Migration / Backwards Compatibility

- Existing markets are unaffected. They remain standalone.
- Groups are additive — old clients that don't understand `deadcat-group`
  simply ignore them and continue showing individual markets.
- A curator can retroactively group existing markets without the original
  creator's involvement.

---

## Open Questions

1. **Probability normalization** — Should the UI display a warning when grouped
   market probabilities don't sum to ~100%? Or silently normalize?
2. **Group editing** — Can outcomes be added/removed after creation? The
   addressable event model (replaceable by same author + d-tag) supports this
   naturally, but the UX of adding a new team mid-season needs thought.
3. **Group ordering** — Should groups have a `sort_order` field, or always sort
   by aggregate volume?
4. **Image/branding** — The `image_url` on GroupAnnouncement: hosted where?
   Could use a Nostr-native media approach or just allow any URL.
5. **Exclusive vs. non-exclusive** — NBA Champion is exclusive (exactly one
   winner). "Which teams make the playoffs?" is non-exclusive (multiple can
   resolve YES). Should the group declare this? It affects whether probability
   normalization makes sense.
