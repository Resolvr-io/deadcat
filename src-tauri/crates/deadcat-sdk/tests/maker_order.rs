use deadcat_sdk::elements::AddressParams;
use deadcat_sdk::elements::confidential::{Asset, Nonce, Value as ConfValue};
use deadcat_sdk::elements::{AssetId, Script, TxOut, TxOutWitness};
use deadcat_sdk::maker_order::pset::cancel_order::{CancelOrderParams, build_cancel_order_pset};
use deadcat_sdk::maker_order::pset::create_order::{CreateOrderParams, build_create_order_pset};
use deadcat_sdk::maker_order::pset::fill_order::{
    FillOrderParams, MakerOrderFill, TakerFill, build_fill_order_pset,
};
use deadcat_sdk::maker_order::taproot::maker_order_control_block;
use deadcat_sdk::maker_order::witness::{
    build_maker_order_cancel_witness, satisfy_maker_order, serialize_satisfied,
};
use deadcat_sdk::taproot::NUMS_KEY_BYTES;
use deadcat_sdk::{
    CompiledMakerOrder, MakerOrderParams, OrderDirection, UnblindedUtxo, derive_maker_receive,
};
use simplicityhl::elements::OutPoint;

const BASE_ASSET: [u8; 32] = [0x01; 32];
const QUOTE_ASSET: [u8; 32] = [0xbb; 32];
const FEE_ASSET: [u8; 32] = [0xbb; 32]; // same as quote for simplicity
const MAKER_PUBKEY: [u8; 32] = [0xaa; 32];

fn sell_base_params() -> MakerOrderParams {
    let (params, _p_order) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        50_000,
        1,
        1,
        OrderDirection::SellBase,
        NUMS_KEY_BYTES,
        &MAKER_PUBKEY,
        &[0x11; 32],
    );
    params
}

fn sell_quote_params() -> MakerOrderParams {
    let (params, _p_order) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        50_000,
        1,
        1,
        OrderDirection::SellQuote,
        NUMS_KEY_BYTES,
        &MAKER_PUBKEY,
        &[0x22; 32],
    );
    params
}

fn explicit_txout(asset_id: &[u8; 32], amount: u64, script_pubkey: &Script) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_id).expect("valid asset id")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: script_pubkey.clone(),
        witness: TxOutWitness::default(),
    }
}

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

// ============================================================================
// Compilation tests
// ============================================================================

#[test]
fn sell_base_compiles() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).expect("sell-base should compile");
    let cmr_bytes: &[u8] = contract.cmr().as_ref();
    assert!(cmr_bytes.iter().any(|&b| b != 0), "CMR should be non-zero");
}

#[test]
fn sell_quote_compiles() {
    let params = sell_quote_params();
    let contract = CompiledMakerOrder::new(params).expect("sell-quote should compile");
    let cmr_bytes: &[u8] = contract.cmr().as_ref();
    assert!(cmr_bytes.iter().any(|&b| b != 0), "CMR should be non-zero");
}

#[test]
fn different_directions_different_cmr() {
    let c1 = CompiledMakerOrder::new(sell_base_params()).unwrap();
    let c2 = CompiledMakerOrder::new(sell_quote_params()).unwrap();
    assert_ne!(
        c1.cmr().as_ref() as &[u8],
        c2.cmr().as_ref() as &[u8],
        "sell-base and sell-quote should have different CMRs"
    );
}

#[test]
fn different_prices_different_cmr() {
    let params1 = sell_base_params();
    let (params2, _) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        100_000,
        1,
        1,
        OrderDirection::SellBase,
        NUMS_KEY_BYTES,
        &MAKER_PUBKEY,
        &[0x11; 32],
    );

    let c1 = CompiledMakerOrder::new(params1).unwrap();
    let c2 = CompiledMakerOrder::new(params2).unwrap();
    assert_ne!(
        c1.cmr().as_ref() as &[u8],
        c2.cmr().as_ref() as &[u8],
        "different prices should produce different CMRs"
    );
}

#[test]
fn script_pubkey_is_p2tr() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let spk = contract.script_pubkey(&MAKER_PUBKEY);
    let bytes = spk.as_bytes();
    assert_eq!(bytes.len(), 34);
    assert_eq!(bytes[0], 0x51); // OP_1
    assert_eq!(bytes[1], 0x20); // PUSH32
}

#[test]
fn address_is_deterministic() {
    let params = sell_base_params();
    let c1 = CompiledMakerOrder::new(params).unwrap();
    let c2 = CompiledMakerOrder::new(params).unwrap();
    assert_eq!(
        c1.address(&MAKER_PUBKEY, &AddressParams::LIQUID)
            .to_string(),
        c2.address(&MAKER_PUBKEY, &AddressParams::LIQUID)
            .to_string(),
    );
}

#[test]
fn different_pubkeys_different_addresses() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let addr1 = contract
        .address(&[0xaa; 32], &AddressParams::LIQUID)
        .to_string();
    // Use NUMS key as a known-valid second pubkey
    let addr2 = contract
        .address(&NUMS_KEY_BYTES, &AddressParams::LIQUID)
        .to_string();
    assert_ne!(addr1, addr2);
}

// ============================================================================
// Control block tests
// ============================================================================

#[test]
fn control_block_is_33_bytes() {
    let order = CompiledMakerOrder::new(sell_base_params()).expect("compile");
    let cb = maker_order_control_block(order.cmr(), &MAKER_PUBKEY);
    assert_eq!(cb.len(), 33);
    assert_eq!(cb[0] & 0xfe, 0xbe); // Simplicity leaf version (masking parity)
    assert_eq!(&cb[1..33], &MAKER_PUBKEY);
}

// ============================================================================
// Witness satisfaction tests
// ============================================================================

#[test]
fn witness_satisfy_no_cosigner() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let no_sig = [0u8; 64];
    let satisfied =
        satisfy_maker_order(&contract, &no_sig).expect("should satisfy with NUMS cosigner");
    let (prog, wit) = serialize_satisfied(&satisfied);
    assert!(!prog.is_empty());
    assert!(!wit.is_empty());
}

#[test]
fn witness_satisfy_sell_quote_no_cosigner() {
    let params = sell_quote_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let no_sig = [0u8; 64];
    let satisfied =
        satisfy_maker_order(&contract, &no_sig).expect("should satisfy sell-quote with NUMS");
    let (prog, wit) = serialize_satisfied(&satisfied);
    assert!(!prog.is_empty());
    assert!(!wit.is_empty());
}

// ============================================================================
// Create order PSET tests
// ============================================================================

#[test]
fn create_order_happy_path() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        order_amount: 100,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: None,
        fee_change_destination: None,
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let pset = build_create_order_pset(&contract, &create_params).unwrap();
    assert_eq!(pset.inputs().len(), 2);
    // order + fee = 2 outputs
    assert_eq!(pset.outputs().len(), 2);
}

#[test]
fn create_order_with_change() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(BASE_ASSET, 200),
        fee_utxo: test_utxo(FEE_ASSET, 1000),
        order_amount: 100,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: Some(Script::new()),
        fee_change_destination: Some(Script::new()),
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let pset = build_create_order_pset(&contract, &create_params).unwrap();
    // order + fee + funding_change + fee_change = 4 outputs
    assert_eq!(pset.outputs().len(), 4);
}

#[test]
fn create_order_zero_amount() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        order_amount: 0,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: None,
        fee_change_destination: None,
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let result = build_create_order_pset(&contract, &create_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::ZeroOrderAmount)));
}

#[test]
fn create_order_insufficient_funding() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(BASE_ASSET, 50),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        order_amount: 100,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: None,
        fee_change_destination: None,
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let result = build_create_order_pset(&contract, &create_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::InsufficientCollateral)
    ));
}

// ============================================================================
// Fill order PSET tests
// ============================================================================

#[test]
fn fill_sell_base_full() {
    let params = sell_base_params();
    // Maker has 10 BASE lots at price 50_000 -> taker pays 500_000 QUOTE
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
            receive_destination: Script::new(),
            receive_amount: 10,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 500_000, // 10 * 50_000
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let pset = build_fill_order_pset(&fill_params).unwrap();
    // 1 taker + 1 order + 1 fee = 3 inputs
    assert_eq!(pset.inputs().len(), 3);
    // 1 taker receive + 1 maker receive + fee = 3 outputs (no remainder)
    assert_eq!(pset.outputs().len(), 3);
}

#[test]
fn fill_sell_base_partial() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 250_000),
            receive_destination: Script::new(),
            receive_amount: 5,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 250_000, // 5 * 50_000
            maker_receive_script: Script::new(),
            is_partial: true,
            remainder_amount: 5,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let pset = build_fill_order_pset(&fill_params).unwrap();
    // 1 taker receive + 1 maker receive + 1 remainder + fee = 4 outputs
    assert_eq!(pset.outputs().len(), 4);
}

#[test]
fn fill_sell_quote_full() {
    let params = sell_quote_params();
    // Maker has 500_000 QUOTE at price 50_000 → maker receives 10 BASE lots
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(BASE_ASSET, 10),
            receive_destination: Script::new(),
            receive_amount: 500_000,
            receive_asset_id: QUOTE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(QUOTE_ASSET, 500_000),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 10, // 500_000 / 50_000
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let pset = build_fill_order_pset(&fill_params).unwrap();
    assert_eq!(pset.inputs().len(), 3);
    assert_eq!(pset.outputs().len(), 3);
}

#[test]
fn fill_sell_quote_partial() {
    let params = sell_quote_params();
    // Maker has 500_000 QUOTE, taker buys 5 lots (250_000 QUOTE), remainder = 250_000
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(BASE_ASSET, 5),
            receive_destination: Script::new(),
            receive_amount: 250_000,
            receive_asset_id: QUOTE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(QUOTE_ASSET, 500_000),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 5,
            maker_receive_script: Script::new(),
            is_partial: true,
            remainder_amount: 250_000, // 500_000 - 5 * 50_000
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let pset = build_fill_order_pset(&fill_params).unwrap();
    assert_eq!(pset.outputs().len(), 4);
}

#[test]
fn fill_batch_two_takers_two_orders() {
    let params1 = sell_base_params();
    let params2 = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![
            TakerFill {
                funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
                receive_destination: Script::new(),
                receive_amount: 10,
                receive_asset_id: BASE_ASSET,
                change_destination: None,
                change_amount: 0,
                change_asset_id: [0u8; 32],
            },
            TakerFill {
                funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
                receive_destination: Script::new(),
                receive_amount: 10,
                receive_asset_id: BASE_ASSET,
                change_destination: None,
                change_amount: 0,
                change_asset_id: [0u8; 32],
            },
        ],
        orders: vec![
            MakerOrderFill {
                contract: CompiledMakerOrder::new(params1).unwrap(),
                order_utxo: test_utxo(BASE_ASSET, 10),
                maker_base_pubkey: MAKER_PUBKEY,
                maker_receive_amount: 500_000,
                maker_receive_script: Script::new(),
                is_partial: false,
                remainder_amount: 0,
            },
            MakerOrderFill {
                contract: CompiledMakerOrder::new(params2).unwrap(),
                order_utxo: test_utxo(BASE_ASSET, 10),
                maker_base_pubkey: MAKER_PUBKEY,
                maker_receive_amount: 500_000,
                maker_receive_script: Script::new(),
                is_partial: false,
                remainder_amount: 0,
            },
        ],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let pset = build_fill_order_pset(&fill_params).unwrap();
    // 2 takers + 2 orders + 1 fee = 5 inputs
    assert_eq!(pset.inputs().len(), 5);
    // 2 taker receives + 2 maker receives + fee = 5 outputs
    assert_eq!(pset.outputs().len(), 5);
}

#[test]
fn fill_below_minimum_rejected() {
    let (params, _) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        50_000,
        5,
        1,
        OrderDirection::SellBase,
        NUMS_KEY_BYTES,
        &MAKER_PUBKEY,
        &[0x11; 32],
    );

    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 100_000),
            receive_destination: Script::new(),
            receive_amount: 2,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 2), // only 2 lots, min is 5
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 100_000,
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::FillBelowMinimum)));
}

#[test]
fn remainder_below_minimum_rejected() {
    let (params, _) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        50_000,
        1,
        5,
        OrderDirection::SellBase,
        NUMS_KEY_BYTES,
        &MAKER_PUBKEY,
        &[0x11; 32],
    );

    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 400_000),
            receive_destination: Script::new(),
            receive_amount: 8,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 400_000, // 8 * 50_000
            maker_receive_script: Script::new(),
            is_partial: true,
            remainder_amount: 2, // below min_remainder_lots=5
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::RemainderBelowMinimum)
    ));
}

#[test]
fn conservation_violation_rejected() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 999_999),
            receive_destination: Script::new(),
            receive_amount: 10,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 999_999, // wrong: should be 500_000
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::ConservationViolation)
    ));
}

#[test]
fn fill_no_takers_rejected() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 500_000,
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::Pset(_))));
}

#[test]
fn fill_insufficient_fee_rejected() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
            receive_destination: Script::new(),
            receive_amount: 10,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 500_000,
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 100),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::InsufficientFee)));
}

#[test]
fn partial_fill_not_last_rejected() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
            receive_destination: Script::new(),
            receive_amount: 10,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![
            MakerOrderFill {
                contract: CompiledMakerOrder::new(params).unwrap(),
                order_utxo: test_utxo(BASE_ASSET, 10),
                maker_base_pubkey: MAKER_PUBKEY,
                maker_receive_amount: 250_000,
                maker_receive_script: Script::new(),
                is_partial: true, // NOT last — should fail
                remainder_amount: 5,
            },
            MakerOrderFill {
                contract: CompiledMakerOrder::new(params).unwrap(),
                order_utxo: test_utxo(BASE_ASSET, 10),
                maker_base_pubkey: MAKER_PUBKEY,
                maker_receive_amount: 500_000,
                maker_receive_script: Script::new(),
                is_partial: false,
                remainder_amount: 0,
            },
        ],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::PartialFillNotLast)
    ));
}

// ============================================================================
// Cancel order PSET tests
// ============================================================================

#[test]
fn cancel_order_happy_path() {
    let cancel_params = CancelOrderParams {
        order_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        order_asset_id: BASE_ASSET,
        refund_destination: Script::new(),
        fee_change_destination: None,
    };
    let pset = build_cancel_order_pset(&cancel_params).unwrap();
    assert_eq!(pset.inputs().len(), 2);
    // refund + fee = 2 outputs
    assert_eq!(pset.outputs().len(), 2);
}

#[test]
fn cancel_order_with_fee_change() {
    let cancel_params = CancelOrderParams {
        order_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 1000),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        order_asset_id: BASE_ASSET,
        refund_destination: Script::new(),
        fee_change_destination: Some(Script::new()),
    };
    let pset = build_cancel_order_pset(&cancel_params).unwrap();
    // refund + fee + fee_change = 3 outputs
    assert_eq!(pset.outputs().len(), 3);
}

#[test]
fn cancel_order_insufficient_fee() {
    let cancel_params = CancelOrderParams {
        order_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 100),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        order_asset_id: BASE_ASSET,
        refund_destination: Script::new(),
        fee_change_destination: None,
    };
    let result = build_cancel_order_pset(&cancel_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::InsufficientFee)));
}

// ============================================================================
// P_order derivation tests
// ============================================================================

#[test]
fn p_order_unique_per_nonce() {
    let params = sell_base_params();
    let (p1, _) = derive_maker_receive(&MAKER_PUBKEY, &[0x11; 32], &params);
    let (p2, _) = derive_maker_receive(&MAKER_PUBKEY, &[0x22; 32], &params);
    assert_ne!(p1, p2, "different nonces should produce different P_order");
}

#[test]
fn p_order_unique_per_pubkey() {
    let params = sell_base_params();
    let nonce = [0x11; 32];
    let (p1, _) = derive_maker_receive(&[0xaa; 32], &nonce, &params);
    let (p2, _) = derive_maker_receive(&NUMS_KEY_BYTES, &nonce, &params);
    assert_ne!(p1, p2, "different pubkeys should produce different P_order");
}

// ============================================================================
// Additional coverage tests
// ============================================================================

#[test]
fn create_order_sell_quote() {
    let params = sell_quote_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        order_amount: 500_000,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: None,
        fee_change_destination: None,
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let pset = build_create_order_pset(&contract, &create_params).unwrap();
    assert_eq!(pset.inputs().len(), 2);
    assert_eq!(pset.outputs().len(), 2);
}

#[test]
fn fill_with_fee_change() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
            receive_destination: Script::new(),
            receive_amount: 10,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, 10),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: 500_000,
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 1000),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: Some(Script::new()),
    };
    let pset = build_fill_order_pset(&fill_params).unwrap();
    // 1 taker receive + 1 maker receive + fee + fee_change = 4 outputs
    assert_eq!(pset.outputs().len(), 4);
}

#[test]
fn fill_overflow_rejected() {
    let params = sell_base_params();
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, u64::MAX),
            receive_destination: Script::new(),
            receive_amount: u64::MAX,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![MakerOrderFill {
            contract: CompiledMakerOrder::new(params).unwrap(),
            order_utxo: test_utxo(BASE_ASSET, u64::MAX),
            maker_base_pubkey: MAKER_PUBKEY,
            maker_receive_amount: u64::MAX,
            maker_receive_script: Script::new(),
            is_partial: false,
            remainder_amount: 0,
        }],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::MakerOrderOverflow)
    ));
}

#[test]
fn create_order_zero_price_rejected() {
    let (params, _) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        0,
        1,
        1,
        OrderDirection::SellBase,
        NUMS_KEY_BYTES,
        &MAKER_PUBKEY,
        &[0x11; 32],
    );
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        order_amount: 100,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: None,
        fee_change_destination: None,
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let result = build_create_order_pset(&contract, &create_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::ZeroPrice)));
}

#[test]
fn cosigner_compiles() {
    let cosigner_key = [0xff; 32];
    let (params_cosigner, _) = MakerOrderParams::new(
        BASE_ASSET,
        QUOTE_ASSET,
        50_000,
        1,
        1,
        OrderDirection::SellBase,
        cosigner_key,
        &MAKER_PUBKEY,
        &[0x11; 32],
    );
    assert!(params_cosigner.has_cosigner());
    let c_cosigner = CompiledMakerOrder::new(params_cosigner).unwrap();

    let params_nums = sell_base_params();
    assert!(!params_nums.has_cosigner());
    let c_nums = CompiledMakerOrder::new(params_nums).unwrap();

    assert_ne!(
        c_cosigner.cmr().as_ref() as &[u8],
        c_nums.cmr().as_ref() as &[u8],
        "cosigner vs NUMS should produce different CMRs"
    );
}

#[test]
fn fill_no_orders_rejected() {
    let fill_params = FillOrderParams {
        takers: vec![TakerFill {
            funding_utxo: test_utxo(QUOTE_ASSET, 500_000),
            receive_destination: Script::new(),
            receive_amount: 10,
            receive_asset_id: BASE_ASSET,
            change_destination: None,
            change_amount: 0,
            change_asset_id: [0u8; 32],
        }],
        orders: vec![],
        fee_utxo: test_utxo(FEE_ASSET, 500),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        fee_change_destination: None,
    };
    let result = build_fill_order_pset(&fill_params);
    assert!(matches!(result, Err(deadcat_sdk::Error::Pset(_))));
}

#[test]
fn create_order_excess_funding_no_change_rejected() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let create_params = CreateOrderParams {
        funding_utxo: test_utxo(BASE_ASSET, 200),
        fee_utxo: test_utxo(FEE_ASSET, 500),
        order_amount: 100,
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        change_destination: None, // no change destination but excess funding
        fee_change_destination: None,
        maker_base_pubkey: MAKER_PUBKEY,
    };
    let result = build_create_order_pset(&contract, &create_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::MissingChangeDestination)
    ));
}

#[test]
fn cancel_order_excess_fee_no_change_rejected() {
    let cancel_params = CancelOrderParams {
        order_utxo: test_utxo(BASE_ASSET, 100),
        fee_utxo: test_utxo(FEE_ASSET, 1000),
        fee_amount: 500,
        fee_asset_id: FEE_ASSET,
        order_asset_id: BASE_ASSET,
        refund_destination: Script::new(),
        fee_change_destination: None, // no change destination but excess fee
    };
    let result = build_cancel_order_pset(&cancel_params);
    assert!(matches!(
        result,
        Err(deadcat_sdk::Error::MissingChangeDestination)
    ));
}

// ============================================================================
// Cancel witness satisfaction test
// ============================================================================

#[test]
fn cancel_witness_satisfies() {
    let params = sell_base_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let dummy_sig = [0xab; 64];
    let witness = build_maker_order_cancel_witness(&dummy_sig);
    let satisfied = contract
        .program()
        .satisfy(witness)
        .expect("cancel witness should satisfy the contract");
    let (prog, wit) = serialize_satisfied(&satisfied);
    assert!(!prog.is_empty());
    assert!(!wit.is_empty());
}

#[test]
fn cancel_witness_satisfies_sell_quote() {
    let params = sell_quote_params();
    let contract = CompiledMakerOrder::new(params).unwrap();
    let dummy_sig = [0xcd; 64];
    let witness = build_maker_order_cancel_witness(&dummy_sig);
    let satisfied = contract
        .program()
        .satisfy(witness)
        .expect("cancel witness should satisfy sell-quote contract");
    let (prog, wit) = serialize_satisfied(&satisfied);
    assert!(!prog.is_empty());
    assert!(!wit.is_empty());
}
