# NIP-46 Session Detection + Auto-Reconnect — Planned

This document captures Phase 2 work for the NIP-46 remote-signing flow. It is
**not built yet**; the current branch ships NIP-46 connect + RPC but has no
awareness of whether the remote signer is still alive.

## Current behavior (as of `feat/nip46-remote-signing`)

When a NIP-46 session ends — user taps "Stop Session" in Primal, the signer
app force-quits, a relay drops, etc. — the `Nip46ManualSigner` keeps waiting.
The next `sign_event` RPC silently blocks for 60s, then returns a timeout
error. The user sees no indicator that anything is wrong until they try to
publish something.

### Why the wallet is structurally safe regardless

- Wallet encryption key derives from the user's password, not the signer.
- The Nostr signer only signs Nostr events (markets, kind:0, NIP-44
  backups) — it never touches the LWK wallet.
- A dropped signer session has **zero impact** on wallet state, balance,
  transactions, swaps, or the encrypted mnemonic on disk.

This is a pure UX upgrade, not a correctness fix.

## Scope

1. **Heartbeat task** — a background async loop in Rust pings the signer
   every 60–120 s with a lightweight RPC (likely `get_public_key` with a
   forced refresh — we ignore the cached value specifically so the signer
   has to actually round-trip).

2. **Session-state flag** — add `nip46_online: Arc<AtomicBool>` to
   `NodeState` (or to `Nip46ManualSigner` itself so it lives with the
   signer). Heartbeat failure flips it `false`; success flips it `true`.

3. **Tauri event** — emit `nip46:status` with `{ connected: bool }` on
   state transitions so the frontend reacts without polling.

4. **Frontend banner** — top-of-app warning banner when
   `nip46_online == false`, modelled on the existing wallet-backup
   reminder in `WalletUnlocked.tsx`:
   > *Remote signer disconnected — publishing is paused. [Reconnect]*

5. **Short-circuit RPCs while offline** — in `Nip46ManualSigner::send_request`,
   check the flag and fail fast (500ms timeout instead of 60s) when the
   signer is already known to be down. User sees errors immediately
   instead of UI-hanging.

6. **Auto-reconnect** — on app regaining focus (Tauri `focus` event) or
   manual click of the banner's "Reconnect" button, re-run the
   equivalent of `connect_from_bunker_uri` using the persisted
   `Nip46Connection` (which already has `bunker_uri` and
   `app_secret_key_hex`). Re-sends the `connect` RPC; compliant signers
   that still have the session stored respond with `ack` and we're back
   online.

## Implementation order (lowest risk first)

1. Add `nip46_online` flag + Tauri event emission (no behavior change
   yet — just observable state).
2. Add heartbeat task (still no user-facing change; logs show the flag
   flipping).
3. Frontend banner reading the flag.
4. Short-circuit RPC timeouts on offline.
5. Auto-reconnect on focus + manual reconnect button.

Each step ships independently and each is a net improvement.

## Edge cases to consider

- **Heartbeat vs. user-triggered RPC interference** — if an RPC is in
  flight when the heartbeat fires, skip the heartbeat. Track "last
  successful response timestamp" and only ping if it's >60 s stale.
- **Offline during app launch** — `init_nostr_identity` restoring a
  persisted NIP-46 connection must not block startup if the signer is
  unreachable. Construct the node, flip `nip46_online=false`, let the
  app render, surface the banner.
- **Network transitions** — laptop sleep/wake, VPN toggle, flaky wifi.
  Auto-reconnect should debounce (no more than once per 10 s) and give
  up after N attempts until the user clicks.
- **Relay vs. signer offline** — a failed ping could mean the signer is
  down OR our relay pool is down. Differentiate by checking relay pool
  connection status before blaming the signer.

## Out of scope for this phase

- Multiple concurrent signers / switching between them.
- Persisted session quality metrics (success rate, avg latency).
- Notification-center-style popup when signer reconnects.

## Files likely to touch

- `src-tauri/src/nip46.rs` — heartbeat + online flag on `Nip46ManualSigner`
- `src-tauri/src/lib.rs` — possible `NodeState` shape change
- `src-tauri/src/commands.rs` — optional `reconnect_nip46` command
- `src/hooks/useTauriEvents.ts` — listen for `nip46:status`
- `src/store/index.ts` — `nip46Online: boolean`
- `src/components/layout/TopShell.tsx` — banner rendering
