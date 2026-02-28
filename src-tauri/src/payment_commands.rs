use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::state::{AppStateManager, PaymentSwap};
use crate::{emit_state, NodeState};

fn current_network(app: &AppHandle) -> Result<crate::Network, String> {
    let manager = app.state::<Mutex<AppStateManager>>();
    let mgr = manager
        .lock()
        .map_err(|_| "state lock failed".to_string())?;
    mgr.network()
        .ok_or("Not initialized - select a network first".to_string())
}

async fn save_swap_and_emit(
    app: &AppHandle,
    swap: PaymentSwap,
    task_label: &str,
) -> Result<(), String> {
    let app_ref = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.upsert_payment_swap(swap);
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("{task_label} save task failed: {e}"))??;
    Ok(())
}

#[tauri::command]
pub async fn pay_lightning_invoice(
    invoice: String,
    app: AppHandle,
) -> Result<crate::payments::boltz::BoltzSubmarineSwapCreated, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let refund_pubkey_hex = node
        .boltz_submarine_refund_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    drop(guard);

    let boltz = crate::payments::boltz::BoltzService::new(current_network(&app)?, None);
    let created = boltz
        .create_submarine_swap(&invoice, &refund_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.invoice_amount_sat,
        expected_amount_sat: Some(created.expected_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: Some(invoice),
        invoice_expiry_seconds: Some(created.invoice_expiry_seconds),
        invoice_expires_at: Some(created.invoice_expires_at.clone()),
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    save_swap_and_emit(&app, saved_swap, "pay_lightning").await?;
    Ok(created)
}

#[tauri::command]
pub async fn create_lightning_receive(
    amount_sat: u64,
    app: AppHandle,
) -> Result<crate::payments::boltz::BoltzLightningReceiveCreated, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let claim_pubkey_hex = node
        .boltz_reverse_claim_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    drop(guard);

    let boltz = crate::payments::boltz::BoltzService::new(current_network(&app)?, None);
    let created = boltz
        .create_lightning_receive(amount_sat, &claim_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.invoice_amount_sat,
        expected_amount_sat: Some(created.expected_onchain_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: Some(created.invoice.clone()),
        invoice_expiry_seconds: Some(created.invoice_expiry_seconds),
        invoice_expires_at: Some(created.invoice_expires_at.clone()),
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    save_swap_and_emit(&app, saved_swap, "lightning_receive").await?;
    Ok(created)
}

#[tauri::command]
pub async fn create_bitcoin_receive(
    amount_sat: u64,
    app: AppHandle,
) -> Result<crate::payments::boltz::BoltzChainSwapCreated, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let claim_pubkey_hex = node
        .boltz_reverse_claim_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    let refund_pubkey_hex = node
        .boltz_submarine_refund_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    drop(guard);

    let boltz = crate::payments::boltz::BoltzService::new(current_network(&app)?, None);
    let created = boltz
        .create_chain_swap_btc_to_lbtc(amount_sat, &claim_pubkey_hex, &refund_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.amount_sat,
        expected_amount_sat: Some(created.expected_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: None,
        invoice_expiry_seconds: None,
        invoice_expires_at: None,
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    save_swap_and_emit(&app, saved_swap, "bitcoin_receive").await?;
    Ok(created)
}

#[tauri::command]
pub async fn create_bitcoin_send(
    amount_sat: u64,
    app: AppHandle,
) -> Result<crate::payments::boltz::BoltzChainSwapCreated, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let claim_pubkey_hex = node
        .boltz_reverse_claim_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    let refund_pubkey_hex = node
        .boltz_submarine_refund_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    drop(guard);

    let boltz = crate::payments::boltz::BoltzService::new(current_network(&app)?, None);
    let created = boltz
        .create_chain_swap_lbtc_to_btc(amount_sat, &claim_pubkey_hex, &refund_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.amount_sat,
        expected_amount_sat: Some(created.expected_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: None,
        invoice_expiry_seconds: None,
        invoice_expires_at: None,
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    save_swap_and_emit(&app, saved_swap, "bitcoin_send").await?;
    Ok(created)
}

#[tauri::command]
pub async fn get_chain_swap_pairs(
    app: AppHandle,
) -> Result<crate::payments::boltz::BoltzChainSwapPairsInfo, String> {
    let boltz = crate::payments::boltz::BoltzService::new(current_network(&app)?, None);
    boltz
        .get_chain_swap_pairs_info()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_payment_swaps(app: AppHandle) -> Result<Vec<PaymentSwap>, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        Ok(mgr.payment_swaps().to_vec())
    })
    .await
    .map_err(|e| format!("list_swaps task failed: {e}"))?
}

#[tauri::command]
pub async fn refresh_payment_swap_status(
    swap_id: String,
    app: AppHandle,
) -> Result<PaymentSwap, String> {
    let swap_id_clone = swap_id.clone();
    let boltz = crate::payments::boltz::BoltzService::new(current_network(&app)?, None);
    let status = boltz
        .get_swap_status(&swap_id_clone)
        .await
        .map_err(|e| e.to_string())?;

    let app_ref = app.clone();
    let updated_swap = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let existing = mgr
            .payment_swaps()
            .iter()
            .find(|swap| swap.id == swap_id_clone)
            .cloned()
            .ok_or_else(|| format!("Payment swap not found: {}", swap_id_clone))?;

        let mut updated = existing;
        updated.status = status.status;
        updated.lockup_txid = status.lockup_txid;
        updated.updated_at = chrono::Utc::now().to_rfc3339();

        mgr.upsert_payment_swap(updated.clone());
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(updated)
    })
    .await
    .map_err(|e| format!("refresh_swap save task failed: {e}"))??;

    Ok(updated_swap)
}
