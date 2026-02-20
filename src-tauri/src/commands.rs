use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use lwk_wollet::elements::AssetId;
use lwk_wollet::WalletTxOut;
use nostr_sdk::prelude::*;
use rand::thread_rng;
use tauri::{Emitter, Manager};

use crate::discovery::{
    self, AttestationResult, ContractAnnouncement, ContractMetadata, CreateContractRequest,
    DiscoveredMarket, IdentityResponse,
};
use crate::state::AppStateManager;
use crate::SdkState;

fn validate_request(request: &CreateContractRequest) -> Result<(), String> {
    if request.question.trim().is_empty() || request.question.len() > 140 {
        return Err("question must be 1-140 characters".to_string());
    }
    if request.description.trim().is_empty() || request.description.len() > 280 {
        return Err("description must be 1-280 characters".to_string());
    }
    if request.resolution_source.trim().is_empty() || request.resolution_source.len() > 120 {
        return Err("resolution_source must be 1-120 characters".to_string());
    }
    if request.starting_yes_price < 1 || request.starting_yes_price > 99 {
        return Err("starting_yes_price must be 1-99".to_string());
    }
    if request.collateral_per_token == 0 {
        return Err("collateral_per_token must be > 0".to_string());
    }
    Ok(())
}

async fn compute_tip_and_now(
    network: crate::WalletNetwork,
) -> Result<(crate::ChainTipResponse, u64), String> {
    let tip = crate::fetch_chain_tip_inner(network).await?;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("time error: {e}"))?
        .as_secs();
    Ok((tip, now_unix))
}

async fn get_or_connect_nostr_client(sdk_state: &SdkState) -> Result<nostr_sdk::Client, String> {
    let mut nostr_client = sdk_state.nostr_client.lock().await;
    if nostr_client.is_none() {
        let c = discovery::connect_client(None).await?;
        *nostr_client = Some(c);
    }
    Ok(nostr_client.as_ref().unwrap().clone())
}

#[tauri::command]
pub async fn init_nostr_identity(
    state: tauri::State<'_, SdkState>,
    app_handle: tauri::AppHandle,
) -> Result<IdentityResponse, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;

    let keys = discovery::load_or_generate_keys(&app_data_dir)?;

    let response = IdentityResponse {
        pubkey_hex: keys.public_key().to_hex(),
        npub: keys.public_key().to_bech32().map_err(|e| format!("bech32 error: {e}"))?,
    };

    {
        let mut nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        *nostr_keys = Some(keys);
    }

    Ok(response)
}

#[tauri::command]
pub fn get_nostr_identity(
    state: tauri::State<'_, SdkState>,
) -> Result<Option<IdentityResponse>, String> {
    let nostr_keys = state
        .nostr_keys
        .lock()
        .map_err(|_| "failed to lock nostr_keys".to_string())?;

    match nostr_keys.as_ref() {
        Some(keys) => Ok(Some(IdentityResponse {
            pubkey_hex: keys.public_key().to_hex(),
            npub: keys.public_key().to_bech32().map_err(|e| format!("bech32 error: {e}"))?,
        })),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn discover_contracts(
    state: tauri::State<'_, SdkState>,
) -> Result<Vec<DiscoveredMarket>, String> {
    let client = get_or_connect_nostr_client(&state).await?;
    discovery::fetch_announcements(&client).await
}

/// Publish a contract to Nostr (Nostr-only mode — no on-chain tx).
///
/// This creates a `ContractAnnouncement` with placeholder asset IDs derived from
/// the oracle pubkey and metadata, then publishes it to Nostr relays. The
/// `creation_txid` will be `None` since no Liquid transaction is created.
///
/// For full on-chain creation, the wallet must be funded and synced. That flow
/// (selecting UTXOs, computing issuance assets, building PSET, signing, broadcasting)
/// will be wired when the LWK wallet integration is complete.
#[tauri::command]
pub async fn publish_contract(
    state: tauri::State<'_, SdkState>,
    request: CreateContractRequest,
    app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    validate_request(&request)?;

    let keys = {
        let nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        nostr_keys
            .clone()
            .ok_or_else(|| "nostr identity not initialized — call init_nostr_identity first".to_string())?
    };

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = keys.public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode error: {e}"))?;
        bytes.try_into().map_err(|_| "pubkey must be 32 bytes".to_string())?
    };

    // Estimate expiry block height from settlement deadline.
    // On Liquid, blocks are ~1 min. We fetch chain tip to compute relative offset.
    let wallet_network: crate::WalletNetwork = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        mgr.network()
            .ok_or_else(|| "network not configured".to_string())?
            .into()
    };
    let (tip, now_unix) = compute_tip_and_now(wallet_network).await?;

    let expiry_time = if request.settlement_deadline_unix > now_unix {
        let seconds_until = request.settlement_deadline_unix - now_unix;
        let blocks_until = (seconds_until / 60) as u32; // ~1 min per block on Liquid
        tip.height + blocks_until
    } else {
        return Err("settlement deadline must be in the future".to_string());
    };

    // Build ContractParams with placeholder asset IDs.
    // Real asset IDs require selecting UTXOs and computing issuance assets.
    // For Nostr-only publishing, we use zeros — the announcement can be updated
    // after on-chain creation via the parameterized-replaceable event.
    let contract_params = deadcat_sdk::params::ContractParams {
        oracle_public_key: oracle_pubkey_bytes,
        collateral_asset_id: [0u8; 32], // Placeholder — set after UTXO selection
        yes_token_asset: [0u8; 32],
        no_token_asset: [0u8; 32],
        yes_reissuance_token: [0u8; 32],
        no_reissuance_token: [0u8; 32],
        collateral_per_token: request.collateral_per_token,
        expiry_time,
    };

    let metadata = ContractMetadata {
        question: request.question,
        description: request.description,
        category: request.category,
        resolution_source: request.resolution_source,
        starting_yes_price: request.starting_yes_price,
    };

    let announcement = ContractAnnouncement {
        version: 1,
        contract_params,
        metadata,
        creation_txid: None,
    };

    // Build and publish the Nostr event
    let event = discovery::build_announcement_event(&keys, &announcement)?;

    let client = get_or_connect_nostr_client(&state).await?;
    let event_id = discovery::publish_event(&client, event.clone()).await?;

    // Return the discovered market representation
    let market = discovery::parse_announcement_event(&event)?;

    Ok(DiscoveredMarket {
        id: event_id.to_hex(),
        ..market
    })
}

#[tauri::command]
pub async fn oracle_attest(
    state: tauri::State<'_, SdkState>,
    market_id_hex: String,
    outcome_yes: bool,
) -> Result<AttestationResult, String> {
    let keys = {
        let nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        nostr_keys
            .clone()
            .ok_or_else(|| "nostr identity not initialized".to_string())?
    };

    // Parse market_id from hex
    let market_id_bytes: [u8; 32] = hex::decode(&market_id_hex)
        .map_err(|e| format!("invalid market_id hex: {e}"))?
        .try_into()
        .map_err(|_| "market_id must be exactly 32 bytes".to_string())?;

    let market_id = deadcat_sdk::params::MarketId(market_id_bytes);

    // Sign the attestation
    let (sig_bytes, msg_bytes) = discovery::sign_attestation(&keys, &market_id, outcome_yes)?;

    // Find the announcement event ID for reference.
    // We search the relay for the announcement to link the attestation.
    let client = get_or_connect_nostr_client(&state).await?;

    // Look up the announcement event by d-tag (market_id_hex)
    let filter = nostr_sdk::Filter::new()
        .kind(discovery::CONTRACT_EVENT_KIND)
        .identifier(&market_id_hex)
        .hashtag(discovery::CONTRACT_TAG);

    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch announcement: {e}"))?;

    let announcement_event_id = events
        .iter()
        .next()
        .map(|e| e.id.to_hex())
        .unwrap_or_default();

    // Build and publish attestation event
    let sig_hex = hex::encode(sig_bytes);
    let msg_hex = hex::encode(msg_bytes);

    let event = discovery::build_attestation_event(
        &keys,
        &market_id_hex,
        &announcement_event_id,
        outcome_yes,
        &sig_hex,
        &msg_hex,
    )?;

    let event_id = discovery::publish_event(&client, event).await?;

    Ok(AttestationResult {
        market_id: market_id_hex,
        outcome_yes,
        signature_hex: sig_hex,
        nostr_event_id: event_id.to_hex(),
    })
}

// ---------------------------------------------------------------------------
// On-chain contract creation helpers
// ---------------------------------------------------------------------------

/// Convert a LWK `WalletTxOut` + full `TxOut` into the SDK's `UnblindedUtxo`.
///
/// Both `lwk_wollet::elements` and `deadcat_sdk::elements` resolve to the same
/// `elements 0.25.2` crate instance, so the types are directly compatible.
/// We use `elements::encode` serialization as a bridge for safety.
fn wallet_utxo_to_sdk_unblinded(
    utxo: &WalletTxOut,
    txout: &lwk_wollet::elements::TxOut,
) -> Result<deadcat_sdk::UnblindedUtxo, String> {
    // Bridge OutPoint via consensus encoding
    let op_bytes = lwk_wollet::elements::encode::serialize(&utxo.outpoint);
    let sdk_outpoint: deadcat_sdk::elements::OutPoint =
        deadcat_sdk::elements::encode::deserialize(&op_bytes)
            .map_err(|e| format!("outpoint bridge: {e}"))?;

    // Bridge TxOut via consensus encoding
    let txout_bytes = lwk_wollet::elements::encode::serialize(txout);
    let sdk_txout: deadcat_sdk::elements::TxOut =
        deadcat_sdk::elements::encode::deserialize(&txout_bytes)
            .map_err(|e| format!("txout bridge: {e}"))?;

    // Extract raw bytes from blinding factors
    let asset_bytes: [u8; 32] = utxo.unblinded.asset.into_inner().to_byte_array();

    let mut abf = [0u8; 32];
    abf.copy_from_slice(utxo.unblinded.asset_bf.into_inner().as_ref());

    let mut vbf = [0u8; 32];
    vbf.copy_from_slice(utxo.unblinded.value_bf.into_inner().as_ref());

    Ok(deadcat_sdk::UnblindedUtxo {
        outpoint: sdk_outpoint,
        txout: sdk_txout,
        asset_id: asset_bytes,
        value: utxo.unblinded.value,
        asset_blinding_factor: abf,
        value_blinding_factor: vbf,
    })
}

/// Select 2 unspent L-BTC UTXOs suitable as defining outpoints.
///
/// Returns the two largest qualifying UTXOs whose combined value covers
/// at least the minimum needed for the creation fee.
fn select_defining_utxos(
    raw_utxos: &[WalletTxOut],
    policy_asset: AssetId,
    min_value_per_utxo: u64,
) -> Result<(WalletTxOut, WalletTxOut), String> {
    let mut candidates: Vec<_> = raw_utxos
        .iter()
        .filter(|u| {
            !u.is_spent
                && u.unblinded.asset == policy_asset
                && u.unblinded.value >= min_value_per_utxo
        })
        .cloned()
        .collect();

    // Sort by value descending so we pick the largest UTXOs
    candidates.sort_by(|a, b| b.unblinded.value.cmp(&a.unblinded.value));

    if candidates.len() < 2 {
        return Err(format!(
            "need at least 2 L-BTC UTXOs with >= {} sats each (found {}). \
             Fund the wallet and try again.",
            min_value_per_utxo,
            candidates.len()
        ));
    }

    Ok((candidates[0].clone(), candidates[1].clone()))
}

// ---------------------------------------------------------------------------
// On-chain contract creation command
// ---------------------------------------------------------------------------

/// Create a prediction market contract on-chain (Liquid creation tx + Nostr announcement).
///
/// This builds and broadcasts the Liquid creation transaction that issues
/// reissuance tokens into the Dormant covenant, then publishes a Nostr
/// announcement with the real asset IDs and creation txid.
#[tauri::command]
pub async fn create_contract_onchain(
    sdk_state: tauri::State<'_, SdkState>,
    request: CreateContractRequest,
    app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    // ── 1. Validate inputs ──────────────────────────────────────────────
    validate_request(&request)?;

    // ── 2. Get oracle pubkey from Nostr keys ────────────────────────────
    let keys = {
        let nostr_keys = sdk_state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        nostr_keys
            .clone()
            .ok_or_else(|| "nostr identity not initialized — call init_nostr_identity first".to_string())?
    };

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = keys.public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| "pubkey must be 32 bytes".to_string())?
    };

    // ── 3. Compute expiry block height ──────────────────────────────────
    let wallet_network: crate::WalletNetwork = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        mgr.network()
            .ok_or_else(|| "network not configured".to_string())?
            .into()
    };
    let (tip, now_unix) = compute_tip_and_now(wallet_network).await?;

    let expiry_time = if request.settlement_deadline_unix > now_unix {
        let seconds_until = request.settlement_deadline_unix - now_unix;
        let blocks_until = (seconds_until / 60) as u32;
        tip.height + blocks_until
    } else {
        return Err("settlement deadline must be in the future".into());
    };

    // ── 4-6. All blocking wallet I/O runs on a dedicated thread ─────────
    //
    // wallet.sync(), fetch_transaction(), sign_pset(), broadcast_and_sync()
    // are all blocking electrum calls. Running them on the async runtime
    // would starve the Tokio thread-pool, so we move them to spawn_blocking.
    let app_handle = app.clone();
    let collateral_per_token = request.collateral_per_token;

    let (creation_txid, contract_params) = tokio::task::spawn_blocking(move || {
        // Access the managed wallet state via AppHandle
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet_mut().ok_or_else(|| "wallet not initialized".to_string())?;

        // Ensure wallet is unlocked
        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to create a contract".to_string());
        }

        wallet.sync().map_err(|e| format!("sync: {e}"))?;

        let raw_utxos = wallet.raw_utxos().map_err(|e| format!("raw_utxos: {e}"))?;
        let policy_hex = wallet.policy_asset_id();
        let policy_asset: AssetId = policy_hex
            .parse()
            .map_err(|e| format!("bad policy asset: {e}"))?;
        let policy_bytes: [u8; 32] = policy_asset.into_inner().to_byte_array();

        let (yes_utxo, no_utxo) = select_defining_utxos(&raw_utxos, policy_asset, 300)?;

        // Fetch raw transactions to get full TxOut (with confidential data)
        let yes_tx = wallet
            .fetch_transaction(&yes_utxo.outpoint.txid)
            .map_err(|e| format!("fetch YES tx: {e}"))?;
        let no_tx = wallet
            .fetch_transaction(&no_utxo.outpoint.txid)
            .map_err(|e| format!("fetch NO tx: {e}"))?;

        let yes_txout = yes_tx
            .output
            .get(yes_utxo.outpoint.vout as usize)
            .ok_or_else(|| "YES UTXO vout out of range".to_string())?
            .clone();
        let no_txout = no_tx
            .output
            .get(no_utxo.outpoint.vout as usize)
            .ok_or_else(|| "NO UTXO vout out of range".to_string())?
            .clone();

        // Get a fresh change address
        let addr = wallet.address(None).map_err(|e| format!("address: {e}"))?;
        let change_addr: lwk_wollet::elements::Address = addr
            .address
            .parse()
            .map_err(|e| format!("bad change address: {e}"))?;
        let change_spk_bytes =
            lwk_wollet::elements::encode::serialize(&change_addr.script_pubkey());
        let change_spk: deadcat_sdk::elements::Script =
            deadcat_sdk::elements::encode::deserialize(&change_spk_bytes)
                .map_err(|e| format!("script bridge: {e}"))?;

        // ── Compile contract + build creation PSET ──────────────────────
        let yes_op_bytes = lwk_wollet::elements::encode::serialize(&yes_utxo.outpoint);
        let yes_outpoint: deadcat_sdk::elements::OutPoint =
            deadcat_sdk::elements::encode::deserialize(&yes_op_bytes)
                .map_err(|e| format!("yes outpoint bridge: {e}"))?;

        let no_op_bytes = lwk_wollet::elements::encode::serialize(&no_utxo.outpoint);
        let no_outpoint: deadcat_sdk::elements::OutPoint =
            deadcat_sdk::elements::encode::deserialize(&no_op_bytes)
                .map_err(|e| format!("no outpoint bridge: {e}"))?;

        let contract = deadcat_sdk::CompiledContract::create(
            oracle_pubkey_bytes,
            policy_bytes,
            collateral_per_token,
            expiry_time,
            yes_outpoint,
            no_outpoint,
        )
        .map_err(|e| format!("contract compilation: {e}"))?;

        let yes_unblinded = wallet_utxo_to_sdk_unblinded(&yes_utxo, &yes_txout)?;
        let no_unblinded = wallet_utxo_to_sdk_unblinded(&no_utxo, &no_txout)?;

        let mut sdk_pset = deadcat_sdk::build_creation_pset(
            &contract,
            &deadcat_sdk::CreationParams {
                yes_defining_utxo: yes_unblinded,
                no_defining_utxo: no_unblinded,
                fee_amount: 300,
                change_destination: Some(change_spk),
                lock_time: 0,
            },
        )
        .map_err(|e| format!("PSET construction: {e}"))?;

        // The SDK leaves reissuance-token outputs as confidential placeholders
        // (amount: None, asset: None). Fill in explicit values so the PSET
        // blinder knows the amounts, then blind the PSET so the final
        // transaction has proper Pedersen commitments + range/surjection proofs.
        {
            let cp = contract.params();
            let yes_rt = AssetId::from_slice(&cp.yes_reissuance_token)
                .map_err(|e| format!("bad YES reissuance asset: {e}"))?;
            let no_rt = AssetId::from_slice(&cp.no_reissuance_token)
                .map_err(|e| format!("bad NO reissuance asset: {e}"))?;

            // Fill in explicit amounts on reissuance-token outputs (indices 0, 1)
            let outputs = sdk_pset.outputs_mut();
            outputs[0].amount = Some(1);
            outputs[0].asset = Some(yes_rt);
            outputs[1].amount = Some(1);
            outputs[1].asset = Some(no_rt);

            // Use the change address's blinding pubkey for all blinded outputs.
            let blinding_pk = change_addr
                .blinding_pubkey
                .ok_or_else(|| "change address has no blinding key".to_string())?;
            let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
                inner: blinding_pk,
                compressed: true,
            };

            // Mark reissuance-token outputs (0, 1) and change output (3) for
            // blinding. The fee output (2) stays explicit.
            for idx in [0usize, 1] {
                outputs[idx].blinding_key = Some(pset_blinding_key);
                outputs[idx].blinder_index = Some(0);
            }
            // Change output exists when there are 4 outputs
            if outputs.len() == 4 {
                outputs[3].blinding_key = Some(pset_blinding_key);
                outputs[3].blinder_index = Some(0);
            }

            // Mark issuance as explicit (required by blind_last)
            let inputs = sdk_pset.inputs_mut();
            inputs[0].blinded_issuance = Some(0x00);
            inputs[1].blinded_issuance = Some(0x00);

            // Build input TxOutSecrets map for blinding
            let mut inp_txout_sec = HashMap::new();
            inp_txout_sec.insert(0usize, yes_utxo.unblinded);
            inp_txout_sec.insert(1usize, no_utxo.unblinded);

            let secp = lwk_wollet::elements::secp256k1_zkp::Secp256k1::new();
            let mut rng = thread_rng();
            sdk_pset
                .blind_last(&mut rng, &secp, &inp_txout_sec)
                .map_err(|e| format!("blind PSET: {e:?}"))?;
        }

        // ── Sign, broadcast, sync ───────────────────────────────────────
        let tx = wallet.sign_pset(sdk_pset).map_err(|e| format!("sign: {e}"))?;
        let txid = wallet
            .broadcast_and_sync(&tx)
            .map_err(|e| format!("broadcast: {e}"))?;

        let params = *contract.params();

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok((txid.to_string(), params))
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    // ── 7. Publish Nostr announcement with real asset IDs ───────────────
    let metadata = ContractMetadata {
        question: request.question,
        description: request.description,
        category: request.category,
        resolution_source: request.resolution_source,
        starting_yes_price: request.starting_yes_price,
    };

    let announcement = ContractAnnouncement {
        version: 1,
        contract_params,
        metadata,
        creation_txid: Some(creation_txid),
    };

    let event = discovery::build_announcement_event(&keys, &announcement)?;

    let client = get_or_connect_nostr_client(&sdk_state).await?;
    let event_id = discovery::publish_event(&client, event.clone()).await?;
    let market = discovery::parse_announcement_event(&event)?;

    Ok(DiscoveredMarket {
        id: event_id.to_hex(),
        ..market
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lwk_wollet::elements::bitcoin::hashes::Hash;
    use lwk_wollet::elements::confidential::{AssetBlindingFactor, ValueBlindingFactor};
    use lwk_wollet::elements::{AddressParams, OutPoint, Script, TxOutSecrets, Txid};
    use lwk_wollet::Chain;

    fn make_utxo(value: u64, asset: AssetId, vout: u32, spent: bool) -> WalletTxOut {
        let addr = lwk_wollet::elements::Address::p2sh(
            &Script::new(),
            None,
            &AddressParams::LIQUID_TESTNET,
        );
        WalletTxOut {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            script_pubkey: Script::new(),
            height: Some(100),
            unblinded: TxOutSecrets {
                asset,
                asset_bf: AssetBlindingFactor::zero(),
                value,
                value_bf: ValueBlindingFactor::zero(),
            },
            wildcard_index: 0,
            ext_int: Chain::External,
            is_spent: spent,
            address: addr,
        }
    }

    fn policy_asset() -> AssetId {
        "0000000000000000000000000000000000000000000000000000000000000001"
            .parse()
            .unwrap()
    }

    fn other_asset() -> AssetId {
        "0000000000000000000000000000000000000000000000000000000000000002"
            .parse()
            .unwrap()
    }

    #[test]
    fn select_defining_utxos_happy_path() {
        let pa = policy_asset();
        let utxos = vec![
            make_utxo(500, pa, 0, false),
            make_utxo(1000, pa, 1, false),
            make_utxo(800, pa, 2, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 1000);
        assert_eq!(b.unblinded.value, 800);
    }

    #[test]
    fn select_defining_utxos_excludes_below_min() {
        let pa = policy_asset();
        let utxos = vec![
            make_utxo(100, pa, 0, false), // too small
            make_utxo(500, pa, 1, false),
            make_utxo(200, pa, 2, false), // too small
            make_utxo(600, pa, 3, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 600);
        assert_eq!(b.unblinded.value, 500);
    }

    #[test]
    fn select_defining_utxos_excludes_spent() {
        let pa = policy_asset();
        let utxos = vec![
            make_utxo(1000, pa, 0, true), // spent
            make_utxo(500, pa, 1, false),
            make_utxo(600, pa, 2, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 600);
        assert_eq!(b.unblinded.value, 500);
    }

    #[test]
    fn select_defining_utxos_excludes_wrong_asset() {
        let pa = policy_asset();
        let other = other_asset();
        let utxos = vec![
            make_utxo(1000, other, 0, false), // wrong asset
            make_utxo(500, pa, 1, false),
            make_utxo(600, pa, 2, false),
        ];
        let (a, b) = select_defining_utxos(&utxos, pa, 300).unwrap();
        assert_eq!(a.unblinded.value, 600);
        assert_eq!(b.unblinded.value, 500);
    }

    #[test]
    fn select_defining_utxos_fewer_than_two() {
        let pa = policy_asset();
        let utxos = vec![make_utxo(500, pa, 0, false)];
        let result = select_defining_utxos(&utxos, pa, 300);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("need at least 2"));
    }

    #[test]
    fn select_defining_utxos_empty() {
        let pa = policy_asset();
        let result = select_defining_utxos(&[], pa, 300);
        assert!(result.is_err());
    }
}
