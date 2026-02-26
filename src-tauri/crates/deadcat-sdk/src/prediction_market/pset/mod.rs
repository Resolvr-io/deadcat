pub mod cancellation;
pub mod creation;
pub mod expiry_redemption;
pub mod initial_issuance;
pub mod issuance;
pub mod oracle_resolve;
pub mod post_resolution_redemption;

use simplicityhl::elements::Script;

use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::state::MarketState;

// Re-export shared PSET helpers so submodules can continue using `super::`.
pub(crate) use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, burn_txout, explicit_txout, fee_txout,
    new_pset, reissuance_token_output,
};

/// Get the covenant script pubkey for a given state.
pub(crate) fn covenant_spk(contract: &CompiledPredictionMarket, state: MarketState) -> Script {
    contract.script_pubkey(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::prediction_market::params::PredictionMarketParams;
    use simplicityhl::elements::LockTime;
    use simplicityhl::elements::{AssetId, OutPoint};

    const TEST_ASSET: [u8; 32] = [0xaa; 32];

    #[test]
    fn burn_txout_script_is_empty() {
        let txout = burn_txout(&TEST_ASSET, 1000);
        assert!(txout.script_pubkey.is_empty());
    }

    #[test]
    fn burn_txout_hash_matches_empty_script() {
        use sha2::{Digest, Sha256};

        let txout = burn_txout(&TEST_ASSET, 1000);
        let hash: [u8; 32] = Sha256::digest(txout.script_pubkey.as_bytes()).into();
        // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let expected = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash, expected);
    }

    #[test]
    fn add_pset_output_sets_asset_field() {
        let txout = explicit_txout(&TEST_ASSET, 5000, &Script::new());
        let expected_asset = AssetId::from_slice(&TEST_ASSET).unwrap();
        let mut pset = new_pset();
        add_pset_output(&mut pset, txout);
        assert_eq!(pset.outputs()[0].asset, Some(expected_asset));
    }

    #[test]
    fn add_pset_output_sets_amount_field() {
        let txout = explicit_txout(&TEST_ASSET, 5000, &Script::new());
        let mut pset = new_pset();
        add_pset_output(&mut pset, txout);
        assert_eq!(pset.outputs()[0].amount, Some(5000));
    }

    // -- Shared helpers for builder tests --

    fn test_utxo(asset_id: [u8; 32], value: u64) -> UnblindedUtxo {
        UnblindedUtxo {
            outpoint: OutPoint::default(),
            txout: explicit_txout(&asset_id, value, &Script::new()),
            asset_id,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    fn test_contract() -> CompiledPredictionMarket {
        let params = PredictionMarketParams {
            oracle_public_key: [0xaa; 32],
            collateral_asset_id: [0xbb; 32],
            yes_token_asset: [0x01; 32],
            no_token_asset: [0x02; 32],
            yes_reissuance_token: [0x03; 32],
            no_reissuance_token: [0x04; 32],
            collateral_per_token: 100_000,
            expiry_time: 1_000_000,
        };
        CompiledPredictionMarket::new(params).expect("test contract should compile")
    }

    // ===== build_creation_pset =====

    #[test]
    fn creation_happy_path() {
        let contract = test_contract();
        let params = creation::CreationParams {
            yes_defining_utxo: test_utxo([0xdd; 32], 1000),
            no_defining_utxo: test_utxo([0xee; 32], 1000),
            fee_amount: 500,
            change_destination: None,
            lock_time: 100,
        };
        let pset = creation::build_creation_pset(&contract, &params).unwrap();
        // 2 defining UTXOs
        assert_eq!(pset.inputs().len(), 2);
        // 2 reissuance tokens + 1 fee = 3
        assert_eq!(pset.outputs().len(), 3);
        assert_eq!(pset.inputs()[0].issuance_inflation_keys, Some(1));
        assert_eq!(pset.inputs()[1].issuance_inflation_keys, Some(1));
        assert_eq!(
            pset.global.tx_data.fallback_locktime,
            Some(LockTime::from_consensus(100))
        );
    }

    #[test]
    fn creation_with_change() {
        let contract = test_contract();
        let params = creation::CreationParams {
            yes_defining_utxo: test_utxo([0xdd; 32], 1000),
            no_defining_utxo: test_utxo([0xee; 32], 1000),
            fee_amount: 500,
            change_destination: Some(Script::new()),
            lock_time: 100,
        };
        let pset = creation::build_creation_pset(&contract, &params).unwrap();
        // 2 reissuance tokens + fee + change = 4
        assert_eq!(pset.outputs().len(), 4);
    }

    #[test]
    fn creation_outputs_target_dormant() {
        let contract = test_contract();
        let params = creation::CreationParams {
            yes_defining_utxo: test_utxo([0xdd; 32], 1000),
            no_defining_utxo: test_utxo([0xee; 32], 1000),
            fee_amount: 500,
            change_destination: None,
            lock_time: 100,
        };
        let pset = creation::build_creation_pset(&contract, &params).unwrap();
        let dormant_spk = contract.script_pubkey(MarketState::Dormant);
        assert_eq!(pset.outputs()[0].script_pubkey, dormant_spk);
        assert_eq!(pset.outputs()[1].script_pubkey, dormant_spk);
        // No asset issuance — only reissuance tokens
        assert_eq!(pset.inputs()[0].issuance_value_amount, None);
        assert_eq!(pset.inputs()[1].issuance_value_amount, None);
    }

    #[test]
    fn creation_insufficient_fee() {
        let contract = test_contract();
        let params = creation::CreationParams {
            yes_defining_utxo: test_utxo([0xdd; 32], 100),
            no_defining_utxo: test_utxo([0xee; 32], 100),
            fee_amount: 500,
            change_destination: None,
            lock_time: 100,
        };
        let result = creation::build_creation_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientFee)));
    }

    // ===== build_initial_issuance_pset =====

    #[test]
    fn initial_issuance_happy_path() {
        let contract = test_contract();
        let p = contract.params();
        let params = initial_issuance::InitialIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 100,
        };
        let pset = initial_issuance::build_initial_issuance_pset(&contract, &params).unwrap();
        // 2 reissuance + 1 collateral + 1 fee = 4 inputs
        assert_eq!(pset.inputs().len(), 4);
        // 2 reissuance tokens + collateral + YES + NO + fee = 6
        assert_eq!(pset.outputs().len(), 6);
        assert_eq!(pset.inputs()[0].issuance_value_amount, Some(10));
        assert_eq!(pset.inputs()[1].issuance_value_amount, Some(10));
        // Output 2 collateral = required (10 * 2 * 100_000 = 2_000_000)
        assert_eq!(pset.outputs()[2].amount, Some(2_000_000));
    }

    #[test]
    fn initial_issuance_outputs_target_unresolved() {
        let contract = test_contract();
        let p = contract.params();
        let params = initial_issuance::InitialIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 100,
        };
        let pset = initial_issuance::build_initial_issuance_pset(&contract, &params).unwrap();
        let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
        // Reissuance token outputs → Unresolved
        assert_eq!(pset.outputs()[0].script_pubkey, unresolved_spk);
        assert_eq!(pset.outputs()[1].script_pubkey, unresolved_spk);
        // Collateral output → Unresolved
        assert_eq!(pset.outputs()[2].script_pubkey, unresolved_spk);
    }

    #[test]
    fn initial_issuance_with_change() {
        let contract = test_contract();
        let p = contract.params();
        let params = initial_issuance::InitialIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 3_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: Some(Script::new()),
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 100,
        };
        let pset = initial_issuance::build_initial_issuance_pset(&contract, &params).unwrap();
        assert_eq!(pset.outputs().len(), 7);
    }

    #[test]
    fn initial_issuance_insufficient_collateral() {
        let contract = test_contract();
        let p = contract.params();
        let params = initial_issuance::InitialIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 1_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 100,
        };
        let result = initial_issuance::build_initial_issuance_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientCollateral)));
    }

    #[test]
    fn initial_issuance_collateral_overflow() {
        let contract = test_contract();
        let p = contract.params();
        let params = initial_issuance::InitialIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, u64::MAX),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: u64::MAX,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 100,
        };
        let result = initial_issuance::build_initial_issuance_pset(&contract, &params);
        assert!(matches!(result, Err(Error::CollateralOverflow)));
    }

    // ===== build_subsequent_issuance_pset =====

    #[test]
    fn subsequent_issuance_happy_path() {
        let contract = test_contract();
        let p = contract.params();
        let params = issuance::SubsequentIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 5_000_000),
            new_collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 200,
        };
        let pset = issuance::build_subsequent_issuance_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 5);
        assert_eq!(pset.outputs().len(), 6);
        assert_eq!(pset.inputs()[0].issuance_value_amount, Some(10));
        assert_eq!(pset.inputs()[1].issuance_value_amount, Some(10));
        assert_eq!(
            pset.global.tx_data.fallback_locktime,
            Some(LockTime::from_consensus(200))
        );
    }

    #[test]
    fn subsequent_issuance_collateral_accumulates() {
        let contract = test_contract();
        let p = contract.params();
        let params = issuance::SubsequentIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 5_000_000),
            new_collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 200,
        };
        let pset = issuance::build_subsequent_issuance_pset(&contract, &params).unwrap();
        // Output 2 is collateral: old (5M) + new (2M) = 7M
        assert_eq!(pset.outputs()[2].amount, Some(7_000_000));
    }

    #[test]
    fn subsequent_issuance_insufficient_collateral() {
        let contract = test_contract();
        let p = contract.params();
        let params = issuance::SubsequentIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 5_000_000),
            new_collateral_utxo: test_utxo(p.collateral_asset_id, 1_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: 10,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 200,
        };
        let result = issuance::build_subsequent_issuance_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientCollateral)));
    }

    #[test]
    fn subsequent_issuance_collateral_overflow() {
        let contract = test_contract();
        let p = contract.params();
        let params = issuance::SubsequentIssuanceParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 5_000_000),
            new_collateral_utxo: test_utxo(p.collateral_asset_id, u64::MAX),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs: u64::MAX,
            fee_amount: 500,
            yes_token_destination: Script::new(),
            no_token_destination: Script::new(),
            collateral_change_destination: None,
            fee_change_destination: None,
            yes_issuance_blinding_nonce: [0x01; 32],
            yes_issuance_asset_entropy: [0x01; 32],
            no_issuance_blinding_nonce: [0x01; 32],
            no_issuance_asset_entropy: [0x02; 32],
            lock_time: 200,
        };
        let result = issuance::build_subsequent_issuance_pset(&contract, &params);
        assert!(matches!(result, Err(Error::CollateralOverflow)));
    }

    // ===== build_oracle_resolve_pset =====

    #[test]
    fn oracle_resolve_yes() {
        let contract = test_contract();
        let p = contract.params();
        let params = oracle_resolve::OracleResolveParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 5_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            outcome_yes: true,
            fee_amount: 500,
            lock_time: 300,
        };
        let pset = oracle_resolve::build_oracle_resolve_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 4);
        assert_eq!(pset.outputs().len(), 4);
        let yes_spk = contract.script_pubkey(MarketState::ResolvedYes);
        assert_eq!(pset.outputs()[0].script_pubkey, yes_spk);
        assert_eq!(pset.outputs()[2].script_pubkey, yes_spk);
    }

    #[test]
    fn oracle_resolve_no() {
        let contract = test_contract();
        let p = contract.params();
        let params = oracle_resolve::OracleResolveParams {
            yes_reissuance_utxo: test_utxo(p.yes_reissuance_token, 1),
            no_reissuance_utxo: test_utxo(p.no_reissuance_token, 1),
            collateral_utxo: test_utxo(p.collateral_asset_id, 5_000_000),
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            outcome_yes: false,
            fee_amount: 500,
            lock_time: 300,
        };
        let pset = oracle_resolve::build_oracle_resolve_pset(&contract, &params).unwrap();
        let no_spk = contract.script_pubkey(MarketState::ResolvedNo);
        assert_eq!(pset.outputs()[0].script_pubkey, no_spk);
        assert_eq!(pset.outputs()[2].script_pubkey, no_spk);
    }

    // ===== build_post_resolution_redemption_pset =====

    #[test]
    fn post_res_redemption_partial() {
        let contract = test_contract();
        let p = contract.params();
        let params = post_resolution_redemption::PostResolutionRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 5)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 5,
            resolved_state: MarketState::ResolvedYes,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let pset =
            post_resolution_redemption::build_post_resolution_redemption_pset(&contract, &params)
                .unwrap();
        // 1 collateral + 1 token + 1 fee = 3 inputs
        assert_eq!(pset.inputs().len(), 3);
        // covenant (remaining) + burn + payout + fee = 4 outputs
        assert_eq!(pset.outputs().len(), 4);
    }

    #[test]
    fn post_res_redemption_full() {
        let contract = test_contract();
        let p = contract.params();
        let params = post_resolution_redemption::PostResolutionRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 10)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 10,
            resolved_state: MarketState::ResolvedYes,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let pset =
            post_resolution_redemption::build_post_resolution_redemption_pset(&contract, &params)
                .unwrap();
        // remaining == 0 → no covenant output: burn + payout + fee = 3
        assert_eq!(pset.outputs().len(), 3);
    }

    #[test]
    fn post_res_redemption_invalid_state() {
        let contract = test_contract();
        let p = contract.params();
        let params = post_resolution_redemption::PostResolutionRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 5)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 5,
            resolved_state: MarketState::Unresolved,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let result =
            post_resolution_redemption::build_post_resolution_redemption_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InvalidState)));
    }

    #[test]
    fn post_res_redemption_insufficient_collateral() {
        let contract = test_contract();
        let p = contract.params();
        let params = post_resolution_redemption::PostResolutionRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 1_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 20)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 20,
            resolved_state: MarketState::ResolvedYes,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let result =
            post_resolution_redemption::build_post_resolution_redemption_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientCollateral)));
    }

    // ===== build_expiry_redemption_pset =====

    #[test]
    fn expiry_redemption_partial() {
        let contract = test_contract();
        let p = contract.params();
        let params = expiry_redemption::ExpiryRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 5)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 5,
            burn_token_asset: p.yes_token_asset,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
            lock_time: 999_999,
        };
        let pset = expiry_redemption::build_expiry_redemption_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 3);
        // remaining > 0 → covenant + burn + payout + fee = 4
        assert_eq!(pset.outputs().len(), 4);
    }

    #[test]
    fn expiry_redemption_full() {
        let contract = test_contract();
        let p = contract.params();
        let params = expiry_redemption::ExpiryRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 20)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 20,
            burn_token_asset: p.yes_token_asset,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
            lock_time: 999_999,
        };
        let pset = expiry_redemption::build_expiry_redemption_pset(&contract, &params).unwrap();
        // remaining == 0 → burn + payout + fee = 3
        assert_eq!(pset.outputs().len(), 3);
    }

    #[test]
    fn expiry_redemption_insufficient_collateral() {
        let contract = test_contract();
        let p = contract.params();
        let params = expiry_redemption::ExpiryRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 30)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 30,
            burn_token_asset: p.yes_token_asset,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
            lock_time: 999_999,
        };
        let result = expiry_redemption::build_expiry_redemption_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientCollateral)));
    }

    #[test]
    fn expiry_redemption_sets_locktime() {
        let contract = test_contract();
        let p = contract.params();
        let params = expiry_redemption::ExpiryRedemptionParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            token_utxos: vec![test_utxo(p.yes_token_asset, 5)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            tokens_burned: 5,
            burn_token_asset: p.yes_token_asset,
            fee_amount: 500,
            payout_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
            lock_time: 999_999,
        };
        let pset = expiry_redemption::build_expiry_redemption_pset(&contract, &params).unwrap();
        assert_eq!(
            pset.global.tx_data.fallback_locktime,
            Some(LockTime::from_consensus(999_999))
        );
    }

    // ===== build_cancellation_pset =====

    #[test]
    fn cancellation_partial() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            yes_reissuance_utxo: None,
            no_reissuance_utxo: None,
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, 5)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, 5)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: 5,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let pset = cancellation::build_cancellation_pset(&contract, &params).unwrap();
        // 1 collateral + 1 yes + 1 no + 1 fee = 4 inputs
        assert_eq!(pset.inputs().len(), 4);
        // covenant + yes_burn + no_burn + refund + fee = 5 outputs
        assert_eq!(pset.outputs().len(), 5);
    }

    #[test]
    fn cancellation_full() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            yes_reissuance_utxo: Some(test_utxo(p.yes_reissuance_token, 1)),
            no_reissuance_utxo: Some(test_utxo(p.no_reissuance_token, 1)),
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, 10)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, 10)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: 10,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let pset = cancellation::build_cancellation_pset(&contract, &params).unwrap();
        // 1 collateral + 1 yes_reissuance + 1 no_reissuance + 1 yes + 1 no + 1 fee = 6 inputs
        assert_eq!(pset.inputs().len(), 6);
        // 2 reissuance→dormant + yes_burn + no_burn + refund + fee = 6 outputs
        assert_eq!(pset.outputs().len(), 6);
    }

    #[test]
    fn cancellation_partial_outputs_target_unresolved() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            yes_reissuance_utxo: None,
            no_reissuance_utxo: None,
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, 5)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, 5)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: 5,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let pset = cancellation::build_cancellation_pset(&contract, &params).unwrap();
        let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
        // Output 0: remaining collateral → Unresolved
        assert_eq!(pset.outputs()[0].script_pubkey, unresolved_spk);
    }

    #[test]
    fn cancellation_full_outputs_target_dormant() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            yes_reissuance_utxo: Some(test_utxo(p.yes_reissuance_token, 1)),
            no_reissuance_utxo: Some(test_utxo(p.no_reissuance_token, 1)),
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, 10)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, 10)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: 10,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let pset = cancellation::build_cancellation_pset(&contract, &params).unwrap();
        let dormant_spk = contract.script_pubkey(MarketState::Dormant);
        // Outputs 0,1: reissuance tokens → Dormant
        assert_eq!(pset.outputs()[0].script_pubkey, dormant_spk);
        assert_eq!(pset.outputs()[1].script_pubkey, dormant_spk);
    }

    #[test]
    fn cancellation_full_missing_reissuance() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 2_000_000),
            yes_reissuance_utxo: None,
            no_reissuance_utxo: None,
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, 10)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, 10)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: 10,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let result = cancellation::build_cancellation_pset(&contract, &params);
        assert!(matches!(result, Err(Error::MissingReissuanceUtxos)));
    }

    #[test]
    fn cancellation_insufficient_collateral() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, 1_000_000),
            yes_reissuance_utxo: None,
            no_reissuance_utxo: None,
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, 10)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, 10)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: 10,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let result = cancellation::build_cancellation_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientCollateral)));
    }

    #[test]
    fn cancellation_collateral_overflow() {
        let contract = test_contract();
        let p = contract.params();
        let params = cancellation::CancellationParams {
            collateral_utxo: test_utxo(p.collateral_asset_id, u64::MAX),
            yes_reissuance_utxo: None,
            no_reissuance_utxo: None,
            yes_token_utxos: vec![test_utxo(p.yes_token_asset, u64::MAX)],
            no_token_utxos: vec![test_utxo(p.no_token_asset, u64::MAX)],
            fee_utxo: test_utxo(p.collateral_asset_id, 500),
            pairs_burned: u64::MAX,
            fee_amount: 500,
            refund_destination: Script::new(),
            fee_change_destination: None,
            token_change_destination: None,
        };
        let result = cancellation::build_cancellation_pset(&contract, &params);
        assert!(matches!(result, Err(Error::CollateralOverflow)));
    }
}
