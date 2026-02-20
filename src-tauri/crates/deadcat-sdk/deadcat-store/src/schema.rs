// @generated automatically by Diesel CLI.

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

diesel::joinable!(utxos -> markets (market_id));
diesel::joinable!(utxos -> maker_orders (maker_order_id));

diesel::allow_tables_to_appear_in_same_query!(
    markets,
    maker_orders,
    utxos,
    sync_state,
);
