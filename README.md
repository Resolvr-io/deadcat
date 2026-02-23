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
- Integrated Liquid wallet with mnemonic encryption (create, import, or restore from Nostr backup)
- Encrypted wallet backup to Nostr relays (NIP-44 self-encryption + NIP-78 addressable storage)
- User-configurable relay list with NIP-65 relay list metadata
- Automatic relay backup scanning during onboarding
- Nostr profile display (kind 0 metadata)
- Market discovery and settlement via Nostr
- Lightning, Liquid, and Bitcoin payment flows via Boltz swaps

## Nostr Protocol Usage

| NIP | Kind | Purpose |
|-----|------|---------|
| [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) | — | Basic protocol: event signing, relay communication, subscriptions |
| [NIP-19](https://github.com/nostr-protocol/nips/blob/master/19.md) | — | Bech32 encoding for keys and identifiers (`npub`, `nsec`) |
| [NIP-44](https://github.com/nostr-protocol/nips/blob/master/44.md) | — | Versioned encryption (XChaCha20 + secp256k1 ECDH) for wallet backup |
| [NIP-65](https://github.com/nostr-protocol/nips/blob/master/65.md) | 10002 | Relay list metadata — user-configurable relay preferences with read/write markers |
| [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) | 30078 | Application-specific data — encrypted wallet mnemonic backup storage |
| Kind 0 | 0 | User profile metadata — profile picture, display name |

### Wallet Backup Flow

1. User's recovery phrase is encrypted locally using NIP-44 (self-encryption to own public key)
2. Encrypted payload is published as a kind 30078 addressable event with d-tag `deadcat-wallet-backup`
3. The event is sent to all configured relays for redundancy
4. On restore, the app fetches the event from relays and decrypts locally using the user's private key

Only the holder of the corresponding `nsec` can decrypt the backup. Relay operators see only opaque ciphertext.

## Tech Stack

- **Frontend**: TypeScript, Vite, Tailwind CSS
- **Desktop Runtime**: [Tauri v2](https://v2.tauri.app/)
- **Wallet**: [LWK](https://github.com/Blockstream/lwk) (Liquid Wallet Kit)
- **Swaps**: [Boltz](https://boltz.exchange/) for Lightning/Bitcoin/Liquid cross-chain swaps
- **Smart Contracts**: [Simplicity](https://github.com/BlockstreamResearch/simplicity) covenants on Liquid
- **Nostr**: [nostr-sdk](https://github.com/rust-nostr/nostr) 0.38 for relay communication, encryption, and event management

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
