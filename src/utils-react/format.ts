import { fxRates } from "../constants";
import type { BaseCurrency } from "../types";

export const formatProbabilitySats = (
  price: number,
  fullContract: number,
): string => `${Math.round(price * fullContract)} sats`;
export const formatProbabilityWithPercent = (
  price: number,
  fullContract: number,
): string =>
  `${Math.round(price * 100)}% (${formatProbabilitySats(price, fullContract)})`;
export const formatPercent = (value: number): string =>
  `${value >= 0 ? "+" : ""}${value.toFixed(1)}%`;
export const formatSats = (value: number): string =>
  `${value.toLocaleString()} sats`;
export const formatSatsInput = (value: number): string =>
  Math.max(1, Math.floor(value)).toLocaleString("en-US");
export const formatVolumeBtc = (value: number): string =>
  value >= 1000
    ? `${(value / 1000).toFixed(1)}K BTC`
    : `${value.toFixed(1)} BTC`;
export const formatBlockHeight = (value: number): string =>
  value.toLocaleString("en-US");

export function formatTimeRemaining(blocksLeft: number): string {
  if (blocksLeft <= 0) return "Expired";
  const mins = blocksLeft;
  if (mins < 60) return `${mins}m left`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h left`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d left`;
  const months = Math.floor(days / 30);
  return `${months}mo left`;
}

const _dateFmtCache = new Map<string, Intl.DateTimeFormat>();
const _numFmtCache = new Map<string, Intl.NumberFormat>();
export function cachedDateFmt(
  key: string,
  locale: string,
  opts: Intl.DateTimeFormatOptions,
): Intl.DateTimeFormat {
  let f = _dateFmtCache.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat(locale, opts);
    _dateFmtCache.set(key, f);
  }
  return f;
}
export function cachedNumFmt(
  key: string,
  locale: string,
  opts: Intl.NumberFormatOptions,
): Intl.NumberFormat {
  let f = _numFmtCache.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, opts);
    _numFmtCache.set(key, f);
  }
  return f;
}

export const formatEstTime = (date: Date): string =>
  cachedDateFmt("est-time", "en-US", {
    timeZone: "America/New_York",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  })
    .format(date)
    .toLowerCase();

export const formatSettlementDateTime = (date: Date): string =>
  `${cachedDateFmt("settlement", "en-US", {
    timeZone: "America/New_York",
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  }).format(date)} ET`;

export function satsToFiat(sats: number, currency: BaseCurrency): number {
  const btcValue = sats / 100_000_000;
  const usdValue = btcValue * fxRates.BTC;
  return usdValue / fxRates[currency];
}

export function formatFiat(value: number, currency: BaseCurrency): string {
  switch (currency) {
    case "USD":
      return cachedNumFmt("USD", "en-US", {
        style: "currency",
        currency: "USD",
      }).format(value);
    case "EUR":
      return cachedNumFmt("EUR", "de-DE", {
        style: "currency",
        currency: "EUR",
      }).format(value);
    case "GBP":
      return cachedNumFmt("GBP", "en-GB", {
        style: "currency",
        currency: "GBP",
      }).format(value);
    case "JPY":
      return cachedNumFmt("JPY", "ja-JP", {
        style: "currency",
        currency: "JPY",
        maximumFractionDigits: 0,
      }).format(value);
    case "CNY":
      return cachedNumFmt("CNY", "zh-CN", {
        style: "currency",
        currency: "CNY",
      }).format(value);
    case "CHF":
      return cachedNumFmt("CHF", "de-CH", {
        style: "currency",
        currency: "CHF",
      }).format(value);
    case "AUD":
      return cachedNumFmt("AUD", "en-AU", {
        style: "currency",
        currency: "AUD",
      }).format(value);
    case "CAD":
      return cachedNumFmt("CAD", "en-CA", {
        style: "currency",
        currency: "CAD",
      }).format(value);
    default:
      return "";
  }
}

/** Accepts baseCurrency as param instead of reading from global state. */
export function satsToFiatStr(sats: number, baseCurrency: BaseCurrency): string {
  if (baseCurrency === "BTC") return "";
  return formatFiat(satsToFiat(sats, baseCurrency), baseCurrency);
}
