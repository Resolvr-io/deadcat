import { invoke } from "@tauri-apps/api/core";
import type {
  CreateLmsrPoolResponse,
  LmsrPoolInfo,
  PriceHistoryEntry,
  ScanLmsrPoolResponse,
} from "../types.ts";

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

export async function createLmsrPool(
  marketParamsJson: string,
  poolParamsJson: string,
  initialSIndex: number,
  initialReservesYes: number,
  initialReservesNo: number,
  initialReservesLbtc: number,
  tableValues: number[],
  feeAmount?: number,
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
      fee_amount: feeAmount,
    },
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
