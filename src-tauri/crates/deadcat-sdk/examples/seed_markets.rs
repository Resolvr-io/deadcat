//! Seed testnet prediction markets with real on-chain contracts.
//!
//! Usage:
//!   cargo run --example seed_markets --features testing -- publish
//!   cargo run --example seed_markets --features testing -- publish 1    (single market)
//!   cargo run --example seed_markets --features testing -- list
//!   cargo run --example seed_markets --features testing -- delete
//!
//! Required environment variables:
//!   DEADCAT_SOURCE_NSEC  — nsec for the source npub (signs events + acts as oracle)
//!   DEADCAT_MNEMONIC     — BIP-39 mnemonic for a funded Liquid testnet wallet
//!
//! Optional:
//!   DEADCAT_ELECTRUM_URL — override default testnet Electrum (ssl://blockstream.info:465)
//!   DEADCAT_DATADIR      — wallet data directory (default: /tmp/deadcat-seed)

use std::sync::Arc;
use std::time::Duration;

// Ensure rustls uses the ring crypto backend (matches the main app's Cargo.toml).
use rustls as _;
fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

use deadcat_sdk::{
    ContractMetadata, DeadcatNode, DiscoveryConfig, MinerFeePolicy, Network, NoopStore, TxOptions,
    DEFAULT_RELAYS, NETWORK_TAG,
};
use nostr_sdk::prelude::*;

// ---------------------------------------------------------------------------
// Market definitions
// ---------------------------------------------------------------------------

struct MarketDef {
    question: &'static str,
    description: &'static str,
    category: &'static str,
    resolution_source: &'static str,
    /// Collateral per token pair in sats
    cpt_sats: u64,
    /// Settlement deadline as Unix timestamp
    settlement_unix: u64,
}

// Settlement ~6 months from mid-2026
const DEC_2026_UNIX: u64 = 1_798_761_600; // 2027-01-01 00:00 UTC
const JUN_2027_UNIX: u64 = 1_814_400_000; // ~mid 2027

const MARKETS: &[MarketDef] = &[
    // Bitcoin
    MarketDef {
        question: "Will Bitcoin exceed $200k by end of 2026?",
        description: "Resolves YES if BTC/USD spot price on any major exchange exceeds $200,000 at any point before January 1, 2027 00:00 UTC.",
        category: "Bitcoin",
        resolution_source: "Coinbase spot price",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will Bitcoin dominance drop below 50% in 2026?",
        description: "Resolves YES if Bitcoin market cap dominance (per CoinGecko) falls below 50% at any point during 2026.",
        category: "Bitcoin",
        resolution_source: "CoinGecko dominance chart",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will a Bitcoin spot ETF surpass $100B AUM?",
        description: "Resolves YES if any single US-listed Bitcoin spot ETF reports assets under management exceeding $100 billion.",
        category: "Bitcoin",
        resolution_source: "Bloomberg ETF data",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will the Bitcoin halving cycle peak occur before Q3 2026?",
        description: "Resolves YES if BTC all-time high for this halving cycle is set before July 1, 2026.",
        category: "Bitcoin",
        resolution_source: "CoinGecko ATH tracker",
        cpt_sats: 5000,
        settlement_unix: 1_719_792_000, // 2026-07-01
    },
    // Politics
    MarketDef {
        question: "Will the US pass stablecoin legislation in 2026?",
        description: "Resolves YES if a federal stablecoin bill is signed into law by the US President before January 1, 2027.",
        category: "Politics",
        resolution_source: "Congress.gov",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will the EU implement MiCA enforcement actions in 2026?",
        description: "Resolves YES if any EU member state issues formal enforcement action under MiCA regulation during 2026.",
        category: "Politics",
        resolution_source: "ESMA press releases",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will a G7 country adopt a Bitcoin strategic reserve?",
        description: "Resolves YES if any G7 nation officially announces a government-held Bitcoin reserve by end of 2026.",
        category: "Politics",
        resolution_source: "Government press releases",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will El Salvador repay its 2027 bonds early?",
        description: "Resolves YES if El Salvador repays or retires its January 2027 sovereign bonds before their maturity date.",
        category: "Politics",
        resolution_source: "Bloomberg bond data",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    // Sports
    MarketDef {
        question: "Will Real Madrid win the 2025-26 Champions League?",
        description: "Resolves YES if Real Madrid CF wins the UEFA Champions League final in the 2025-26 season.",
        category: "Sports",
        resolution_source: "UEFA official results",
        cpt_sats: 5000,
        settlement_unix: 1_717_200_000, // ~Jun 2026
    },
    MarketDef {
        question: "Will the Kansas City Chiefs three-peat at Super Bowl LXI?",
        description: "Resolves YES if the Kansas City Chiefs win Super Bowl LXI (February 2027).",
        category: "Sports",
        resolution_source: "NFL official results",
        cpt_sats: 5000,
        settlement_unix: 1_739_232_000, // ~Feb 2027
    },
    MarketDef {
        question: "Will a new 100m world record be set in 2026?",
        description: "Resolves YES if a new men's 100m sprint world record (sub-9.58s) is ratified by World Athletics during 2026.",
        category: "Sports",
        resolution_source: "World Athletics records page",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will Lewis Hamilton win a race for Ferrari in 2026?",
        description: "Resolves YES if Lewis Hamilton wins at least one Formula 1 Grand Prix while driving for Scuderia Ferrari in 2026.",
        category: "Sports",
        resolution_source: "FIA official race results",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    // Culture
    MarketDef {
        question: "Will a Nostr client reach 10M monthly active users?",
        description: "Resolves YES if any Nostr-based client reports or is independently verified to have 10 million monthly active users.",
        category: "Culture",
        resolution_source: "App store analytics / Nostr relay stats",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will an AI model pass the Turing test in 2026?",
        description: "Resolves YES if a peer-reviewed study published in 2026 demonstrates an AI system passing a standard Turing test with >70% of judges unable to distinguish it from a human.",
        category: "Culture",
        resolution_source: "Peer-reviewed publication",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will a major studio release a fully AI-generated film?",
        description: "Resolves YES if a major Hollywood studio releases a theatrical film where the primary creative content (script, visuals, audio) is AI-generated.",
        category: "Culture",
        resolution_source: "Studio press release / industry reporting",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will Wikipedia adopt Nostr for contributor identity?",
        description: "Resolves YES if the Wikimedia Foundation officially integrates Nostr-based identity for editor accounts.",
        category: "Culture",
        resolution_source: "Wikimedia Foundation announcements",
        cpt_sats: 5000,
        settlement_unix: JUN_2027_UNIX,
    },
    // Weather
    MarketDef {
        question: "Will 2026 be the hottest year on record?",
        description: "Resolves YES if NASA GISS or NOAA declares 2026 as the warmest year in their instrumental record.",
        category: "Weather",
        resolution_source: "NASA GISS / NOAA annual report",
        cpt_sats: 5000,
        settlement_unix: 1_801_353_600, // 2027-02-01
    },
    MarketDef {
        question: "Will a Category 6 hurricane classification be adopted?",
        description: "Resolves YES if NOAA or WMO officially adopts a Category 6 designation for the Saffir-Simpson Hurricane Wind Scale.",
        category: "Weather",
        resolution_source: "NOAA / WMO announcements",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    // Macro
    MarketDef {
        question: "Will the Fed cut rates below 3% by end of 2026?",
        description: "Resolves YES if the Federal Reserve's target federal funds rate upper bound is below 3.00% on December 31, 2026.",
        category: "Macro",
        resolution_source: "Federal Reserve press releases",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will gold exceed $3,500/oz in 2026?",
        description: "Resolves YES if the LBMA PM gold fix exceeds $3,500 per troy ounce at any point during 2026.",
        category: "Macro",
        resolution_source: "LBMA gold price",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will US national debt exceed $40 trillion?",
        description: "Resolves YES if the US total public debt outstanding exceeds $40 trillion as reported by the Treasury.",
        category: "Macro",
        resolution_source: "US Treasury Debt to the Penny",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
    MarketDef {
        question: "Will Ethereum flip Bitcoin in market cap?",
        description: "Resolves YES if Ethereum's market capitalization exceeds Bitcoin's at any point during 2026 per CoinGecko.",
        category: "Bitcoin",
        resolution_source: "CoinGecko market cap",
        cpt_sats: 5000,
        settlement_unix: DEC_2026_UNIX,
    },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_keys() -> Keys {
    let nsec = std::env::var("DEADCAT_SOURCE_NSEC")
        .expect("DEADCAT_SOURCE_NSEC env var required (nsec1...)");
    let secret = SecretKey::from_bech32(nsec.trim()).expect("invalid DEADCAT_SOURCE_NSEC");
    Keys::new(secret)
}

fn oracle_pubkey_bytes(keys: &Keys) -> [u8; 32] {
    let hex_str = keys.public_key().to_hex();
    let bytes = hex::decode(&hex_str).expect("pubkey hex decode");
    bytes.try_into().expect("pubkey must be 32 bytes")
}

fn default_tx_options() -> TxOptions {
    TxOptions {
        fee_policy: MinerFeePolicy::RateSatPerVb { sat_per_vb: 0.5 },
    }
}

fn electrum_url() -> String {
    std::env::var("DEADCAT_ELECTRUM_URL")
        .unwrap_or_else(|_| "ssl://blockstream.info:465".to_string())
}

fn datadir() -> std::path::PathBuf {
    std::env::var("DEADCAT_DATADIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/deadcat-seed"))
}

/// Convert a settlement Unix timestamp to a block height estimate.
/// Liquid produces a block roughly every minute.
fn unix_to_block_height(settlement_unix: u64, current_height: u32, current_unix: u64) -> u32 {
    if settlement_unix <= current_unix {
        return current_height + 1;
    }
    let seconds_until = settlement_unix - current_unix;
    let blocks_until = (seconds_until / 60) as u32;
    current_height + blocks_until
}

async fn setup_node(keys: Keys) -> Arc<DeadcatNode<NoopStore>> {
    let config = DiscoveryConfig {
        relays: DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
        network_tag: NETWORK_TAG.to_string(),
        source_author: Some(keys.public_key()),
        ..Default::default()
    };

    let (node, _rx) = DeadcatNode::new(keys, Network::LiquidTestnet, config);
    let node = Arc::new(node);

    // Unlock wallet
    let mnemonic = std::env::var("DEADCAT_MNEMONIC")
        .expect("DEADCAT_MNEMONIC env var required (12-word BIP-39 phrase)");
    let electrum = electrum_url();
    let dir = datadir();

    println!("Unlocking wallet (electrum: {electrum})...");
    let node_clone = node.clone();
    let handle = tokio::task::spawn_blocking(move || {
        node_clone
            .unlock_wallet(&mnemonic, &electrum, &dir)
            .expect("failed to unlock wallet");
    });
    handle.await.expect("wallet unlock task failed");

    // Show balance
    let balance = node.balance().unwrap_or_default();
    let policy_asset = Network::LiquidTestnet.into_lwk().policy_asset();
    let lbtc_sats = balance.get(&policy_asset).copied().unwrap_or(0);
    println!("Wallet balance: {} sats ({:.8} tL-BTC)", lbtc_sats, lbtc_sats as f64 / 100_000_000.0);

    // Check if we have enough UTXOs — market creation needs at least 2
    let utxo_count = node
        .utxos()
        .unwrap_or_default()
        .iter()
        .filter(|u| u.unblinded.asset == policy_asset)
        .count();

    if utxo_count < 2 && lbtc_sats > 2000 {
        println!("Only {} L-BTC UTXO(s) — splitting for market creation...", utxo_count);
        let self_addr = node.address(None).await.expect("address").address().to_string();
        let split_amount = lbtc_sats / 2;
        match node.send_lbtc(self_addr, split_amount, default_tx_options()).await {
            Ok((txid, fee)) => {
                println!("Split tx: {} (fee: {} sats)", txid, fee);
                println!("Waiting for confirmation...");
                tokio::time::sleep(Duration::from_secs(5)).await;
                if let Err(e) = node.sync_wallet().await {
                    eprintln!("Warning: sync after split failed: {e}");
                }
                let balance = node.balance().unwrap_or_default();
                let lbtc_sats = balance.get(&policy_asset).copied().unwrap_or(0);
                println!("Balance after split: {} sats", lbtc_sats);
            }
            Err(e) => {
                eprintln!("Warning: failed to split UTXOs: {e}");
                eprintln!("Market creation may fail. Fund wallet with more UTXOs.");
            }
        }
    }
    println!();

    // Start Nostr subscription
    if let Err(e) = node.start_subscription().await {
        eprintln!("Warning: failed to start Nostr subscription: {e}");
    }

    node
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

async fn cmd_publish(keys: &Keys, count: Option<usize>) {
    let node = setup_node(keys.clone()).await;
    let oracle_pk = oracle_pubkey_bytes(keys);
    let max = count.unwrap_or(MARKETS.len()).min(MARKETS.len());

    // Get current chain tip for block height estimation
    let current_height = 2_800_000u32; // approximate Liquid testnet height
    let current_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    println!("Creating {} market(s)...\n", max);

    for (i, market) in MARKETS.iter().take(max).enumerate() {
        let expiry_height = unix_to_block_height(market.settlement_unix, current_height, current_unix);
        let metadata = ContractMetadata {
            question: market.question.to_string(),
            description: market.description.to_string(),
            category: market.category.to_string(),
            resolution_source: market.resolution_source.to_string(),
        };

        println!("[{}/{}] Creating: {}", i + 1, max, market.question);

        // Prepare market creation
        let prepared = match node
            .prepare_create_market(
                oracle_pk,
                market.cpt_sats,
                expiry_height,
                546, // min UTXO value (dust limit)
                default_tx_options(),
                metadata,
            )
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("  FAILED to prepare: {e}");
                continue;
            }
        };

        // Broadcast + announce on Nostr
        let result = match node.broadcast_prepared_market_creation(prepared).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("  FAILED to broadcast: {e}");
                continue;
            }
        };

        println!("  Market ID:    {}", result.market.market_id);
        println!("  Creation TX:  {}", result.market.anchor.creation_txid);
        println!("  Event ID:     {}", result.market.id);
        println!("  Fee:          {} sats", result.fee.amount_sat);
        println!("  Category:     {}", market.category);
        println!();

        // Sync wallet between markets so UTXOs are up to date
        if i + 1 < max {
            println!("  Syncing wallet...");
            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Err(e) = node.sync_wallet().await {
                eprintln!("  Warning: wallet sync failed: {e}");
            }
        }
    }

    // Final balance
    let balance = node.balance().unwrap_or_default();
    let policy_asset = Network::LiquidTestnet.into_lwk().policy_asset();
    let lbtc_sats = balance.get(&policy_asset).copied().unwrap_or(0);
    println!("Remaining balance: {} sats\n", lbtc_sats);
    println!("Done.");
}

async fn cmd_list(keys: &Keys) {
    let client = {
        let c = Client::default();
        for url in DEFAULT_RELAYS {
            let _ = c.add_relay(*url).await;
        }
        c.connect_with_timeout(Duration::from_secs(5)).await;
        c
    };

    let pubkey = keys.public_key();
    let filter = Filter::new()
        .kind(Kind::Custom(30078))
        .hashtag("deadcat-contract")
        .author(pubkey);

    println!(
        "Fetching markets from {} ...\n",
        pubkey.to_bech32().unwrap_or_default()
    );

    match client
        .fetch_events(vec![filter], Duration::from_secs(10))
        .await
    {
        Ok(events) => {
            if events.is_empty() {
                println!("No markets found.");
                return;
            }
            for (i, event) in events.iter().enumerate() {
                let d_tag = event
                    .tags
                    .iter()
                    .find_map(|t| {
                        let s = t.as_slice();
                        if s.len() >= 2 && s[0] == "d" {
                            Some(s[1].clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();

                let question = serde_json::from_str::<serde_json::Value>(&event.content)
                    .ok()
                    .and_then(|v| {
                        v.get("metadata")
                            .and_then(|m| m.get("question"))
                            .and_then(|q| q.as_str())
                            .map(String::from)
                    })
                    .unwrap_or_else(|| "(unparseable)".to_string());

                println!(
                    "[{:2}] {} | d={}... | {}",
                    i + 1,
                    &event.id.to_hex()[..12],
                    &d_tag[..16.min(d_tag.len())],
                    question,
                );
            }
            println!("\nTotal: {} markets", events.iter().count());
        }
        Err(e) => eprintln!("Failed to fetch: {e}"),
    }
}

async fn cmd_delete(keys: &Keys) {
    let client = {
        let c = Client::default();
        for url in DEFAULT_RELAYS {
            let _ = c.add_relay(*url).await;
        }
        c.connect_with_timeout(Duration::from_secs(5)).await;
        c
    };

    let pubkey = keys.public_key();
    let filter = Filter::new()
        .kind(Kind::Custom(30078))
        .hashtag("deadcat-contract")
        .author(pubkey);

    println!("Fetching existing markets to delete...\n");

    let events = match client
        .fetch_events(vec![filter], Duration::from_secs(10))
        .await
    {
        Ok(events) => events,
        Err(e) => {
            eprintln!("Failed to fetch: {e}");
            return;
        }
    };

    if events.is_empty() {
        println!("No markets found.");
        return;
    }

    let count = events.iter().count();
    println!(
        "Found {} markets. Publishing tombstone replacements...\n",
        count
    );

    for event in events.iter() {
        let d_tag = event
            .tags
            .iter()
            .find_map(|t| {
                let s = t.as_slice();
                if s.len() >= 2 && s[0] == "d" {
                    Some(s[1].clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let tombstone = EventBuilder::new(Kind::Custom(30078), "")
            .tags(vec![
                Tag::identifier(&d_tag),
                Tag::custom(TagKind::custom("deleted"), vec!["true".to_string()]),
            ])
            .sign_with_keys(keys)
            .expect("failed to sign tombstone");

        match client.send_event(tombstone).await {
            Ok(_) => println!(
                "  Deleted: d={}...",
                &d_tag[..16.min(d_tag.len())]
            ),
            Err(e) => eprintln!(
                "  FAILED to delete d={}...: {e}",
                &d_tag[..16.min(d_tag.len())]
            ),
        }
    }
    println!("\nDeleted {} markets.", count);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    install_crypto_provider();
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    if command == "help" {
        print_usage();
        return;
    }

    let keys = load_keys();
    println!(
        "Source: {}\n",
        keys.public_key().to_bech32().unwrap_or_default()
    );

    match command {
        "publish" => {
            let count = args.get(2).and_then(|s| s.parse::<usize>().ok());
            cmd_publish(&keys, count).await;
        }
        "list" => cmd_list(&keys).await,
        "delete" => cmd_delete(&keys).await,
        _ => print_usage(),
    }
}

fn print_usage() {
    eprintln!("Usage: seed_markets <command> [args]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  publish [N]  Create N markets on-chain + announce on Nostr (default: all {}))", MARKETS.len());
    eprintln!("  list         List markets published by the source npub");
    eprintln!("  delete       Delete all markets by publishing tombstone events");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  DEADCAT_SOURCE_NSEC   nsec for the source npub (required)");
    eprintln!("  DEADCAT_MNEMONIC      BIP-39 mnemonic for funded testnet wallet (required for publish)");
    eprintln!("  DEADCAT_ELECTRUM_URL  Electrum server (default: ssl://blockstream.info:465)");
    eprintln!("  DEADCAT_DATADIR       Wallet data dir (default: /tmp/deadcat-seed)");
}
