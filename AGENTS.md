# Agents

## Mandatory CI Checks

All agents MUST pass these checks before committing or creating PRs. Do NOT push code that fails any of these.

```bash
# TypeScript type checking
npx tsc --noEmit

# Biome lint + format (must pass with zero errors)
npx @biomejs/biome check --error-on-warnings .

# Rust formatting
cd src-tauri && cargo fmt --all -- --check

# Rust compilation
cd src-tauri && cargo check
```

Fix any issues before committing. Formatting fixes should be a separate `style:` commit.

## Build & Development Commands

**All commands must be run through the Nix dev shell:**
```
nix develop --command just {command}
```

- Install dependencies: `nix develop --command just install`
- Dev server: `nix develop --command just dev`
- Frontend only: `pnpm dev` (runs on localhost:1420)
- Build native app: `cargo tauri build`
- TypeScript check: `nix develop --command just tsc`
- Lint: `nix develop --command just biome-lint`
- Format: `nix develop --command just biome-format`
- Auto-fix: `nix develop --command just biome-fix`
- Rust format check: `nix develop --command just cargo-fmt`
- Rust clippy: `nix develop --command just cargo-clippy`
- Rust tests: `nix develop --command just cargo-test`

## Code Style & Conventions

- **Language**: TypeScript (frontend), Rust (backend/Tauri)
- **Frontend**: React with Zustand state management, TanStack Query for data fetching
- **Styling**: Tailwind CSS with custom design tokens defined in `src/style.css`
- **Formatting/Linting**: Biome v2.x — use `npx @biomejs/biome` (NOT `npx biome`, which resolves to old 0.3.3)
- **Types**: Type definitions in `src/types.ts`, store slices in `src/store/index.ts`
- **Naming**: camelCase for variables/functions, PascalCase for types/components, UPPER_SNAKE_CASE for constants
- **Rust**: Edition 2021, minimum rustc 1.77.2 (do not use `std::sync::LazyLock` — use `once_cell::sync::Lazy`)
- **Error handling**: Rust commands return `Result<T, String>`; frontend uses try/catch with `invoke()`

## Commit Conventions

- Use conventional commit prefixes: `feat:`, `fix:`, `refactor:`, `style:`, `docs:`
- Do NOT add Claude as co-author or contributor
- Do NOT commit or push without explicit user permission
- Do NOT push to master/main without explicit permission
- Do not use nostr.band (defunct)

## Architecture

- **Tauri v2** desktop app: TypeScript frontend + Rust backend
- **Frontend entry**: `src/App.tsx` — React app with component tree
- **Backend entry**: `src-tauri/src/lib.rs` — Tauri command handlers
- **SDK**: `src-tauri/crates/deadcat-sdk/` — Liquid wallet (LWK 0.14), Nostr discovery, prediction market covenants
- **Store**: `src-tauri/crates/deadcat-sdk/deadcat-store/` — SQLite persistence via Diesel
- **IPC**: Frontend calls Rust backend via `@tauri-apps/api/core` `invoke()`
- **Nostr**: Kind 30078 (NIP-78) for market announcements, orders, pools, attestations
- **Blockchain**: Blockstream esplora API for chain tip, Electrum for wallet sync

## Key Types

- `Market`: Core prediction market with covenant state, asset IDs, prices, UTXOs
- `CovenantState`: 0=Dormant, 1=Unresolved, 2=ResolvedYes, 3=ResolvedNo, 4=Expired
- `Side`: "yes" | "no" outcome tokens
- `OrderType`: "market" | "limit"
- `WalletNetwork`: "liquid" | "liquid-testnet" | "liquid-regtest"
