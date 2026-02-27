// @generated automatically by Diesel CLI.

diesel::table! {
    amm_pools (pool_id) {
        pool_id -> Binary,
        yes_asset_id -> Binary,
        no_asset_id -> Binary,
        lbtc_asset_id -> Binary,
        lp_asset_id -> Binary,
        lp_reissuance_token_id -> Binary,
        fee_bps -> Integer,
        cosigner_pubkey -> Binary,
        cmr -> Binary,
        issued_lp -> BigInt,
        covenant_spk -> Binary,
        pool_status -> Integer,
        created_at -> Text,
        updated_at -> Text,
        nostr_event_id -> Nullable<Text>,
        nostr_event_json -> Nullable<Text>,
        market_id -> Nullable<Binary>,
        creation_txid -> Nullable<Binary>,
    }
}

diesel::table! {
    maker_orders (id) {
        id -> Integer,
        base_asset_id -> Binary,
        quote_asset_id -> Binary,
        price -> BigInt,
        min_fill_lots -> BigInt,
        min_remainder_lots -> BigInt,
        direction -> Integer,
        maker_receive_spk_hash -> Binary,
        cosigner_pubkey -> Binary,
        cmr -> Binary,
        maker_base_pubkey -> Nullable<Binary>,
        covenant_spk -> Nullable<Binary>,
        order_status -> Integer,
        created_at -> Text,
        updated_at -> Text,
        order_nonce -> Nullable<Binary>,
        maker_receive_spk -> Nullable<Binary>,
        nostr_event_id -> Nullable<Text>,
        nostr_event_json -> Nullable<Text>,
    }
}

diesel::table! {
    markets (market_id) {
        market_id -> Binary,
        oracle_public_key -> Binary,
        collateral_asset_id -> Binary,
        yes_token_asset -> Binary,
        no_token_asset -> Binary,
        yes_reissuance_token -> Binary,
        no_reissuance_token -> Binary,
        collateral_per_token -> BigInt,
        expiry_time -> Integer,
        cmr -> Binary,
        dormant_spk -> Binary,
        unresolved_spk -> Binary,
        resolved_yes_spk -> Binary,
        resolved_no_spk -> Binary,
        current_state -> Integer,
        created_at -> Text,
        updated_at -> Text,
        yes_issuance_entropy -> Nullable<Binary>,
        no_issuance_entropy -> Nullable<Binary>,
        yes_issuance_blinding_nonce -> Nullable<Binary>,
        no_issuance_blinding_nonce -> Nullable<Binary>,
        question -> Nullable<Text>,
        description -> Nullable<Text>,
        category -> Nullable<Text>,
        resolution_source -> Nullable<Text>,
        starting_yes_price -> Nullable<Integer>,
        creator_pubkey -> Nullable<Binary>,
        creation_txid -> Nullable<Text>,
        nevent -> Nullable<Text>,
        nostr_event_id -> Nullable<Text>,
        nostr_event_json -> Nullable<Text>,
        dormant_txid -> Nullable<Text>,
        unresolved_txid -> Nullable<Text>,
        resolved_yes_txid -> Nullable<Text>,
        resolved_no_txid -> Nullable<Text>,
    }
}

diesel::table! {
    pool_state_snapshots (id) {
        id -> Integer,
        pool_id -> Binary,
        txid -> Binary,
        r_yes -> BigInt,
        r_no -> BigInt,
        r_lbtc -> BigInt,
        issued_lp -> BigInt,
        block_height -> Nullable<Integer>,
        created_at -> Text,
    }
}

diesel::table! {
    sync_state (id) {
        id -> Integer,
        last_block_hash -> Nullable<Binary>,
        last_block_height -> Integer,
        updated_at -> Text,
    }
}

diesel::table! {
    utxos (txid, vout) {
        txid -> Binary,
        vout -> Integer,
        script_pubkey -> Binary,
        asset_id -> Binary,
        value -> BigInt,
        asset_blinding_factor -> Binary,
        value_blinding_factor -> Binary,
        raw_txout -> Binary,
        market_id -> Nullable<Binary>,
        maker_order_id -> Nullable<Integer>,
        market_state -> Nullable<Integer>,
        spent -> Integer,
        spending_txid -> Nullable<Binary>,
        block_height -> Nullable<Integer>,
        spent_block_height -> Nullable<Integer>,
        amm_pool_id -> Nullable<Binary>,
    }
}

diesel::joinable!(pool_state_snapshots -> amm_pools (pool_id));
diesel::joinable!(utxos -> amm_pools (amm_pool_id));
diesel::joinable!(utxos -> maker_orders (maker_order_id));
diesel::joinable!(utxos -> markets (market_id));

diesel::allow_tables_to_appear_in_same_query!(
    amm_pools,
    maker_orders,
    markets,
    pool_state_snapshots,
    sync_state,
    utxos,
);
