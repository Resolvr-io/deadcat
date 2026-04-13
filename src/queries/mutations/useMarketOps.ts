import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  issueTokens,
  redeemTokens,
  redeemExpiredTokens,
  cancelTokens,
  resolveMarket,
} from "../../services/markets";
import type {
  Market,
  TxOptions,
  IssuanceResult,
  RedemptionResult,
  CancellationResult,
  ResolutionResult,
} from "../../types";

export function useIssueTokens() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      pairs: number;
      txOptions?: TxOptions;
    }): Promise<IssuanceResult> =>
      issueTokens(params.market, params.pairs, params.txOptions),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    },
  });
}

export function useRedeemTokens() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      tokens: number;
      txOptions?: TxOptions;
    }): Promise<RedemptionResult> =>
      redeemTokens(
        params.market,
        params.market.anchor!,
        params.tokens,
        params.txOptions,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    },
  });
}

export function useRedeemExpiredTokens() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      tokenAssetHex: string;
      tokens: number;
      txOptions?: TxOptions;
    }): Promise<RedemptionResult> =>
      redeemExpiredTokens(
        params.market,
        params.market.anchor!,
        params.tokenAssetHex,
        params.tokens,
        params.txOptions,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    },
  });
}

export function useCancelTokens() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      pairs: number;
      txOptions?: TxOptions;
    }): Promise<CancellationResult> =>
      cancelTokens(
        params.market,
        params.market.anchor!,
        params.pairs,
        params.txOptions,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    },
  });
}

export function useResolveMarket() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      outcomeYes: boolean;
      oracleSignatureHex: string;
      txOptions?: TxOptions;
    }): Promise<ResolutionResult> =>
      resolveMarket(
        params.market,
        params.market.anchor!,
        params.outcomeYes,
        params.oracleSignatureHex,
        params.txOptions,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    },
  });
}
