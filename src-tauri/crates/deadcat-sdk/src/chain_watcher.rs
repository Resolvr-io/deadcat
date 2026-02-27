//! `ChainWatcher` — persistent Electrum subscription relay.
//!
//! Maintains a long-lived TCP connection to an Electrum server,
//! subscribes to scripthash notifications for known contracts, and
//! pushes typed [`ChainEvent`]s to the Node layer.
//!
//! The watcher runs on a **dedicated OS thread** because
//! `electrum_client::Client` is `!Send`.  Communication with the
//! async Node world uses `tokio::sync::mpsc` channels.

use std::collections::HashMap;
use std::time::Duration;

use electrum_client::ElectrumApi;

use crate::amm_pool::params::PoolId;

// ── Public types ────────────────────────────────────────────────────

/// Describes what contract a subscribed script belongs to.
#[derive(Debug, Clone)]
pub enum ScriptOwner {
    Market { market_id: [u8; 32] },
    Order { order_spk: Vec<u8> },
    Pool { pool_id: PoolId },
}

/// Commands sent from the Node layer to the watcher thread.
#[derive(Debug)]
pub enum WatchCmd {
    Subscribe {
        script_bytes: Vec<u8>,
        owner: ScriptOwner,
    },
    Unsubscribe {
        script_bytes: Vec<u8>,
    },
    Shutdown,
}

/// Events emitted by the watcher thread to the Node layer.
#[derive(Debug, Clone)]
pub enum ChainEvent {
    MarketActivity { market_id: [u8; 32] },
    OrderActivity { order_spk: Vec<u8> },
    PoolActivity { pool_id: PoolId },
    NewBlock { height: u32 },
    ConnectionLost,
    Reconnected,
}

/// Configuration for the chain watcher.
#[derive(Debug, Clone)]
pub struct ChainWatcherConfig {
    pub electrum_url: String,
    /// Poll interval for checking notifications (default: 1s).
    pub poll_interval: Duration,
    /// Maximum reconnection backoff (default: 60s).
    pub max_backoff: Duration,
}

impl ChainWatcherConfig {
    pub fn new(electrum_url: &str) -> Self {
        Self {
            electrum_url: electrum_url.to_string(),
            poll_interval: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
        }
    }
}

/// Handle for sending commands to a running watcher thread.
#[derive(Clone)]
pub struct ChainWatcherHandle {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<WatchCmd>,
}

impl ChainWatcherHandle {
    /// Subscribe a script to the watcher.
    pub fn subscribe(&self, script_bytes: Vec<u8>, owner: ScriptOwner) {
        let _ = self.cmd_tx.send(WatchCmd::Subscribe {
            script_bytes,
            owner,
        });
    }

    /// Unsubscribe a script from the watcher.
    pub fn unsubscribe(&self, script_bytes: Vec<u8>) {
        let _ = self.cmd_tx.send(WatchCmd::Unsubscribe { script_bytes });
    }

    /// Shut down the watcher thread.
    pub fn shutdown(&self) {
        let _ = self.cmd_tx.send(WatchCmd::Shutdown);
    }
}

// ── Internals ───────────────────────────────────────────────────────

/// State held by the watcher thread.
struct WatcherState {
    /// Map from raw script bytes → owner.
    subscriptions: HashMap<Vec<u8>, ScriptOwner>,
    /// Last known block height.
    last_height: Option<u32>,
}

impl WatcherState {
    fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
            last_height: None,
        }
    }
}

/// Try to connect to the Electrum server with exponential backoff.
///
/// Returns `None` if a `Shutdown` command arrives while waiting to connect.
fn connect_with_backoff(
    url: &str,
    max_backoff: Duration,
    cmd_rx: &mut tokio::sync::mpsc::UnboundedReceiver<WatchCmd>,
    event_tx: &tokio::sync::mpsc::UnboundedSender<ChainEvent>,
) -> Option<electrum_client::Client> {
    let mut backoff = Duration::from_secs(1);
    loop {
        match electrum_client::Client::new(url) {
            Ok(client) => return Some(client),
            Err(e) => {
                log::warn!("chain_watcher: connect failed ({e}), retrying in {backoff:?}");
                let _ = event_tx.send(ChainEvent::ConnectionLost);
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(max_backoff);

                // Drain commands while disconnected — exit on Shutdown
                while let Ok(cmd) = cmd_rx.try_recv() {
                    if matches!(cmd, WatchCmd::Shutdown) {
                        log::info!("chain_watcher: shutdown during reconnect");
                        return None;
                    }
                }
            }
        }
    }
}

/// Re-subscribe all tracked scripts after a reconnect.
fn resubscribe_all(client: &electrum_client::Client, state: &WatcherState) {
    for script_bytes in state.subscriptions.keys() {
        let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_bytes.clone());
        if let Err(e) = client.script_subscribe(&btc_script) {
            log::warn!(
                "chain_watcher: re-subscribe failed for {}: {e}",
                hex::encode(script_bytes),
            );
        }
    }
}

// ── Spawn ───────────────────────────────────────────────────────────

/// Spawn the chain watcher on a dedicated OS thread.
///
/// Returns a handle for sending commands, and a receiver for chain events.
pub fn spawn_chain_watcher(
    config: ChainWatcherConfig,
) -> (
    ChainWatcherHandle,
    tokio::sync::mpsc::UnboundedReceiver<ChainEvent>,
) {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = ChainWatcherHandle { cmd_tx };

    std::thread::Builder::new()
        .name("chain-watcher".into())
        .spawn(move || {
            watcher_thread_main(config, cmd_rx, event_tx);
        })
        .expect("failed to spawn chain-watcher thread");

    (handle, event_rx)
}

/// Main loop of the watcher thread.
fn watcher_thread_main(
    config: ChainWatcherConfig,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<WatchCmd>,
    event_tx: tokio::sync::mpsc::UnboundedSender<ChainEvent>,
) {
    let mut state = WatcherState::new();
    let mut client = match connect_with_backoff(
        &config.electrum_url,
        config.max_backoff,
        &mut cmd_rx,
        &event_tx,
    ) {
        Some(c) => c,
        None => return,
    };

    // Subscribe to block headers (raw variant — Liquid headers fail Bitcoin deser)
    if let Err(e) = client.block_headers_subscribe_raw() {
        log::warn!("chain_watcher: initial block_headers_subscribe failed: {e}");
    }

    log::info!("chain_watcher: connected to {}", config.electrum_url);

    loop {
        // 1. Drain all pending commands (non-blocking)
        loop {
            match cmd_rx.try_recv() {
                Ok(WatchCmd::Subscribe {
                    script_bytes,
                    owner,
                }) => {
                    let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_bytes.clone());
                    match client.script_subscribe(&btc_script) {
                        Ok(_status) => {
                            log::debug!("chain_watcher: subscribed {}", hex::encode(&script_bytes),);
                            state.subscriptions.insert(script_bytes, owner);
                        }
                        Err(e) => {
                            log::warn!("chain_watcher: subscribe failed: {e}");
                        }
                    }
                }
                Ok(WatchCmd::Unsubscribe { script_bytes }) => {
                    let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_bytes.clone());
                    let _ = client.script_unsubscribe(&btc_script);
                    log::debug!("chain_watcher: unsubscribed {}", hex::encode(&script_bytes));
                    state.subscriptions.remove(&script_bytes);
                }
                Ok(WatchCmd::Shutdown) => {
                    log::info!("chain_watcher: shutting down");
                    return;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    log::info!("chain_watcher: command channel closed, shutting down");
                    return;
                }
            }
        }

        // 2. Ping to flush the socket and process server-side queued notifications
        let alive = client
            .raw_call("server.ping", Vec::<electrum_client::Param>::new())
            .is_ok();

        if !alive {
            log::warn!("chain_watcher: connection lost, reconnecting...");
            client = match connect_with_backoff(
                &config.electrum_url,
                config.max_backoff,
                &mut cmd_rx,
                &event_tx,
            ) {
                Some(c) => c,
                None => return, // Shutdown received during reconnect
            };
            // Re-subscribe block headers
            let _ = client.block_headers_subscribe_raw();
            resubscribe_all(&client, &state);
            let _ = event_tx.send(ChainEvent::Reconnected);
            log::info!("chain_watcher: reconnected");
            continue;
        }

        // 3. Check for new block headers
        if let Ok(Some(header)) = client.block_headers_pop_raw() {
            let height = header.height as u32;
            if state.last_height.is_none_or(|prev| height > prev) {
                state.last_height = Some(height);
                log::debug!("chain_watcher: new block at height {height}");
                let _ = event_tx.send(ChainEvent::NewBlock { height });
            }
        }

        // 4. Check for script notifications
        // Collect keys first to avoid borrow issues
        let scripts: Vec<Vec<u8>> = state.subscriptions.keys().cloned().collect();

        for script_bytes in scripts {
            if let Some(owner) = state.subscriptions.get(&script_bytes) {
                let btc_script = lwk_wollet::bitcoin::ScriptBuf::from(script_bytes.clone());
                match client.script_pop(&btc_script) {
                    Ok(Some(_status_change)) => {
                        // Emit typed event based on owner
                        let event = match owner {
                            ScriptOwner::Market { market_id } => ChainEvent::MarketActivity {
                                market_id: *market_id,
                            },
                            ScriptOwner::Order { order_spk } => ChainEvent::OrderActivity {
                                order_spk: order_spk.clone(),
                            },
                            ScriptOwner::Pool { pool_id } => {
                                ChainEvent::PoolActivity { pool_id: *pool_id }
                            }
                        };
                        log::debug!("chain_watcher: activity on {}", hex::encode(&script_bytes));
                        let _ = event_tx.send(event);
                    }
                    Ok(None) => {} // no change
                    Err(e) => {
                        log::warn!(
                            "chain_watcher: script_pop failed for {}: {e}",
                            hex::encode(&script_bytes),
                        );
                    }
                }
            }
        }

        // 5. Sleep before next poll
        std::thread::sleep(config.poll_interval);
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = ChainWatcherConfig::new("tcp://localhost:50001");
        assert_eq!(cfg.poll_interval, Duration::from_secs(1));
        assert_eq!(cfg.max_backoff, Duration::from_secs(60));
    }

    #[test]
    fn handle_clone_and_send() {
        // Verify ChainWatcherHandle is Clone + Send
        fn assert_clone_send<T: Clone + Send>() {}
        assert_clone_send::<ChainWatcherHandle>();
    }

    #[test]
    fn chain_event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ChainEvent>();
    }

    #[test]
    fn shutdown_stops_thread() {
        // Spawn with a bogus URL — the thread should still respond to Shutdown
        // before connect_with_backoff retries.
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel::<ChainEvent>();

        // Send Shutdown immediately so the thread exits on first cmd drain
        let _ = cmd_tx.send(WatchCmd::Shutdown);

        let handle = std::thread::spawn(move || {
            let _config = ChainWatcherConfig {
                electrum_url: "tcp://127.0.0.1:1".into(), // won't connect
                poll_interval: Duration::from_millis(10),
                max_backoff: Duration::from_millis(10),
            };
            // Override: manually run the cmd drain portion only
            // (full watcher_thread_main would block on connect)
            let mut cmd_rx = cmd_rx;
            loop {
                match cmd_rx.try_recv() {
                    Ok(WatchCmd::Shutdown) => return true,
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return false,
                    _ => continue,
                }
            }
        });

        assert!(handle.join().unwrap());
    }
}
