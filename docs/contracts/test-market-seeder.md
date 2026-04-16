# Test Market Seeder

Seed testnet prediction markets with real on-chain contracts and Nostr announcements.

## Prerequisites

- **DEADCAT_SOURCE_NSEC** — nsec for the source npub (signs events + acts as oracle)
- **DEADCAT_MNEMONIC** — BIP-39 mnemonic for a funded Liquid testnet wallet

Optional:
- **DEADCAT_ELECTRUM_URL** — override default testnet Electrum (`ssl://blockstream.info:465`)
- **DEADCAT_DATADIR** — wallet data directory (default: `/tmp/deadcat-seed`)

## Commands

Run from the SDK crate directory:

```bash
cd src-tauri/crates/deadcat-sdk
```

### List existing markets

```bash
DEADCAT_SOURCE_NSEC=nsec1... \
  cargo run --example seed_markets --features testing -- list
```

### Publish markets

Publish all markets (40 total):

```bash
DEADCAT_SOURCE_NSEC=nsec1... DEADCAT_MNEMONIC="word1 word2 ..." \
  cargo run --example seed_markets --features testing -- publish
```

Publish only the first N markets:

```bash
DEADCAT_SOURCE_NSEC=nsec1... DEADCAT_MNEMONIC="word1 word2 ..." \
  cargo run --example seed_markets --features testing -- publish 5
```

The first 5 markets are one per category (Bitcoin, Politics, Sports, Culture, Macro) for quick visual testing.

### Delete all markets

Publishes tombstone replacements that make the events unparseable:

```bash
DEADCAT_SOURCE_NSEC=nsec1... \
  cargo run --example seed_markets --features testing -- delete
```

## Notes

- Each market creation requires **2 L-BTC UTXOs** and takes ~1 minute (one Liquid block) between markets for the change output to confirm.
- The seeder automatically splits UTXOs if only 1 is available, and retries sync up to 90 seconds between markets.
- The seeder fetches the real Liquid testnet chain tip for accurate expiry height estimation.
- Collateral per token is 5,000 sats for all test markets.
- All markets are published by the source npub and use the same key as oracle for resolution.

## Test Markets

| # | Category | Question |
|---|----------|----------|
| 1 | Bitcoin | Will Bitcoin exceed $200k by end of 2026? |
| 2 | Politics | Will the US pass stablecoin legislation in 2026? |
| 3 | Sports | Will Lewis Hamilton win a race for Ferrari in 2026? |
| 4 | Culture | Will GTA VI be released in 2026? |
| 5 | Macro | Will the Fed cut rates below 3% by end of 2026? |
| 6 | Bitcoin | Will Bitcoin dominance drop below 50% in 2026? |
| 7 | Bitcoin | Will a Bitcoin spot ETF surpass $100B AUM? |
| 8 | Bitcoin | Will the Bitcoin halving cycle peak occur before Q3 2026? |
| 9 | Bitcoin | Will Ethereum flip Bitcoin in market cap? |
| 10 | Politics | Will the EU implement MiCA enforcement actions in 2026? |
| 11 | Politics | Will a G7 country adopt a Bitcoin strategic reserve? |
| 12 | Politics | Will El Salvador repay its 2027 bonds early? |
| 13 | Sports | Will Real Madrid win the 2025-26 Champions League? |
| 14 | Sports | Will the Kansas City Chiefs three-peat at Super Bowl LXI? |
| 15 | Sports | Will a new 100m world record be set in 2026? |
| 16 | Culture | Will a Nostr client reach 10M monthly active users? |
| 17 | Culture | Will an AI model pass the Turing test in 2026? |
| 18 | Culture | Will a major studio release a fully AI-generated film? |
| 19 | Culture | Will Wikipedia adopt Nostr for contributor identity? |
| 20 | Weather | Will 2026 be the hottest year on record? |
| 21 | Weather | Will a Category 6 hurricane classification be adopted? |
| 22 | Macro | Will gold exceed $3,500/oz in 2026? |
| 23 | Macro | Will US national debt exceed $40 trillion? |
| 24 | Bitcoin | Will Bitcoin transaction fees average over $50 in Q4 2026? |
| 25 | Bitcoin | Will Lightning Network capacity exceed 10,000 BTC? |
| 26 | Bitcoin | Will a Bitcoin L2 reach $1B TVL? |
| 27 | Politics | Will Texas pass a state Bitcoin reserve bill? |
| 28 | Politics | Will the SEC approve a Solana spot ETF in 2026? |
| 29 | Politics | Will China lift its cryptocurrency trading ban? |
| 30 | Sports | Will the US win the most gold medals at the 2026 Winter Olympics? |
| 31 | Sports | Will Lionel Messi retire from professional football in 2026? |
| 32 | Sports | Will an NBA team complete a 73+ win regular season? |
| 33 | Culture | Will a decentralized social protocol surpass 50M users? |
| 34 | Culture | Will an AI-generated song reach #1 on Billboard Hot 100? |
| 35 | Culture | Will GTA VI be released in 2026? |
| 36 | Weather | Will Arctic sea ice hit a new record minimum in 2026? |
| 37 | Weather | Will a US city record a temperature above 130F? |
| 38 | Macro | Will US unemployment exceed 5% in 2026? |
| 39 | Macro | Will the S&P 500 exceed 7,000 in 2026? |
| 40 | Macro | Will the US 10-year Treasury yield exceed 6%? |
