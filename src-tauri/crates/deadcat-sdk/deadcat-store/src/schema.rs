// @generated automatically by Diesel CLI.

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
    lmsr_pools (pool_id) {
        pool_id -> Text,
        market_id -> Text,
        creation_txid -> Text,
        witness_schema_version -> Text,
        current_s_index -> BigInt,
        reserve_yes -> BigInt,
        reserve_no -> BigInt,
        reserve_collateral -> BigInt,
        reserve_yes_outpoint -> Text,
        reserve_no_outpoint -> Text,
        reserve_collateral_outpoint -> Text,
        state_source -> Text,
        last_transition_txid -> Nullable<Text>,
        params_json -> Text,
        nostr_event_id -> Nullable<Text>,
        nostr_event_json -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    market_candidates (candidate_id) {
        candidate_id -> Integer,
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
        dormant_yes_rt_spk -> Binary,
        dormant_no_rt_spk -> Binary,
        unresolved_yes_rt_spk -> Binary,
        unresolved_no_rt_spk -> Binary,
        unresolved_collateral_spk -> Binary,
        resolved_yes_collateral_spk -> Binary,
        resolved_no_collateral_spk -> Binary,
        expired_collateral_spk -> Binary,
        yes_issuance_entropy -> Nullable<Binary>,
        no_issuance_entropy -> Nullable<Binary>,
        yes_issuance_blinding_nonce -> Nullable<Binary>,
        no_issuance_blinding_nonce -> Nullable<Binary>,
        question -> Nullable<Text>,
        description -> Nullable<Text>,
        category -> Nullable<Text>,
        resolution_source -> Nullable<Text>,
        creator_pubkey -> Nullable<Binary>,
        creation_txid -> Text,
        yes_dormant_asset_blinding_factor -> Binary,
        yes_dormant_value_blinding_factor -> Binary,
        no_dormant_asset_blinding_factor -> Binary,
        no_dormant_value_blinding_factor -> Binary,
        creation_tx -> Binary,
        nevent -> Nullable<Text>,
        nostr_event_id -> Nullable<Text>,
        nostr_event_json -> Nullable<Text>,
        first_seen_at -> Text,
        last_seen_at -> Text,
        expires_at -> Nullable<Text>,
        promoted_at -> Nullable<Text>,
        promotion_height -> Nullable<Integer>,
        promotion_block_hash -> Nullable<Binary>,
    }
}

diesel::table! {
    markets (market_id) {
        market_id -> Binary,
        candidate_id -> Integer,
        current_state -> Integer,
        created_at -> Text,
        updated_at -> Text,
        dormant_txid -> Nullable<Text>,
        unresolved_txid -> Nullable<Text>,
        resolved_yes_txid -> Nullable<Text>,
        resolved_no_txid -> Nullable<Text>,
        expired_txid -> Nullable<Text>,
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
        market_slot -> Nullable<Integer>,
        spent -> Integer,
        spending_txid -> Nullable<Binary>,
        block_height -> Nullable<Integer>,
        spent_block_height -> Nullable<Integer>,
    }
}

diesel::joinable!(markets -> market_candidates (candidate_id));
diesel::joinable!(utxos -> maker_orders (maker_order_id));
diesel::joinable!(utxos -> markets (market_id));

diesel::allow_tables_to_appear_in_same_query!(
    lmsr_pools,
    maker_orders,
    market_candidates,
    markets,
    sync_state,
    utxos,
);
