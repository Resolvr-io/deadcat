/** Map raw SDK/API error strings to user-friendly messages. */
export function friendlyError(raw: string): string {
  const lower = raw.toLowerCase();
  // NIP-46 signing — remote signer didn't respond or refused.
  if (lower.includes("sign_event") && lower.includes("timeout"))
    return "Your signer didn't respond. Open your signer app to approve the request and try again.";
  if (
    lower.includes("sign_event failed") ||
    lower.includes("nip44_encrypt failed") ||
    lower.includes("nip04_encrypt failed")
  )
    return "Signing failed. Open your signer app and try again.";
  if (lower.includes("get_public_key failed"))
    return "Couldn't reach your signer. Make sure it's online and try again.";
  if (
    lower.includes("insufficient funds") ||
    (lower.includes("missing") && lower.includes("units for asset"))
  )
    return "Insufficient funds to cover the amount and network fee.";
  if (lower.includes("no liquidity"))
    return "No liquidity available for this market yet.";
  if (lower.includes("insufficient taker funding"))
    return "Insufficient funds to complete this trade.";
  if (lower.includes("insufficient utxo"))
    return "Not enough wallet outputs. Try syncing your wallet.";
  if (lower.includes("insufficient fee"))
    return "Not enough funds to cover the network fee.";
  if (lower.includes("timed out") || lower.includes("timeout"))
    return "Request timed out. Please try again.";
  if (lower.includes("wallet locked") || lower.includes("wallet is locked"))
    return "Wallet is locked. Unlock it first.";
  if (lower.includes("expected value at line"))
    return "Service temporarily unavailable. Please try again later.";
  if (lower.includes("boltz api error"))
    return "Swap service error. Please try again later.";
  return raw;
}
