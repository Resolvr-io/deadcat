# Deadcat.Live

Deadcat.Live is a desktop application for trading prediction markets on the [Liquid Network](https://liquid.net/). It provides a native, cross-platform experience for creating, trading, and settling binary outcome markets using covenant-based settlement.

## Quick Start

The only prerequisite is [Nix](https://nixos.org/). Install it with the [Determinate Nix Installer](https://github.com/DeterminateSystems/nix-installer):

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

Then clone and run:

```bash
git clone https://github.com/Resolvr-io/deadcat.git
cd deadcat
nix develop
just install
just dev
```

The app will open a native desktop window with the frontend dev server on `http://localhost:1420`.

## Features

- Browse and filter prediction markets by category (Bitcoin, Politics, Sports, Culture, Weather, Macro)
- Trade YES/NO outcome tokens with market or limit orders
- Issue, redeem, and cancel market positions
- Integrated Liquid wallet (create/import via mnemonic)
- Market discovery and settlement via Nostr

## Tech Stack

- **Frontend**: TypeScript, Vite, Tailwind CSS
- **Desktop Runtime**: [Tauri v2](https://v2.tauri.app/)
- **Wallet**: [LWK](https://github.com/Blockstream/lwk) (Liquid Wallet Kit)
- **Smart Contracts**: [Simplicity](https://github.com/BlockstreamResearch/simplicity) covenants on Liquid
- **Discovery**: [Nostr](https://nostr.com/) for market announcements

## Development

```bash
just dev              # Run in development mode with hot-reload
just tsc              # TypeScript type checking
just biome-lint       # Lint with Biome
just biome-format     # Format with Biome
just biome-fix        # Auto-fix with Biome
```

### Build

```bash
cargo tauri build     # Build native app bundle
```

### Screenshot Tests

```bash
just screenshots          # Run screenshot tests
just screenshots-update   # Update baseline screenshots
```

The Nix dev shell provides Chromium and sets `PUPPETEER_EXECUTABLE_PATH` automatically.

## License

MIT
