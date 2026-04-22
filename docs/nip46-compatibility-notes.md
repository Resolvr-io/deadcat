# NIP-46 compatibility — issues solved on `feat/nip46-remote-signing`

Field notes from getting both **Primal iOS** and **Amber** working across
all three NIP-46 entry points (bunker:// paste, nostrconnect:// QR, and
persisted-connection restore). Primal was the hardest — it behaves
strictly against the modern NIP-46 spec, and the rust `nostr-connect`
0.38 crate we depend on predates several of those requirements. Each
issue below produced a hang, a silent failure, or a cryptic error in
testing. Written up so the next person hitting these doesn't have to
rediscover them.

> **Note on spec vs. implementation.** NIP-46 has evolved significantly
> since its 2023 publication. Older implementations (the rust
> `nostr-connect` 0.38 crate, early Amber builds) were written against
> the original NIP-04-transport spec. Modern signers (Primal iOS, newer
> Amber) follow the post-NIP-44 revision. We had to straddle both.

---

## 1. URI format: `metadata=<JSON>` vs. flat query params

**Symptom (Primal iOS).** QR scans but Primal shows *"Application /
Unknown url"* in the approval sheet and silently fails to publish the
connect response.

**Cause.** `nostr-connect` 0.38's `Display` impl for
`NostrConnectURI::Client` at `nostr-0.38.0/src/nips/nip46.rs:948-953`
emits metadata as a **raw JSON blob** in a `metadata=` query param,
including literal spaces, braces, and quotes:

```
nostrconnect://<pubkey>?metadata={"name":"Deadcat Live"}&relay=...
```

Strict URL parsers (Primal's) choke on the unescaped JSON. Even
URL-encoded JSON, some signers simply ignore `metadata=` and look for
**flat params** (`name=`, `url=`, `image=`) — the format used by
`nostr-tools`'s `createNostrConnectURI` and every working JS client
(Zap Cooking, wordswithzaps, mutable).

**Fix.** Build the display URI by hand with flat params. Never trust the
crate's `Display`.

## 2. URL encoding: `%20` vs. `+` for spaces

**Symptom.** Even with flat params, Primal still occasionally balked.

**Cause.** JS `URLSearchParams.toString()` form-encodes spaces as `+`,
not `%20`. Reference clients all emit URIs with `+`. Primal evidently
normalizes on the JS form.

**Fix.** Our `percent_encode` helper in `src-tauri/src/nip46.rs:54-71`
emits `+` for spaces and `%XX` for everything else outside the RFC 3986
unreserved set — byte-for-byte matching `URLSearchParams`.

## 3. Relay order: put `relay.primal.net` first

**Symptom.** Subscription was live on all three default relays, EOSE
received, but Primal never published the connect response.

**Cause.** Unclear whether Primal truly only publishes to the first
relay in the URI, or whether it prioritizes relays it already trusts —
but the comment in `wordswithzaps/src/nostr/client.ts` reads:
*"primal first for Primal app compatibility"*, and it's the pattern
all working clients use.

**Fix.** In `prepare_nostrconnect`, move any `relay.primal.net` entry to
the front of the list before building the URI.

## 4. `secret` parameter: required by Primal

**Symptom.** URI looked correct, Primal showed the right app name/URL,
user tapped "Login As" — and nothing happened.

**Cause.** Without `secret=<hex>` in the URI, Primal doesn't consider
the handshake legitimate and silently drops it. Per NIP-46 the `secret`
is "optional" but in practice it's mandatory for Primal to play ball.
`nostr-tools` always includes one; the type signature of
`createNostrConnectURI` makes it **required**, not optional.

**Fix.** Generate a 16-hex (8-byte) secret with `rand::thread_rng` and
include it in the URI as `secret=<hex>`. Matches the nostr-tools
convention.

## 5. Secret-echo response can't be parsed by `nostr-connect` 0.38

**Symptom.** Primal publishes the connect response (verified in relay
logs), we receive it on our subscription… but the crate's matcher
silently drops it. Our handshake hangs until the 180 s timeout.

**Cause.** Per NIP-46, when the URI carries a secret, the signer's
`connect` response has `result: "<the-secret>"` — echoed verbatim, not
literal `"ack"`. The crate's `ResponseResult::parse` at
`nostr-0.38.0/src/nips/nip46.rs:390-407` only maps `"ack"` to
`ResponseResult::Connect`; any other string falls through to
`EncryptionDecryption(...)` and the bootstrap's match arm at
`nostr-connect-0.38.0/src/client.rs:435-439` requires exactly
`Some(ResponseResult::Connect)`. So the echoed-secret response is
discarded. Fixed in later crate versions (0.44+), but bumping means
migrating the entire `nostr-sdk` 0.38 surface across our SDK crate,
market discovery, and wallet backup.

**Fix.** Skip the crate for Phase 1 of the handshake entirely. In
`await_nostrconnect_handshake` we subscribe on relays ourselves with
`nostr_sdk::Client`, NIP-44-decrypt incoming kind:24133 events, and
look for `response.result == our_secret`. Once matched, we have the
signer's pubkey from `event.pubkey` and hand off to the crate via a
synthesized bunker URI (Phase 2 — which *does* work in 0.38 because
bunker connects respond with plain `"ack"`).

## 6. NIP-04 vs NIP-44 transport encryption

**Symptom.** Handshake completes correctly, we build a bunker URI, hand
it to the crate, and the crate sends the `connect` RPC. Primal
responds:

```json
{"id":"...","result":null,"error":"Failed to decrypt content."}
```

**Cause.** `nostr-connect` 0.38's `EventBuilder::nostr_connect` in
`nostr-0.38.0/src/event/builder.rs:863-873` **hardcodes NIP-04
encryption** (AES-256-CBC with `?iv=` separator). NIP-46 was written
before NIP-44 existed. Modern signers (Primal, current Amber) speak
only NIP-44 (ChaCha20 + HMAC-SHA256); they literally can't decrypt
NIP-04 RPCs. Fixed in the crate in 0.44.

**Fix.** Replace the crate's RPC transport entirely with our own
`Nip46ManualSigner` (`src-tauri/src/nip46.rs:146+`) that:
- NIP-44 encrypts every outgoing RPC
- Accepts both NIP-04 and NIP-44 incoming (some legacy signers still
  reply NIP-04)
- Keeps a persistent subscription and routes responses to pending
  callers by request `id` via `oneshot` channels
- Implements all six `NostrSigner` trait methods

## 7. Bunker URI `secret=` must be echoed in the `connect` RPC

**Symptom.** Bunker-paste flow: after our earlier fixes, everything
looked correct, but Amber responded with `"error": "invalid secret"`.

**Cause.** Per NIP-46 the `connect` RPC params are
`[<signer_pubkey>, <secret?>, <perms?>]`. When the signer-issued bunker
URI carries a `secret=<x>`, the signer stores it server-side and only
accepts a `connect` request from the client that echoes it back. Our
parser was dropping the secret, and `Nip46ManualSigner::connect` was
sending only the pubkey.

**Fix.** `parse_bunker_uri` now extracts `secret=` alongside `relay=`
params. `connect_from_bunker_uri` builds the RPC params as
`[signer_pubkey, secret]` when the secret is present.

---

## Separate-but-related issues surfaced along the way

### 8. `fetch_nostr_profile` rejected remote signers

`fetch_nostr_profile` was built on `get_keys_and_client`, which errors
for non-local signers with *"requires local keys"*. But fetching a
public kind:0 event only needs the **user's pubkey**, not their secret
key. Swapped to `get_signer_and_client` + `signer.get_public_key()`
which works for both local and remote signers.

### 9. Profile-fetch retry loop cancelled by step transition

The initial retry loop lived inside a `useEffect` in `NostrSetupStep`,
which gets unmounted the moment the `nostrconnect:connected` handler
sets `onboardingStep: "wallet"`. The cleanup flipped `cancelled = true`
during the very first attempt. Moved the fetch to fire-and-forget
inside the handler itself (not scoped to the component's lifecycle), and
added a secondary retry ladder in `WalletSetupStep` as a safety net.

### 10. `walletStatus` didn't sync after NIP-46 create-wallet

`useTauriEvents.ts` only updates the store's `walletStatus` on
`unlocked → locked` transitions. After `create_wallet + unlock_wallet`
fired `app_state_updated` with `walletStatus: "unlocked"`, the store
stayed at its bootstrap value `"not_created"`. `finishOnboarding` then
set `walletOpen: true`, so `WalletPage` matched `"not_created"` and
rendered the legacy `<WalletSetup>` component instead of the unlocked
app — making it look like the wallet creation silently failed. Fixed by
explicitly setting `walletStatus: "unlocked"` in
`handleBunkerWalletSubmit` before calling `finishOnboarding`.

---

## Tested signer compatibility

| Signer | bunker:// paste | nostrconnect:// QR | Restore from disk |
| --- | --- | --- | --- |
| Amber | ✓ | ✓ | ✓ |
| Primal iOS | ✓ | ✓ | ✓ |

## Things NOT yet tested

- nsec.app (deprecated, out of scope)
- Alby browser extension remote signer
- Self-hosted nostr-connect bridges

---

## Takeaways for future protocol work

1. **Don't trust the crate's URI `Display`.** Every working reference
   client hand-builds URIs. The rust crate in particular emits a format
   nothing else accepts.
2. **Reference implementations first.** wordswithzaps, mutable, Zap
   Cooking, and the nostr-tools source are the ground truth for what
   Primal/Amber actually accept — not the spec.
3. **Avoid crate-internal bootstrap for initial handshake.** The rust
   `nostr-connect` 0.38 bootstrap is too rigid (NIP-04 only, strict
   response matcher). Hand-rolling the first round-trip gives us
   control over encryption, parsing, and error surfacing.
4. **Keep the crate for bunker-URI ongoing RPCs**, because it already
   handles the pool, retries, and signer trait plumbing — the parts
   that are painful to reimplement. We just need NIP-44 encryption,
   which `Nip46ManualSigner` provides.
