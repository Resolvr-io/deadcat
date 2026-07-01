# Deadcat Tauri to Web App Migration Plan

## Purpose

Migrate Deadcat from a Tauri desktop app into a browser web app while preserving
the current product surface: market discovery, comments, wallet display,
trading, market creation, pool operations, notifications, Lightning/Bitcoin
swap flows, and operator workflows.

This is a plan only. It does not prescribe immediate implementation changes.

## Reference Migration Pattern

Use a proven desktop-to-web migration pattern focused on browser-safe
boundaries:

- The original app stayed intact while a separate browser target was created.
- The browser target forked the UI and reshaped it around web-native routing,
  app state, local preferences, and browser-safe APIs.
- Tauri IPC calls were not carried into the web app. They were replaced by
  explicit data/provider boundaries.
- Backend and wallet dependencies were hidden behind typed interfaces and dev
  mocks so the browser UI could run before all production services were ready.
- Wallet custody moved out of the app. The web app became a dapp that connects
  to an external wallet provider instead of holding keys.
- Browser-only constraints were accepted directly: no local filesystem access,
  no app-data directory, no desktop event bus, and no in-app seed custody.

Deadcat should follow the same sequence, but its backend surface is much larger
than a simple portfolio application. Deadcat needs a real service boundary for
the Rust SDK and a stronger wallet/signer provider contract because market
operations construct and sign Liquid PSETs, not only simple withdrawals.

## Target Architecture

The final system should have four explicit layers:

1. `deadcat-web`
   - Browser React app.
   - Owns UI state, routes, local preferences, query caching, and rendering.
   - Does not import `@tauri-apps/*`.
   - Does not hold seed phrases, encrypted mnemonics, local nsecs, wallet DBs,
     or desktop filesystem paths.

2. `deadcat-api`
   - HTTP plus WebSocket or SSE service extracted from the current Tauri command
     layer.
   - Reuses `deadcat-sdk` and `deadcat-store`.
   - Owns market discovery, order/pool indexing, quote construction,
     transaction assembly, chain reads, relay subscriptions, and notification
     indexing.
   - Exposes typed endpoints that mirror current frontend use cases rather than
     exposing raw Tauri command names forever.

3. External wallet provider
   - Browser wallet provider with a Deadcat-compatible request/approval API.
   - Owns Liquid key custody, descriptor derivation, address generation,
     balance sync, PSET signing, wallet approval UX, and broadcast when
     appropriate.
   - Must use the standard Liquid BIP84 singlesig derivation already validated
     for Deadcat-compatible wallets.

4. External Nostr signer/provider
   - NIP-07, NIP-46, wallet-provided signing, or a Deadcat companion signer.
   - Owns Nostr private keys and signing approvals.
   - The web app should request signatures and decrypt/encrypt operations
     through a provider instead of storing local nsecs.

## Non-Negotiable Product Constraints

- Do not move Deadcat seed custody into the browser app.
- Do not persist NWC strings, mnemonics, nsecs, descriptors with private
  material, or wallet passwords in browser localStorage.
- Do not depend on browser direct Electrum TCP access.
- Do not require the web frontend to open local files or write downloads through
  Tauri plugins.
- Preserve the ability to run the web UI with dev mocks before the production
  provider/API stack is complete.
- Keep the Tauri app working until the web app passes the full feature parity
  matrix.

## Current Deadcat Surfaces to Replace

### Tauri IPC

Current frontend entry points include:

- `src/api/tauri.ts`
- direct `invoke(...)` calls in onboarding, wallet, settings, profile, detail
  pages, market queries, trading mutations, wallet mutations, and activity
  tracking
- `listen(...)` calls in `src/hooks/useTauriEvents.ts`,
  `src/queries/useNotifications.ts`, and `src/App.tsx`

All of these need to be routed through browser-safe providers:

- `src/data/deadcat-api.ts` for app/backend requests
- `src/data/wallet/provider.ts` for external wallet requests
- `src/data/nostr/provider.ts` for external Nostr signing
- `src/data/events.ts` for WebSocket/SSE subscriptions
- `src/lib/open-url.ts` and browser file helpers for Tauri plugin replacements

### Rust Tauri Layer

The following Tauri responsibilities need extraction into an API service or
provider layer:

- app state and network config from `src-tauri/src/state.rs`
- wallet lifecycle and sync from `src-tauri/src/lib.rs`
- Nostr identity and social commands from `src-tauri/src/commands.rs`
- market discovery, orders, comments, zaps, reactions, follows, mutes, and
  notifications from `src-tauri/src/commands.rs`
- payment and swap commands from `src-tauri/src/payment_commands.rs`
- filesystem-backed key, mnemonic, NWC, notification, relay cache, and local
  state persistence
- background wallet sync, relay subscription tasks, and Tauri event emission
- `deadcat-store` SQLite paths under the Tauri app data directory

### Desktop Plugins

Remove or replace these frontend dependencies:

- `@tauri-apps/api`
- `@tauri-apps/plugin-dialog`
- `@tauri-apps/plugin-fs`
- `@tauri-apps/plugin-log`
- `@tauri-apps/plugin-opener`

Browser replacements:

- URL opening: `window.open` with safe URL validation.
- File import: `<input type="file">` plus `FileReader`.
- File export: `Blob`, object URL, and explicit user download.
- Logging: browser console plus remote telemetry if desired.
- App close/quit flows: remove or convert to ordinary web session warnings.

## Proposed Project Shape

Start by adding a separate web target while preserving the desktop app. The
exact location can be a sibling app, a `web/` directory, or a separate
repository. The public requirement is separation of browser code from the Tauri
runtime, not a specific repository layout:

```text
deadcat-web/
  package.json
  vite.config.ts
  src/
    data/
    routes/
    components/
    app/

deadcat-api/
  Cargo.toml
  src/

existing-desktop-app/
  src-tauri/
  src/
```

If the repository should stay single-app at first, use a temporary `web/`
directory and migrate after the proof of concept. The important point is to
avoid deleting `src-tauri` until parity is proven.

## Phase 0: Inventory and Contracts

Goal: freeze the current behavioral surface before moving code.

Tasks:

- Generate a complete inventory of direct `invoke`, `listen`, and Tauri plugin
  imports.
- Classify each command as one of:
  - browser local UI preference
  - backend read
  - backend mutation
  - wallet provider request
  - Nostr signer request
  - background event stream
  - desktop-only behavior to remove
- Define TypeScript request/response contracts for each class.
- Define the first version of the Deadcat web provider contracts.
- Add mock fixtures for markets, orders, wallet snapshots, comments,
  notifications, pools, swaps, and profile state.
- Record feature parity requirements in a migration checklist.

Exit criteria:

- Every Tauri callsite has an owner in the target architecture.
- The team can grep for a marker such as `@mock-boundary` or `@api-boundary`
  to find every temporary mock or service swap point.
- No command is migrated without a typed contract.

## Phase 1: Create the Browser App Target

Goal: create a web app that runs independently of Tauri.

Tasks:

- Create `deadcat-web` or an equivalent browser app target.
- Start with Vite React unless routing/server-rendering requirements justify a
  heavier web framework.
- Decide whether to keep SPA routing initially or adopt TanStack Router/Start.
  A conservative path is Vite SPA first, TanStack Router after the IPC boundary
  is isolated.
- Copy only browser-safe frontend files from `src/`.
- Move shared UI components gradually, not through a one-time bulk rewrite.
- Add `src/data/demo.ts` or equivalent fixtures so the app can boot without the
  Rust backend.
- Add browser-safe equivalents for splash/loading, URL opening, file import,
  file export, and local preferences.
- Keep the existing desktop app unchanged.

Exit criteria:

- The browser app boots with mocked data.
- It has no `@tauri-apps/*` imports.
- It can render home, market detail, wallet shell, settings, profile, and
  notifications with fixtures.

## Phase 2: Extract a Typed Frontend API Boundary

Goal: make the current desktop frontend and the new web frontend consume the
same logical API surface.

Tasks:

- Replace `src/api/tauri.ts` with a domain API interface:
  - app/session API
  - markets API
  - trading API
  - pools API
  - comments/social API
  - notifications API
  - payments API
  - wallet API
  - Nostr identity API
- Move direct `invoke(...)` calls out of components into domain services.
- Keep a `tauriDeadcatApi` implementation for the desktop app.
- Add `mockDeadcatApi` for the browser app.
- Add `httpDeadcatApi` for the future API service.
- Convert query hooks and mutation hooks to depend on the domain API, not on
  Tauri.

Exit criteria:

- Components do not import `@tauri-apps/api/core`.
- React Query hooks call domain services.
- Desktop behavior is unchanged through the Tauri implementation.
- Browser mock behavior is available through the mock implementation.

## Phase 3: Replace the Tauri Event Bus

Goal: make app updates work in a browser.

Current Tauri events include:

- `app_state_updated`
- `wallet_sync_complete`
- `wallet_snapshot`
- `discovery:market`
- `discovery:market-refresh`
- `discovery:attestation`
- `discovery:pool`
- `discovery:order`
- `discovery:orders-invalidated`
- `notifications_updated`
- `close-requested`

Target replacements:

- WebSocket or SSE stream from `deadcat-api` for backend events.
- Provider events from the external wallet for account, balance, lock, and
  disconnect changes.
- Provider events from the Nostr signer for account, permission, and disconnect
  changes.
- Remove desktop window close events from the web target.

Tasks:

- Create `DeadcatEventSource` interface.
- Implement `tauriEventSource`, `mockEventSource`, and `webEventSource`.
- Keep React Query invalidation logic centralized, similar to the existing
  `useTauriEvents` hook.
- Add reconnect/backoff behavior for WebSocket/SSE.

Exit criteria:

- The web app can receive mocked event updates and invalidate queries.
- The event source can be swapped without component changes.
- No web component imports `@tauri-apps/api/event`.

## Phase 4: Define the External Wallet Provider

Goal: remove in-app wallet custody from the web app while preserving Deadcat
wallet features.

Minimum provider contract:

- `connect(): Promise<Account>`
- `disconnect(): Promise<void>`
- `getStatus(): Promise<{ locked: boolean }>`
- `getDescriptor(): Promise<{ descriptor: string; fingerprint: string }>`
- `getNewAddress(): Promise<{ index: number; address: string }>`
- `getBalance(): Promise<{ locked: boolean; assets: Record<string, number> }>`
- `getTransactions(): Promise<WalletTransaction[]>`
- `getUtxos(filter?: UtxoFilter): Promise<WalletUtxo[]>`
- `signPset(request: SignPsetRequest): Promise<SignedPset>`
- `signAndBroadcast(request: SignAndBroadcastRequest): Promise<{ txid: string }>`
- `send(params: { address: string; sats: number; assetId?: string }): Promise<{ txid: string }>`
- `on("accountChanged" | "balanceChanged" | "locked" | "unlocked" | "disconnect", handler)`
- `off(...)`

Deadcat-specific additions to validate:

- Whether market issuance, redemption, LMSR pool operations, and limit orders
  can be represented as generic PSET signing requests.
- Whether the provider must expose wallet-owned token/reissuance metadata.
- Whether fee estimation and coin selection live in the provider, API service,
  or both.
- Whether broadcast happens in the provider or in `deadcat-api` after signing.

Tasks:

- Use a typed external-provider interface with a development mock so the web UI
  can run before production wallet integration is complete.
- Align the descriptor with the BIP84 Liquid path already confirmed for
  Deadcat-compatible wallets.
- Remove seed generation, mnemonic restore, wallet password, and encrypted
  mnemonic flows from the web app.
- Replace wallet creation/restoration screens with connect-wallet and install
  companion-wallet screens.
- Keep existing desktop wallet flows only in the Tauri app until decommission.

Exit criteria:

- The web app can connect to a mock provider and render wallet state.
- The web app has no seed phrase or wallet password UX.
- All wallet operations go through the provider contract.

## Phase 5: Define the Nostr Signer Provider

Goal: remove local Nostr private key custody from the web app.

Provider options:

- NIP-07 browser extension.
- NIP-46 remote signer.
- Wallet or Deadcat companion signer exposing Nostr signing.
- Backend-assisted NIP-46 session broker, without backend custody of private
  keys.

Required capabilities:

- return public key / npub
- sign Nostr events
- NIP-44 encrypt/decrypt where required by mutes, backups, and private data
- NIP-98 auth event creation
- connect/disconnect and permission status

Tasks:

- Create `src/data/nostr/provider.ts`.
- Move profile, comments, zaps, reactions, follows, mutes, relay list, source
  npub, and backup operations behind a signer-aware API boundary.
- Rework backup flows. Browser web should not export or import raw local key
  files through Tauri filesystem APIs.
- Decide if Nostr backup of wallet mnemonic remains a web feature. If it does,
  backup encryption and decryption must happen inside the wallet/signer
  provider, not inside the browser app with raw secret material.

Exit criteria:

- The browser app can publish comments and reactions through a mock signer.
- Nostr identity UI is connect/disconnect based.
- No web code stores nsecs or reads identity files.

## Phase 6: Extract the Rust API Service

Goal: move current Tauri command behavior into a deployable service.

Recommended service stack:

- Rust HTTP server using `axum` or equivalent.
- WebSocket or SSE endpoint for events.
- Reuse `deadcat-sdk` for contract, PSET, discovery, and wallet-independent
  logic.
- Reuse `deadcat-store`, but make the store path and database lifecycle
  service-managed instead of Tauri app-data-managed.

Extraction strategy:

- Move pure command logic into service structs that can be called by both Tauri
  commands and HTTP handlers.
- Keep thin Tauri wrappers during transition.
- Add HTTP handlers after the service structs exist.
- Avoid duplicating market logic in TypeScript.

Endpoint groups:

- `/app/state`
- `/network`
- `/markets`
- `/markets/:id/orders`
- `/markets/:id/comments`
- `/trades/quote`
- `/trades/execute`
- `/orders`
- `/pools`
- `/wallet/pset/*`
- `/notifications`
- `/relays`
- `/payments/boltz/*`
- `/events`

Security requirements:

- Browser clients must authenticate to mutation endpoints.
- Mutation endpoints must bind requests to the connected wallet/signer identity.
- Server must never receive seed phrases.
- PSET signing must require wallet provider approval.
- CORS and origin policy must be explicit.
- Rate limits are required for relay-heavy, quote-heavy, and mutation endpoints.

Exit criteria:

- `deadcat-api` can serve read-only market discovery to the web app.
- Event streaming works for discovery and notifications.
- Desktop Tauri wrappers can still call the extracted service layer locally.

## Phase 7: Port Feature Areas in Order

Port features in this order to reduce risk:

1. Read-only shell
   - Home, market lists, group views, market detail, profile shell, settings
     shell.
   - Uses fixtures first, then `deadcat-api` read endpoints.

2. Market discovery and orders
   - `discover_contracts`, `list_contracts`, `fetch_orders`,
     `list_own_orders`, price history, pool lists.
   - Replace relay cache file with service-side cache and browser query cache.

3. Comments and social reads
   - comments, zaps, reactions, follows, mutes, profile reads.
   - Add signer-backed mutations only after reads are stable.

4. Wallet read surface
   - connect external provider, balance, transactions, UTXOs, receive address,
     locked state, account switch.

5. Comments and social writes
   - publish comment, delete comment, react, follow, mute, profile publish.
   - Requires external Nostr signing.

6. Trading
   - quote trade from API.
   - execute trade via API-built PSET and provider signing.
   - invalidate markets, wallet, orders, and pool queries through web events.

7. Market lifecycle operations
   - create market onchain, publish contract, issue/cancel/redeem/resolve.
   - Requires explicit approval UX for complex PSETs.

8. LMSR pool operations
   - table generation can be browser WASM or API-side; choose after measuring.
   - create, scan, adjust, close pools through API and wallet provider signing.

9. Payments and swaps
   - Lightning receive/pay and Bitcoin chain swaps.
   - Decide whether Boltz calls live in `deadcat-api`, wallet provider, or a
     dedicated payment service.
   - Do not persist NWC credentials in browser localStorage.

10. Notifications
   - Move notification indexing to `deadcat-api`.
   - Deliver unread counts and notification updates over WebSocket/SSE.

Exit criteria:

- Each feature area has parity tests before the next riskier area starts.
- The web app remains deployable at the end of every phase.

## Phase 8: Browser Persistence Model

Replace Tauri app-data persistence with:

- localStorage for harmless UI preferences only
- IndexedDB for larger non-secret browser caches if needed
- service database for market/order/pool/comment/notification indexes
- wallet provider storage for wallet secrets and wallet sync state
- signer provider storage for Nostr secrets and permissions

Current desktop files that need web equivalents or removal:

- `config.json`
- `local_state.json`
- `source_config.json`
- `relay_market_cache.json`
- per-network `deadcat.db`
- wallet mnemonic file
- NWC file
- NIP-46 connection file
- notification files
- own-events files
- identity backup/import files

Exit criteria:

- Browser storage contains no secrets.
- Clearing site data produces a recoverable disconnected state.
- API database backup/restore is documented separately from browser state.

## Phase 9: Build, Test, and Deployment

Web build tasks:

- Remove Tauri env assumptions from Vite config.
- Use ordinary `VITE_` environment variables.
- Add production API base URL configuration.
- Add provider availability detection.
- Add CI for web typecheck, lint, unit tests, and build.

Test matrix:

- no wallet provider installed
- provider installed but locked
- provider connected and unlocked
- account change
- network mismatch
- API offline
- relay outage
- chain backend outage
- market read-only mode
- signer unavailable
- failed signing approval
- rejected transaction approval
- transaction broadcast failure
- notification stream reconnect
- mobile browser layout

Security tests:

- no secrets in localStorage/sessionStorage/IndexedDB
- no mnemonic or nsec in network requests
- CORS rejects unknown origins
- mutation endpoints reject unauthenticated clients
- PSET approval text matches requested action
- Nostr event signing prompts show event kind and target

Exit criteria:

- Web build is reproducible in CI.
- Browser app passes parity smoke tests.
- Security review finds no browser secret custody regressions.

## Phase 10: Decommission Tauri

Only start this phase after feature parity is accepted.

Tasks:

- Freeze new Tauri-only feature work.
- Add an in-app desktop migration notice if existing users need to move to the
  web app plus companion wallet.
- Provide an export/sweep path for any desktop-only wallet state that cannot be
  recovered by the standard seed path.
- Remove desktop-only commands from active frontend code.
- Remove `src-tauri`, Tauri dependencies, and desktop packaging from the web
  product branch after a final archive tag.
- Keep `deadcat-sdk` and `deadcat-store` as shared Rust crates for the API.

Exit criteria:

- Existing users have a documented migration path.
- No production workflow depends on Tauri IPC.
- Web app plus API plus provider covers the accepted product surface.

## Provider and API Mock Policy

Mirror the generic provider-boundary pattern:

- Every incomplete backend/provider dependency gets a typed dev mock.
- Mock files must be clearly marked and greppable.
- Mocks should exercise realistic states, including locked wallet, empty
  balances, multiple assets, relay outage, failed approvals, and stale data.
- Production builds should not silently fall back to mocks unless explicitly
  configured.

Suggested markers:

- `@mock-boundary`
- `@api-boundary`
- `@provider-boundary`
- `@tauri-removal`

## Main Risks

- Deadcat has real custody and signing flows. The web app must not become a
  browser seed wallet by accident.
- Some current Rust operations assume local wallet state and direct Electrum
  access. Those need clean separation between PSET construction, signing, and
  broadcast.
- Current notification and relay caches are file-backed. Web deployment needs
  service-managed persistence.
- Current components contain direct `invoke(...)` calls. Leaving those in place
  will make the web fork drift quickly.
- Trading and LMSR pool operations may need provider capabilities beyond simple
  wallet balance, receive, and send flows.
- API authentication and wallet/signer identity binding are product-critical,
  not deployment details.

## Open Decisions

- Should Deadcat Web be an SPA, or should it adopt a server-capable React
  framework?
- Should `deadcat-api` be public multi-user infrastructure, a user-run local
  service, or both?
- Should market/order/pool PSET construction happen entirely API-side, partly
  provider-side, or partly in browser WASM?
- Should broadcast happen through the wallet provider or through `deadcat-api`
  after provider signing?
- Should Nostr signing be provided by NIP-07, NIP-46, wallet-provided signing,
  or a new Deadcat companion provider?
- What is the migration path for existing Tauri wallet files, NWC files, and
  Nostr identity files?
- Which desktop-only features should be removed rather than ported?

## First Three Implementation PRs

1. API boundary cleanup in the existing app
   - Add domain API interfaces.
   - Move direct `invoke(...)` calls out of components.
   - Keep behavior unchanged through a Tauri implementation.

2. Browser web shell with mocks
   - Add the separate web app target.
   - Copy the UI shell and core routes.
   - Use mock data and mock providers.
   - Prove there are no Tauri imports.

3. Read-only backend service
   - Extract read-only market discovery and order listing from Tauri commands
     into reusable Rust service code.
   - Add HTTP endpoints and event streaming.
   - Wire the web app read-only market views to the service.

These PRs keep risk low and match the reusable migration method: isolate the
browser app first, prove the boundary with mocks, then replace mocks with real
service/provider implementations one feature area at a time.
