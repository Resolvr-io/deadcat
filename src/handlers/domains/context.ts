import type { Action, ActionDomain } from "../../actions.ts";
import type {
  ActionTab,
  OrderType,
  Side,
  SizeMode,
  TradeIntent,
} from "../../types.ts";

export type ClickDomainContext = {
  target: HTMLElement;
  actionEl: HTMLElement | null;
  action: Action | null;
  actionDomain: ActionDomain | null;
  side: Side | null;
  tradeChoiceRaw: string | null;
  tradeIntent: TradeIntent | null;
  sizeMode: SizeMode | null;
  tradeSizePreset: number;
  tradeSizeDelta: number;
  limitPriceDelta: number;
  contractsStepDelta: number;
  orderType: OrderType | null;
  tab: ActionTab | null;
  render: () => void;
  finishOnboarding: () => Promise<void>;
};
