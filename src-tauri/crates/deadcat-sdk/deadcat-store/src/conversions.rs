use deadcat_sdk::amm_pool::contract::CompiledAmmPool;
use deadcat_sdk::amm_pool::params::{AmmPoolParams, PoolId};
use deadcat_sdk::elements::encode::{deserialize as elements_deserialize, serialize};
use deadcat_sdk::elements::hashes::Hash;
use deadcat_sdk::elements::{OutPoint, TxOut, Txid};
use deadcat_sdk::{
    CompiledContract, CompiledMakerOrder, ContractParams, MakerOrderParams, MarketId, MarketState,
    OrderDirection, UnblindedUtxo, derive_maker_receive, maker_receive_script_pubkey,
};

use crate::error::StoreError;
use crate::models::{
    AmmPoolRow, MakerOrderRow, MarketRow, NewAmmPoolRow, NewMakerOrderRow, NewMarketRow,
    NewUtxoRow, UtxoRow,
};
use crate::store::{
    AmmPoolInfo, IssuanceData, MakerOrderInfo, MarketInfo, OrderStatus, PoolStatus,
};
use deadcat_sdk::discovery::ContractMetadataInput;

pub fn vec_to_array32(v: &[u8], field: &str) -> std::result::Result<[u8; 32], StoreError> {
    v.try_into().map_err(|_| {
        StoreError::InvalidData(format!("{field}: expected 32 bytes, got {}", v.len()))
    })
}

pub fn direction_to_i32(dir: OrderDirection) -> i32 {
    match dir {
        OrderDirection::SellBase => 0,
        OrderDirection::SellQuote => 1,
    }
}

pub fn direction_from_i32(v: i32) -> std::result::Result<OrderDirection, StoreError> {
    match v {
        0 => Ok(OrderDirection::SellBase),
        1 => Ok(OrderDirection::SellQuote),
        other => Err(StoreError::InvalidData(format!(
            "invalid order direction: {other}"
        ))),
    }
}

// --- MarketRow -> SDK types ---

impl TryFrom<&MarketRow> for ContractParams {
    type Error = StoreError;

    fn try_from(row: &MarketRow) -> std::result::Result<Self, Self::Error> {
        Ok(ContractParams {
            oracle_public_key: vec_to_array32(&row.oracle_public_key, "oracle_public_key")?,
            collateral_asset_id: vec_to_array32(&row.collateral_asset_id, "collateral_asset_id")?,
            yes_token_asset: vec_to_array32(&row.yes_token_asset, "yes_token_asset")?,
            no_token_asset: vec_to_array32(&row.no_token_asset, "no_token_asset")?,
            yes_reissuance_token: vec_to_array32(
                &row.yes_reissuance_token,
                "yes_reissuance_token",
            )?,
            no_reissuance_token: vec_to_array32(&row.no_reissuance_token, "no_reissuance_token")?,
            collateral_per_token: row.collateral_per_token as u64,
            // NOTE: expiry_time stored as i32 limits to ~2038 for epoch timestamps.
            // Block heights (the typical usage) are well within i32 range.
            expiry_time: row.expiry_time as u32,
        })
    }
}

impl TryFrom<&MarketRow> for MarketId {
    type Error = StoreError;

    fn try_from(row: &MarketRow) -> std::result::Result<Self, Self::Error> {
        Ok(MarketId(vec_to_array32(&row.market_id, "market_id")?))
    }
}

impl TryFrom<&MarketRow> for MarketState {
    type Error = StoreError;

    fn try_from(row: &MarketRow) -> std::result::Result<Self, Self::Error> {
        MarketState::from_u64(row.current_state as u64).ok_or_else(|| {
            StoreError::InvalidData(format!("invalid market state: {}", row.current_state))
        })
    }
}

impl TryFrom<&MarketRow> for MarketInfo {
    type Error = StoreError;

    fn try_from(row: &MarketRow) -> std::result::Result<Self, Self::Error> {
        let issuance = match (
            &row.yes_issuance_entropy,
            &row.no_issuance_entropy,
            &row.yes_issuance_blinding_nonce,
            &row.no_issuance_blinding_nonce,
        ) {
            (Some(ye), Some(ne), Some(ybn), Some(nbn)) => Some(IssuanceData {
                yes_entropy: vec_to_array32(ye, "yes_issuance_entropy")?,
                no_entropy: vec_to_array32(ne, "no_issuance_entropy")?,
                yes_blinding_nonce: vec_to_array32(ybn, "yes_issuance_blinding_nonce")?,
                no_blinding_nonce: vec_to_array32(nbn, "no_issuance_blinding_nonce")?,
            }),
            _ => None,
        };

        Ok(MarketInfo {
            market_id: MarketId::try_from(row)?,
            params: ContractParams::try_from(row)?,
            state: MarketState::try_from(row)?,
            cmr: vec_to_array32(&row.cmr, "cmr")?,
            issuance,
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
            question: row.question.clone(),
            description: row.description.clone(),
            category: row.category.clone(),
            resolution_source: row.resolution_source.clone(),
            starting_yes_price: row.starting_yes_price.map(|v| v as u8),
            creator_pubkey: row.creator_pubkey.clone(),
            creation_txid: row.creation_txid.clone(),
            nevent: row.nevent.clone(),
            nostr_event_id: row.nostr_event_id.clone(),
            nostr_event_json: row.nostr_event_json.clone(),
        })
    }
}

// --- ContractParams + CompiledContract -> NewMarketRow ---

pub fn new_market_row(
    params: &ContractParams,
    compiled: &CompiledContract,
    metadata: Option<&ContractMetadataInput>,
) -> NewMarketRow {
    let market_id = params.market_id();
    NewMarketRow {
        market_id: market_id.as_bytes().to_vec(),
        oracle_public_key: params.oracle_public_key.to_vec(),
        collateral_asset_id: params.collateral_asset_id.to_vec(),
        yes_token_asset: params.yes_token_asset.to_vec(),
        no_token_asset: params.no_token_asset.to_vec(),
        yes_reissuance_token: params.yes_reissuance_token.to_vec(),
        no_reissuance_token: params.no_reissuance_token.to_vec(),
        collateral_per_token: params.collateral_per_token as i64,
        expiry_time: params.expiry_time as i32,
        cmr: compiled.cmr().as_ref().to_vec(),
        dormant_spk: compiled
            .script_pubkey(MarketState::Dormant)
            .as_bytes()
            .to_vec(),
        unresolved_spk: compiled
            .script_pubkey(MarketState::Unresolved)
            .as_bytes()
            .to_vec(),
        resolved_yes_spk: compiled
            .script_pubkey(MarketState::ResolvedYes)
            .as_bytes()
            .to_vec(),
        resolved_no_spk: compiled
            .script_pubkey(MarketState::ResolvedNo)
            .as_bytes()
            .to_vec(),
        question: metadata.and_then(|m| m.question.clone()),
        description: metadata.and_then(|m| m.description.clone()),
        category: metadata.and_then(|m| m.category.clone()),
        resolution_source: metadata.and_then(|m| m.resolution_source.clone()),
        starting_yes_price: metadata.and_then(|m| m.starting_yes_price.map(|v| v as i32)),
        creator_pubkey: metadata.and_then(|m| m.creator_pubkey.clone()),
        creation_txid: metadata.and_then(|m| m.creation_txid.clone()),
        nevent: metadata.and_then(|m| m.nevent.clone()),
        nostr_event_id: metadata.and_then(|m| m.nostr_event_id.clone()),
        nostr_event_json: metadata.and_then(|m| m.nostr_event_json.clone()),
    }
}

// --- MakerOrderRow -> SDK types ---

impl TryFrom<&MakerOrderRow> for MakerOrderParams {
    type Error = StoreError;

    fn try_from(row: &MakerOrderRow) -> std::result::Result<Self, Self::Error> {
        let maker_pubkey = row
            .maker_base_pubkey
            .as_ref()
            .map(|v| vec_to_array32(v, "maker_base_pubkey"))
            .transpose()?
            .unwrap_or([0u8; 32]);
        Ok(MakerOrderParams {
            base_asset_id: vec_to_array32(&row.base_asset_id, "base_asset_id")?,
            quote_asset_id: vec_to_array32(&row.quote_asset_id, "quote_asset_id")?,
            price: row.price as u64,
            min_fill_lots: row.min_fill_lots as u64,
            min_remainder_lots: row.min_remainder_lots as u64,
            direction: direction_from_i32(row.direction)?,
            maker_receive_spk_hash: vec_to_array32(
                &row.maker_receive_spk_hash,
                "maker_receive_spk_hash",
            )?,
            cosigner_pubkey: vec_to_array32(&row.cosigner_pubkey, "cosigner_pubkey")?,
            maker_pubkey,
        })
    }
}

impl TryFrom<&MakerOrderRow> for MakerOrderInfo {
    type Error = StoreError;

    fn try_from(row: &MakerOrderRow) -> std::result::Result<Self, Self::Error> {
        Ok(MakerOrderInfo {
            id: row.id,
            params: MakerOrderParams::try_from(row)?,
            status: OrderStatus::from_i32(row.order_status)?,
            cmr: vec_to_array32(&row.cmr, "cmr")?,
            maker_base_pubkey: row
                .maker_base_pubkey
                .as_ref()
                .map(|v| vec_to_array32(v, "maker_base_pubkey"))
                .transpose()?,
            order_nonce: row
                .order_nonce
                .as_ref()
                .map(|v| vec_to_array32(v, "order_nonce"))
                .transpose()?,
            nostr_event_id: row.nostr_event_id.clone(),
            nostr_event_json: row.nostr_event_json.clone(),
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
        })
    }
}

// --- MakerOrderParams + CompiledMakerOrder -> NewMakerOrderRow ---

pub fn new_maker_order_row(
    params: &MakerOrderParams,
    compiled: &CompiledMakerOrder,
    maker_base_pubkey: Option<&[u8; 32]>,
    order_nonce: Option<&[u8; 32]>,
    nostr_event_id: Option<&str>,
    nostr_event_json: Option<&str>,
) -> NewMakerOrderRow {
    let covenant_spk = maker_base_pubkey.map(|pk| compiled.script_pubkey(pk).as_bytes().to_vec());

    // Compute maker_receive_spk when both pubkey and nonce are present
    let maker_receive_spk = match (maker_base_pubkey, order_nonce) {
        (Some(pubkey), Some(nonce)) => {
            let (p_order, _) = derive_maker_receive(pubkey, nonce, params);
            Some(maker_receive_script_pubkey(&p_order))
        }
        _ => None,
    };

    NewMakerOrderRow {
        base_asset_id: params.base_asset_id.to_vec(),
        quote_asset_id: params.quote_asset_id.to_vec(),
        price: params.price as i64,
        min_fill_lots: params.min_fill_lots as i64,
        min_remainder_lots: params.min_remainder_lots as i64,
        direction: direction_to_i32(params.direction),
        maker_receive_spk_hash: params.maker_receive_spk_hash.to_vec(),
        cosigner_pubkey: params.cosigner_pubkey.to_vec(),
        cmr: compiled.cmr().as_ref().to_vec(),
        maker_base_pubkey: maker_base_pubkey.map(|pk| pk.to_vec()),
        covenant_spk,
        order_nonce: order_nonce.map(|n| n.to_vec()),
        maker_receive_spk,
        nostr_event_id: nostr_event_id.map(|s| s.to_string()),
        nostr_event_json: nostr_event_json.map(|s| s.to_string()),
    }
}

// --- UtxoRow -> UnblindedUtxo ---

impl TryFrom<&UtxoRow> for UnblindedUtxo {
    type Error = StoreError;

    fn try_from(row: &UtxoRow) -> std::result::Result<Self, Self::Error> {
        let txid_bytes = vec_to_array32(&row.txid, "txid")?;
        let txid = Txid::from_byte_array(txid_bytes);
        let outpoint = OutPoint::new(txid, row.vout as u32);
        let txout: TxOut = elements_deserialize(&row.raw_txout)
            .map_err(|e| StoreError::InvalidData(format!("raw_txout deserialization: {e}")))?;
        Ok(UnblindedUtxo {
            outpoint,
            txout,
            asset_id: vec_to_array32(&row.asset_id, "asset_id")?,
            value: row.value as u64,
            asset_blinding_factor: vec_to_array32(
                &row.asset_blinding_factor,
                "asset_blinding_factor",
            )?,
            value_blinding_factor: vec_to_array32(
                &row.value_blinding_factor,
                "value_blinding_factor",
            )?,
        })
    }
}

// --- UnblindedUtxo -> NewUtxoRow ---

/// Build a `NewUtxoRow` from an `UnblindedUtxo`, associating it with either
/// a market (with state) or a maker order.
pub fn new_utxo_row(
    utxo: &UnblindedUtxo,
    market_id: Option<&MarketId>,
    market_state: Option<MarketState>,
    maker_order_id: Option<i32>,
    block_height: Option<u32>,
) -> NewUtxoRow {
    NewUtxoRow {
        txid: utxo.outpoint.txid.as_byte_array().to_vec(),
        vout: utxo.outpoint.vout as i32,
        script_pubkey: utxo.txout.script_pubkey.as_bytes().to_vec(),
        asset_id: utxo.asset_id.to_vec(),
        value: utxo.value as i64,
        asset_blinding_factor: utxo.asset_blinding_factor.to_vec(),
        value_blinding_factor: utxo.value_blinding_factor.to_vec(),
        raw_txout: serialize(&utxo.txout),
        market_id: market_id.map(|id| id.as_bytes().to_vec()),
        maker_order_id,
        market_state: market_state.map(|s| s.as_u64() as i32),
        block_height: block_height.map(|h| h as i32),
        amm_pool_id: None,
    }
}

// --- AmmPoolRow -> SDK types ---

impl TryFrom<&AmmPoolRow> for AmmPoolParams {
    type Error = StoreError;

    fn try_from(row: &AmmPoolRow) -> std::result::Result<Self, Self::Error> {
        Ok(AmmPoolParams {
            yes_asset_id: vec_to_array32(&row.yes_asset_id, "yes_asset_id")?,
            no_asset_id: vec_to_array32(&row.no_asset_id, "no_asset_id")?,
            lbtc_asset_id: vec_to_array32(&row.lbtc_asset_id, "lbtc_asset_id")?,
            lp_asset_id: vec_to_array32(&row.lp_asset_id, "lp_asset_id")?,
            lp_reissuance_token_id: vec_to_array32(
                &row.lp_reissuance_token_id,
                "lp_reissuance_token_id",
            )?,
            fee_bps: row.fee_bps as u64,
            cosigner_pubkey: vec_to_array32(&row.cosigner_pubkey, "cosigner_pubkey")?,
        })
    }
}

impl TryFrom<&AmmPoolRow> for PoolId {
    type Error = StoreError;

    fn try_from(row: &AmmPoolRow) -> std::result::Result<Self, Self::Error> {
        Ok(PoolId(vec_to_array32(&row.pool_id, "pool_id")?))
    }
}

impl TryFrom<&AmmPoolRow> for AmmPoolInfo {
    type Error = StoreError;

    fn try_from(row: &AmmPoolRow) -> std::result::Result<Self, Self::Error> {
        Ok(AmmPoolInfo {
            pool_id: PoolId::try_from(row)?,
            params: AmmPoolParams::try_from(row)?,
            status: PoolStatus::from_i32(row.pool_status)?,
            cmr: vec_to_array32(&row.cmr, "cmr")?,
            issued_lp: row.issued_lp as u64,
            r_yes: row.r_yes.map(|v| v as u64),
            r_no: row.r_no.map(|v| v as u64),
            r_lbtc: row.r_lbtc.map(|v| v as u64),
            covenant_spk: row.covenant_spk.clone(),
            nostr_event_id: row.nostr_event_id.clone(),
            nostr_event_json: row.nostr_event_json.clone(),
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
        })
    }
}

// --- AmmPoolParams + CompiledAmmPool -> NewAmmPoolRow ---

pub fn new_amm_pool_row(
    params: &AmmPoolParams,
    compiled: &CompiledAmmPool,
    issued_lp: u64,
    reserves: Option<&deadcat_sdk::amm_pool::math::PoolReserves>,
    nostr_event_id: Option<&str>,
    nostr_event_json: Option<&str>,
) -> NewAmmPoolRow {
    let pool_id = PoolId::from_params(params);
    NewAmmPoolRow {
        pool_id: pool_id.0.to_vec(),
        yes_asset_id: params.yes_asset_id.to_vec(),
        no_asset_id: params.no_asset_id.to_vec(),
        lbtc_asset_id: params.lbtc_asset_id.to_vec(),
        lp_asset_id: params.lp_asset_id.to_vec(),
        lp_reissuance_token_id: params.lp_reissuance_token_id.to_vec(),
        fee_bps: params.fee_bps as i32,
        cosigner_pubkey: params.cosigner_pubkey.to_vec(),
        cmr: compiled.cmr().as_ref().to_vec(),
        issued_lp: issued_lp as i64,
        r_yes: reserves.map(|r| r.r_yes as i64),
        r_no: reserves.map(|r| r.r_no as i64),
        r_lbtc: reserves.map(|r| r.r_lbtc as i64),
        covenant_spk: compiled.script_pubkey(issued_lp).as_bytes().to_vec(),
        pool_status: PoolStatus::Active.as_i32(),
        nostr_event_id: nostr_event_id.map(|s| s.to_string()),
        nostr_event_json: nostr_event_json.map(|s| s.to_string()),
    }
}
