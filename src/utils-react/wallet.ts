export function btcLabel(showLbtcLabel: boolean): string {
  return showLbtcLabel ? "L-BTC" : "BTC";
}

export function satsLabel(showLbtcLabel: boolean): string {
  return showLbtcLabel ? "L-sats" : "sats";
}

export function formatLbtc(
  sats: number,
  walletUnit: "sats" | "btc",
  showLbtcLabel: boolean,
): string {
  if (walletUnit === "sats") {
    return `${sats.toLocaleString()} ${satsLabel(showLbtcLabel)}`;
  }
  const btc = sats / 100_000_000;
  return `${btc.toFixed(8)} ${btcLabel(showLbtcLabel)}`;
}

export function formatCompactSats(sats: number): string {
  if (sats >= 1_000_000_000)
    return `${(sats / 1_000_000_000).toFixed(1).replace(/\.0$/, "")} B`;
  if (sats >= 1_000_000)
    return `${(sats / 1_000_000).toFixed(1).replace(/\.0$/, "")} M`;
  if (sats >= 1_000)
    return `${(sats / 1_000).toFixed(1).replace(/\.0$/, "")} K`;
  return sats.toLocaleString();
}

export function flowLabel(flow: string): string {
  switch (flow) {
    case "liquid_to_lightning":
      return "Lightning Send";
    case "lightning_to_liquid":
      return "Lightning Receive";
    case "bitcoin_to_liquid":
      return "Bitcoin Receive";
    case "liquid_to_bitcoin":
      return "Bitcoin Send";
    default:
      return flow;
  }
}

export function formatSwapStatus(status: string): string {
  return status.replace(/[._]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}
