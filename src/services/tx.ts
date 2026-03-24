import type { TxOptions } from "../types.ts";

export const DEFAULT_TX_OPTIONS: TxOptions = {
  feePolicy: {
    kind: "confirmation_target_blocks",
    blocks: 6,
  },
};
