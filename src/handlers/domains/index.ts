import type { ActionDomain } from "../../actions.ts";
import { handleAppDomain } from "./app.ts";
import type { ClickDomainContext } from "./context.ts";
import { handleMarketDomain } from "./market.ts";
import { handleOnboardingDomain } from "./onboarding.ts";
import { handleWalletDomain } from "./wallet.ts";

type DomainHandler = (ctx: ClickDomainContext) => Promise<void>;

const DOMAIN_HANDLERS: Record<ActionDomain, DomainHandler> = {
  onboarding: handleOnboardingDomain,
  app: handleAppDomain,
  wallet: handleWalletDomain,
  market: handleMarketDomain,
};

const DOMAIN_HANDLER_ORDER: ActionDomain[] = [
  "onboarding",
  "app",
  "wallet",
  "market",
];

export async function dispatchDomainAction(
  ctx: ClickDomainContext,
): Promise<void> {
  if (ctx.actionDomain) {
    await DOMAIN_HANDLERS[ctx.actionDomain](ctx);
    return;
  }

  for (const domain of DOMAIN_HANDLER_ORDER) {
    await DOMAIN_HANDLERS[domain](ctx);
  }
}

export type { ClickDomainContext } from "./context.ts";
