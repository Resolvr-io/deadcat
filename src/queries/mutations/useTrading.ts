import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  cancelLimitOrder,
  createLimitOrder,
  executeTrade,
  quoteTrade,
} from "../../services/markets";
import type {
  CancelLimitOrderResponse,
  CreateLimitOrderResponse,
  DiscoveredOrder,
  ExecuteTradeExpectedQuote,
  ExecuteTradeResponse,
  Market,
  Side,
  TradeDirection,
  TradeQuoteResponse,
  TxOptions,
} from "../../types";

export function useQuoteTrade() {
  return useMutation({
    mutationFn: (params: {
      market: Market;
      side: Side;
      direction: TradeDirection;
      exactInput: number;
    }): Promise<TradeQuoteResponse> =>
      quoteTrade(
        params.market,
        params.side,
        params.direction,
        params.exactInput,
      ),
  });
}

export function useExecuteTrade() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      side: Side;
      direction: TradeDirection;
      exactInput: number;
      txOptions?: TxOptions;
      expectedQuote?: ExecuteTradeExpectedQuote;
    }): Promise<ExecuteTradeResponse> =>
      executeTrade(
        params.market,
        params.side,
        params.direction,
        params.exactInput,
        params.txOptions,
        params.expectedQuote,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
      void queryClient.invalidateQueries({ queryKey: ["orders"] });
      void queryClient.invalidateQueries({ queryKey: ["ownOrders"] });
      void queryClient.invalidateQueries({ queryKey: ["pools"] });
    },
  });
}

export function useCreateLimitOrder() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      market: Market;
      side: Side;
      direction: TradeDirection;
      price: number;
      amount: number;
      txOptions?: TxOptions;
    }): Promise<CreateLimitOrderResponse> =>
      createLimitOrder(
        params.market,
        params.side,
        params.direction,
        params.price,
        params.amount,
        params.txOptions,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["orders"] });
      void queryClient.invalidateQueries({ queryKey: ["ownOrders"] });
    },
  });
}

export function useCancelLimitOrder() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      order: DiscoveredOrder;
      orderIndex?: number;
      txOptions?: TxOptions;
    }): Promise<CancelLimitOrderResponse> =>
      cancelLimitOrder(params.order, params.orderIndex, params.txOptions),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
      void queryClient.invalidateQueries({ queryKey: ["orders"] });
      void queryClient.invalidateQueries({ queryKey: ["ownOrders"] });
    },
  });
}
