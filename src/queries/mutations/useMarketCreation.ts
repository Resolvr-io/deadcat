import { useMutation, useQueryClient } from "@tanstack/react-query";
import { createContractOnchain } from "../../services/markets";
import type {
  CreateContractOnchainResponse,
  MarketCategory,
  TxOptions,
} from "../../types";

export function useCreateMarket() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (params: {
      question: string;
      description: string;
      category: MarketCategory;
      resolution_source: string;
      settlement_deadline_unix: number;
      collateral_per_token: number;
      tx_options: TxOptions;
    }): Promise<CreateContractOnchainResponse> =>
      createContractOnchain(params),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["markets"] });
      void queryClient.invalidateQueries({ queryKey: ["walletSnapshot"] });
    },
  });
}
