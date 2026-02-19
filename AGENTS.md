# Agents

## Build & Development Commands
- Install dependencies: `just install` or `pnpm install`
- Dev server: `just dev` or `cargo tauri dev`
- Frontend only: `pnpm dev` (runs on localhost:1420)
- Build frontend: `pnpm run build`
- Build native app: `cargo tauri build`
- TypeScript check: `just tsc`
- Lint: `just biome-lint`
- Format: `just biome-format`
- Auto-fix: `just biome-fix`
- Screenshot tests: `just screenshots`
- Update screenshots: `just screenshots-update`
- Nix dev shell: `nix develop` (provides all dependencies)

## Code Style & Conventions
- **Language**: TypeScript (frontend), Rust (backend/Tauri)
- **Frontend**: Vanilla TypeScript with DOM manipulation (no framework — single `render()` function rebuilds the DOM on state change)
- **Styling**: Tailwind CSS with custom design tokens (golden-ratio spacing, custom typography scale) defined in `src/style.css`
- **Formatting/Linting**: Biome (not Prettier/ESLint)
- **Types**: All type definitions live at the top of `src/main.ts`
- **State**: Global `state` object; mutate state then call `render()` to update UI
- **Event handling**: Single event delegation on `#app` element using `data-*` attributes for actions
- **Naming**: camelCase for variables/functions, PascalCase for types, UPPER_SNAKE_CASE for constants
- **Rust**: Edition 2021, minimum rustc 1.77.2; Tauri commands in `src-tauri/src/lib.rs`
- **Error handling**: Rust commands return `Result<T, String>`; frontend uses try/catch with `invoke()`

## Architecture
- **Frontend entry**: `src/main.ts` — single-file app containing all state, types, rendering, and event handling
- **Backend entry**: `src-tauri/src/lib.rs` — Tauri command handlers for wallet operations (LWK)
- **IPC**: Frontend calls Rust backend via `@tauri-apps/api/core` `invoke()`
- **Wallet**: LWK (`lwk_signer`, `lwk_wollet`) for Liquid Network wallet/signer management
- **Data**: Market data is currently mock data embedded in `main.ts`
- **Blockchain**: Blockstream esplora API for chain tip queries

## Key Types
- `Market`: Core prediction market with covenant state, asset IDs, prices, UTXOs
- `CovenantState`: 0–3 representing settlement stages
- `Side`: "yes" | "no" outcome tokens
- `OrderType`: "market" | "limit"
- `ActionTab`: "trade" | "issue" | "redeem" | "cancel"
- `WalletNetwork`: "liquid" | "liquid-testnet" | "liquid-regtest"
