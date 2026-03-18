# Mock Markets for Screenshots

Example prediction markets to populate the UI for demos and screenshots.

| # | Question | Category | Yes Price | 24h Change | Volume (BTC) | Liquidity (BTC) |
|---|----------|----------|-----------|------------|--------------|-----------------|
| 1 | Will Bitcoin exceed $150k by end of 2026? | Bitcoin | 72% | +3.4% | 2.45 | 8.12 |
| 2 | Will the US pass a Bitcoin strategic reserve bill in 2026? | Politics | 38% | -5.2% | 5.80 | 14.30 |
| 3 | Will a major hurricane hit Florida before October 2026? | Weather | 61% | +1.8% | 1.20 | 3.50 |
| 4 | Will the Fed cut rates below 3% by Q4 2026? | Macro | 29% | -1.5% | 3.10 | 9.70 |
| 5 | Will Bitcoin dominance exceed 65% this year? | Bitcoin | 55% | +8.2% | 1.80 | 5.40 |
| 6 | Will a Champions League final go to penalties in 2026? | Sports | 22% | +0.5% | 0.90 | 2.10 |
| 7 | Will a new Satoshi Nakamoto documentary air on Netflix in 2026? | Culture | 15% | -2.8% | 0.40 | 1.30 |
| 8 | Will Lightning Network capacity exceed 10,000 BTC? | Bitcoin | 44% | +6.1% | 1.50 | 4.80 |
| 9 | Will El Salvador's Bitcoin bonds be fully subscribed? | Macro | 67% | -4.1% | 2.00 | 6.20 |
| 10 | Will MicroStrategy hold over 500,000 BTC by year end? | Bitcoin | 81% | +2.1% | 4.20 | 11.50 |
| 11 | Will the SEC approve a spot Ethereum ETF staking amendment? | Macro | 45% | -3.3% | 3.60 | 7.80 |
| 12 | Will a nation-state besides El Salvador adopt BTC as legal tender? | Politics | 12% | +1.2% | 0.85 | 2.40 |
| 13 | Will the 2026 FIFA World Cup final have over 2.5 goals? | Sports | 58% | +0.9% | 1.10 | 3.20 |
| 14 | Will a Category 5 typhoon hit Japan before November 2026? | Weather | 34% | -0.7% | 0.70 | 1.90 |
| 15 | Will Nostr reach 10 million monthly active users in 2026? | Culture | 18% | +4.5% | 0.55 | 1.60 |
| 16 | Will Bitcoin mining difficulty exceed 120T before Q3 2026? | Bitcoin | 63% | +1.4% | 1.90 | 5.10 |
| 17 | Will the US 10-year Treasury yield drop below 3.5%? | Macro | 24% | -2.1% | 2.70 | 8.40 |
| 18 | Will a European country ban proof-of-work mining in 2026? | Politics | 9% | -0.4% | 0.35 | 0.95 |
| 19 | Will the Bitcoin mempool clear to under 1 sat/vB for a full week? | Bitcoin | 41% | +3.8% | 1.30 | 3.90 |
| 20 | Will a major AI lab accept Bitcoin payments by end of 2026? | Culture | 52% | +7.3% | 2.15 | 6.00 |

## Details

All markets use:
- **State:** 1 (live/unresolved)
- **Collateral per token:** 100 sats
- **Current height:** 885,000
- **Expiry heights:** 900,000–950,000

### Resolution Sources

| # | Resolution Source |
|---|-----------------|
| 1 | Coinbase spot price |
| 2 | Congressional Record |
| 3 | National Hurricane Center |
| 4 | Federal Reserve press release |
| 5 | CoinGecko |
| 6 | UEFA official results |
| 7 | Netflix catalog |
| 8 | mempool.space Lightning explorer |
| 9 | El Salvador Ministry of Finance |
| 10 | MicroStrategy quarterly filings (SEC EDGAR) |
| 11 | SEC filing / Federal Register |
| 12 | Official government gazette of adopting nation |
| 13 | FIFA official match report |
| 14 | Japan Meteorological Agency |
| 15 | Nostr.com stats page |
| 16 | mempool.space mining dashboard |
| 17 | US Treasury Department / FRED |
| 18 | Official Gazette of the legislating country |
| 19 | mempool.space fee estimates |
| 20 | Official announcement from the AI lab |

### Descriptions

1. Resolves YES if BTC/USD spot price on any major exchange exceeds $150,000 before Jan 1, 2027.
2. Resolves YES if the US Congress passes legislation establishing a national Bitcoin reserve before Dec 31, 2026.
3. Resolves YES if a Category 3+ hurricane makes landfall in Florida before Oct 31, 2026.
4. Resolves YES if the Federal Funds Rate target is below 3.00% at any point before Dec 31, 2026.
5. Resolves YES if Bitcoin market cap dominance exceeds 65% on CoinGecko at any point in 2026.
6. Resolves YES if the 2025-26 UEFA Champions League final is decided by a penalty shootout.
7. Resolves YES if Netflix releases a documentary primarily about Satoshi Nakamoto's identity before Dec 31, 2026.
8. Resolves YES if total Lightning Network capacity exceeds 10,000 BTC at any point in 2026.
9. Resolves YES if El Salvador's Volcano Bonds reach full subscription before their offering closes.
10. Resolves YES if MicroStrategy's Bitcoin holdings exceed 500,000 BTC as reported in any quarterly filing before Dec 31, 2026.
11. Resolves YES if the SEC approves an amendment allowing staking within any spot Ethereum ETF before Dec 31, 2026.
12. Resolves YES if any sovereign nation besides El Salvador enacts legislation making Bitcoin legal tender before Dec 31, 2026.
13. Resolves YES if the 2026 FIFA World Cup final match ends with a combined total of 3 or more goals (excluding penalties).
14. Resolves YES if a Category 5 super typhoon makes landfall in Japan before Nov 1, 2026.
15. Resolves YES if Nostr protocol reaches 10 million monthly active users on any tracking service before Dec 31, 2026.
16. Resolves YES if Bitcoin network mining difficulty exceeds 120 trillion before Jul 1, 2026.
17. Resolves YES if the US 10-year Treasury yield closes below 3.50% on any trading day before Dec 31, 2026.
18. Resolves YES if any EU member state enacts a ban on proof-of-work cryptocurrency mining before Dec 31, 2026.
19. Resolves YES if the Bitcoin mempool recommended fee stays at or below 1 sat/vB for 7 consecutive days in 2026.
20. Resolves YES if any of the top 5 AI labs (by compute) publicly accepts Bitcoin as a payment method before Dec 31, 2026.
