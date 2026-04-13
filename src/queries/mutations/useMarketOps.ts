import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  cancelTokens,
  issueTokens,
  redeemExpiredTokens,
  redeemTokens,
  resolveMarket,
} from "../../services/markets";
import type {
  CancellationResult,
  IssuanceResult,
  Market,
  RedemptionResult,
  ResolutionResult,
  TxOptions,
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
    }): Promise<RedemptionResult> => {
      const anchor = params.market.anchor;
      if (!anchor) throw new Error("Market anchor is required for redemption");
      return redeemTokens(
        params.market,
        anchor,
        params.tokens,
        params.txOptions,
      );
    },
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
    }): Promise<RedemptionResult> => {
      const anchor = params.market.anchor;
      if (!anchor) throw new Error("Market anchor is required for redemption");
      return redeemExpiredTokens(
        params.market,
        anchor,
        params.tokenAssetHex,
        params.tokens,
        params.txOptions,
      );
    },
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
    }): Promise<CancellationResult> => {
      const anchor = params.market.anchor;
      if (!anchor)
        throw new Error("Market anchor is required for cancellation");
      return cancelTokens(
        params.market,
        anchor,
        params.pairs,
        params.txOptions,
      );
    },
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
    }): Promise<ResolutionResult> => {
      const anchor = params.market.anchor;
      if (!anchor) throw new Error("Market anchor is required for resolution");
      return resolveMarket(
        params.market,
        anchor,
        params.outcomeYes,
        params.oracleSignatureHex,
        params.txOptions,
      );
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
    },
  });
}
