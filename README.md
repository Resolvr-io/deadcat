# Deadcat.Live

Deadcat.Live is a Tauri v2 desktop application for trading prediction markets on the [Liquid Network](https://liquid.net/) (a Bitcoin sidechain). It provides a native, cross-platform desktop experience for creating, trading, and settling binary outcome markets.

## What is Deadcat?

Deadcat is a prediction market platform built on the Liquid Network using covenant-based settlement. It enables:

- **Binary Markets**: Create YES/NO outcome markets across categories (Bitcoin, Politics, Sports, Culture, Weather, Macro)
- **Trading**: Buy and sell outcome tokens using market or limit orders
- **Issuance & Settlement**: Issue new market tokens, resolve markets via oracle, and redeem winnings
- **Liquid Wallet**: Integrated wallet management via LWK (Liquid Wallet Kit)

## Features

- Browse and filter prediction markets by category
- View detailed market information including orderbook, prices, and covenant state
- Trade YES/NO outcome tokens with market or limit orders
- Issue, redeem, and cancel market positions
- Integrated Liquid wallet (create/import via mnemonic)
- Real-time chain tip tracking via Blockstream API

## Tech Stack

- **Frontend**: TypeScript (vanilla DOM, no framework)
- **Desktop Runtime**: [Tauri v2](https://v2.tauri.app/)
- **Styling**: [Tailwind CSS](https://tailwindcss.com/)
- **Wallet**: [LWK](https://github.com/Blockstream/lwk) (Liquid Wallet Kit) for wallet and signer operations
- **Build System**: Vite + Cargo (via Tauri CLI)

## Prerequisites

- [Node.js](https://nodejs.org/) (v20+)
- [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install) (1.77.2+ stable)
- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform
- [just](https://github.com/casey/just) command runner (optional but recommended)

Alternatively, use [Nix](https://nixos.org/) to get a complete dev shell:

```bash
nix develop
```

## Getting Started

### Install Dependencies

```bash
pnpm install
```

### Development

Run the app in development mode with hot-reload:

```bash
# Using just (recommended)
just dev

# Or directly
cargo tauri dev
```

The frontend dev server runs on `http://localhost:1420`.

### Build

Build the production desktop application:

```bash
pnpm run build        # Build frontend only
cargo tauri build     # Build native app bundle
```

### Code Quality

```bash
just tsc              # TypeScript type checking
just biome-lint       # Lint with Biome
just biome-format     # Format with Biome
just biome-fix        # Auto-fix with Biome
```

### Screenshot Tests

```bash
just screenshots          # Run screenshot tests
just screenshots-update   # Update baseline screenshots
```

Requires Chromium/Chrome. The Nix dev shell sets `PUPPETEER_EXECUTABLE_PATH` automatically.

## Project Structure

```
deadcat/
├── src/                    # Frontend source
│   ├── main.ts            # Application entry point (all app logic)
│   └── style.css          # Design tokens and Tailwind directives
├── src-tauri/             # Tauri/Rust backend source
│   ├── src/
│   │   ├── lib.rs         # Tauri command handlers (wallet/signer)
│   │   └── main.rs        # Rust entry point
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri app configuration
├── index.html             # HTML entry point
├── package.json           # Node.js dependencies
├── justfile               # Task runner commands
├── flake.nix              # Nix development environment
├── vite.config.ts         # Vite build configuration
└── tailwind.config.js     # Tailwind CSS configuration
```

## Tauri Commands

The Rust backend exposes these commands to the frontend via Tauri IPC:

| Command | Description |
|---------|-------------|
| `create_software_signer` | Generate or import a wallet mnemonic |
| `create_wollet` | Create a Liquid wallet instance |
| `wallet_new_address` | Derive a new receiving address |
| `wallet_signer_id` | Get the signer ID for a wallet |
| `fetch_chain_tip` | Get current blockchain height/hash |

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes and ensure code quality checks pass (`just tsc`, `just biome-lint`)
4. Commit your changes
5. Open a pull request against `main`

