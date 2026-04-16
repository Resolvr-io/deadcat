import type { BaseCurrency, NavCategory } from "./types";

export const EXECUTION_FEE_RATE = 0.01;
export const WIN_FEE_RATE = 0.02;
export const SATS_PER_FULL_CONTRACT = 2; // multiplied by cptSats at runtime

export const categories: NavCategory[] = [
  "Trending",
  "New",
  "Portfolio",
  "Politics",
  "Sports",
  "Culture",
  "Bitcoin",
  "Weather",
  "Macro",
  "Ending Soon",
  "Resolved",
  "My Markets",
];

export const baseCurrencyOptions: BaseCurrency[] = [
  "BTC",
  "USD",
  "EUR",
  "JPY",
  "GBP",
  "CNY",
  "CHF",
  "AUD",
  "CAD",
];

export const fxRates: Record<BaseCurrency, number> = {
  BTC: 97000,
  USD: 1,
  EUR: 1.08,
  JPY: 0.0067,
  GBP: 1.28,
  CNY: 0.14,
  CHF: 1.12,
  AUD: 0.65,
  CAD: 0.74,
};
