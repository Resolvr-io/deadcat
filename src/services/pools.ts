import { invoke } from "@tauri-apps/api/core";
import type {
  CloseLmsrPoolResponse,
  CreateLmsrPoolResponse,
  LmsrPoolInfo,
  PriceHistoryEntry,
  ScanLmsrPoolResponse,
  TxOptions,
} from "../types.ts";
import { DEFAULT_TX_OPTIONS } from "./tx.ts";

export async function generateLmsrTable(
  liquidityParam: number,
  tableDepth: number,
  qStepLots: number,
  sBias: number,
  halfPayoutSats: number,
): Promise<number[]> {
  return invoke<number[]>("generate_lmsr_table", {
    liquidityParam,
    tableDepth,
    qStepLots,
    sBias,
    halfPayoutSats,
  });
}

export async function buildPoolParamsJson(params: {
  yesAssetId: string;
  noAssetId: string;
  collateralAssetId: string;
  tableDepth: number;
  qStepLots: number;
  sBias: number;
  sMaxIndex: number;
  halfPayoutSats: number;
  feeBps: number;
  minRYes: number;
  minRNo: number;
  minRCollateral: number;
  cosignerPubkey: string;
  tableValues: number[];
}): Promise<string> {
  return invoke<string>("build_pool_params_json", {
    request: {
      yes_asset_id: params.yesAssetId,
      no_asset_id: params.noAssetId,
      collateral_asset_id: params.collateralAssetId,
      table_depth: params.tableDepth,
      q_step_lots: params.qStepLots,
      s_bias: params.sBias,
      s_max_index: params.sMaxIndex,
      half_payout_sats: params.halfPayoutSats,
      fee_bps: params.feeBps,
      min_r_yes: params.minRYes,
      min_r_no: params.minRNo,
      min_r_collateral: params.minRCollateral,
      cosigner_pubkey: params.cosignerPubkey,
      table_values: params.tableValues,
    },
  });
}

export async function createLmsrPool(
  marketParamsJson: string,
  poolParamsJson: string,
  initialSIndex: number,
  initialReservesYes: number,
  initialReservesNo: number,
  initialReservesLbtc: number,
  tableValues: number[],
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<CreateLmsrPoolResponse> {
  return invoke<CreateLmsrPoolResponse>("create_lmsr_pool", {
    request: {
      market_params_json: marketParamsJson,
      pool_params_json: poolParamsJson,
      initial_s_index: initialSIndex,
      initial_reserves_yes: initialReservesYes,
      initial_reserves_no: initialReservesNo,
      initial_reserves_lbtc: initialReservesLbtc,
      table_values: tableValues,
      tx_options: txOptions,
    },
  });
}

export async function closeLmsrPool(
  poolId: string,
  txOptions: TxOptions = DEFAULT_TX_OPTIONS,
): Promise<CloseLmsrPoolResponse> {
  return invoke<CloseLmsrPoolResponse>("close_lmsr_pool", {
    request: { pool_id: poolId, tx_options: txOptions },
  });
}

export async function scanLmsrPool(
  poolId: string,
): Promise<ScanLmsrPoolResponse> {
  return invoke<ScanLmsrPoolResponse>("scan_lmsr_pool", { poolId });
}

export async function listLmsrPools(
  marketId?: string,
): Promise<LmsrPoolInfo[]> {
  return invoke<LmsrPoolInfo[]>("list_lmsr_pools", { marketId });
}

export async function getPriceHistory(
  marketId: string,
  limit?: number,
): Promise<PriceHistoryEntry[]> {
  return invoke<PriceHistoryEntry[]>("get_price_history", {
    marketId,
    limit,
  });
}

export async function getPoolPriceHistory(
  poolId: string,
  limit?: number,
  sinceBlockHeight?: number,
): Promise<PriceHistoryEntry[]> {
  return invoke<PriceHistoryEntry[]>("get_pool_price_history", {
    poolId,
    limit,
    sinceBlockHeight,
  });
}
