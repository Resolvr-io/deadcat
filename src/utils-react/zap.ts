/**
 * NIP-57 zap client.
 *
 * Given a recipient's profile (lud16 or lud06), this module resolves
 * the LNURL-pay endpoint, asks the backend to build and sign a
 * kind:9734 zap request, fetches a Lightning invoice from the
 * recipient's callback, and pays it via Bitcoin Connect's WebLN
 * provider (with `launchPaymentModal` as a fallback).
 *
 * The backend is responsible for signing — see the `sign_zap_request`
 * Tauri command. This keeps all NostrSigner interaction (local nsec
 * and NIP-46 remote signer) on the Rust side.
 */

import {
  launchPaymentModal,
  requestProvider,
} from "@getalby/bitcoin-connect-react";
import { invoke } from "@tauri-apps/api/core";
import type { NostrProfile } from "../types";

export type LnurlPayResponse = {
  callback: string;
  minSendable: number;
  maxSendable: number;
  allowsNostr?: boolean;
  nostrPubkey?: string;
  commentAllowed?: number;
  tag?: string;
};

export type LnurlCallbackResponse = {
  pr: string;
  verify?: string;
  routes?: unknown[];
};

/**
 * Resolve an `lud16` or `lud06` identifier into the LNURL-pay endpoint
 * URL. Returns `null` if the profile has neither set.
 */
export function resolveLnurlEndpoint(profile: NostrProfile): string | null {
  if (profile.lud16) {
    const [name, domain] = profile.lud16.split("@");
    if (!name || !domain) return null;
    return `https://${domain}/.well-known/lnurlp/${name}`;
  }
  // lud06 is bech32-encoded; if needed we'd decode it here. Most
  // modern profiles use lud16, so punt on lud06 for now.
  return null;
}

/** Fetch the LNURL-pay endpoint and return its metadata. */
export async function fetchLnurlPay(
  endpoint: string,
): Promise<LnurlPayResponse> {
  const res = await fetch(endpoint);
  if (!res.ok) {
    throw new Error(`LNURL fetch failed (${res.status})`);
  }
  const json = (await res.json()) as LnurlPayResponse;
  if (json.tag && json.tag !== "payRequest") {
    throw new Error(`Unexpected LNURL tag: ${json.tag}`);
  }
  return json;
}

/**
 * POST the signed zap request to the LNURL callback and return the
 * Lightning invoice. NIP-57 specifies GET here; browsers/WKWebView
 * don't support POST bodies on GET. Most receivers accept either.
 */
export async function fetchZapInvoice(args: {
  callback: string;
  amountMsats: number;
  signedZapRequestJson: string;
  lnurl?: string;
}): Promise<LnurlCallbackResponse> {
  const params = new URLSearchParams({
    amount: String(args.amountMsats),
    nostr: args.signedZapRequestJson,
  });
  if (args.lnurl) params.set("lnurl", args.lnurl);
  const url = `${args.callback}${args.callback.includes("?") ? "&" : "?"}${params.toString()}`;
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`LNURL callback failed (${res.status})`);
  }
  const json = (await res.json()) as LnurlCallbackResponse & {
    status?: string;
    reason?: string;
  };
  if (json.status === "ERROR") {
    throw new Error(json.reason || "LNURL callback returned an error");
  }
  if (!json.pr) {
    throw new Error("LNURL callback did not return an invoice");
  }
  return json;
}

/** Call the backend to build and sign a kind:9734 zap request. */
async function signZapRequest(args: {
  recipientPubkeyHex: string;
  amountMsats: number;
  relays: string[];
  content: string;
  lnurl?: string;
  eventIdHex?: string;
  eventCoordinate?: string;
}): Promise<string> {
  return invoke<string>("sign_zap_request", {
    recipientPubkeyHex: args.recipientPubkeyHex,
    amountMsats: args.amountMsats,
    relays: args.relays,
    content: args.content,
    lnurl: args.lnurl ?? null,
    eventIdHex: args.eventIdHex ?? null,
    eventCoordinate: args.eventCoordinate ?? null,
  });
}

export type ZapResult = {
  preimage?: string;
  /** True when the payment was completed via Bitcoin Connect's
   *  internal provider; false when the user had to confirm via the
   *  fallback payment modal. */
  paidViaProvider: boolean;
  /** True when the payment modal closed without a definitive
   *  paid/cancelled signal — e.g. the user closed the QR-flow modal
   *  after paying externally. BC has no callback for external
   *  payments, so we can't distinguish "cancelled" from "paid in
   *  another wallet". The caller should treat this as a non-error
   *  outcome, surface a non-committal message, and rely on
   *  kind:9735 receipt aggregation to confirm the zap landed. */
  pending?: boolean;
};

/**
 * End-to-end zap: resolve LNURL → sign zap request → fetch invoice →
 * pay via BC provider (with launchPaymentModal fallback).
 */
export async function sendZap(args: {
  recipientProfile: NostrProfile;
  recipientPubkeyHex: string;
  amountSats: number;
  content: string;
  relays: string[];
  /** Hex event id, when zapping a specific event (e.g. comment). */
  eventIdHex?: string;
  /** Addressable event coordinate (kind:pubkey:d), when zapping an
   *  addressable event (e.g. a market). */
  eventCoordinate?: string;
}): Promise<ZapResult> {
  const endpoint = resolveLnurlEndpoint(args.recipientProfile);
  if (!endpoint) {
    throw new Error(
      "Recipient has no Lightning address (lud16) in their profile",
    );
  }

  const lnurlMeta = await fetchLnurlPay(endpoint);
  if (!lnurlMeta.allowsNostr) {
    throw new Error("Recipient's Lightning address doesn't support Nostr zaps");
  }

  const amountMsats = args.amountSats * 1000;
  if (amountMsats < lnurlMeta.minSendable) {
    throw new Error(
      `Amount below recipient minimum (${Math.ceil(lnurlMeta.minSendable / 1000)} sats)`,
    );
  }
  if (amountMsats > lnurlMeta.maxSendable) {
    throw new Error(
      `Amount above recipient maximum (${Math.floor(lnurlMeta.maxSendable / 1000)} sats)`,
    );
  }

  const signedJson = await signZapRequest({
    recipientPubkeyHex: args.recipientPubkeyHex,
    amountMsats,
    relays: args.relays,
    content: args.content,
    eventIdHex: args.eventIdHex,
    eventCoordinate: args.eventCoordinate,
  });

  const { pr } = await fetchZapInvoice({
    callback: lnurlMeta.callback,
    amountMsats,
    signedZapRequestJson: signedJson,
  });

  // Prefer paying via BC's connected WebLN provider so the payment is
  // silent. Fall back to the BC payment modal if nothing is connected.
  try {
    const provider = await requestProvider();
    const result = (await provider.sendPayment(pr)) as {
      preimage?: string;
    };
    return { preimage: result?.preimage, paidViaProvider: true };
  } catch {
    return new Promise<ZapResult>((resolve) => {
      const { setPaid } = launchPaymentModal({
        invoice: pr,
        onPaid: ({ preimage }) => {
          resolve({ preimage, paidViaProvider: false });
        },
        // BC's `onCancelled` fires both when the user truly cancels
        // AND when they close the modal after paying in an external
        // wallet — BC has no way to detect off-device settlement.
        // Resolve with `pending: true` instead of rejecting so the
        // caller can surface a non-committal "if you paid, it'll
        // appear shortly" message and rely on kind:9735 aggregation
        // to confirm.
        onCancelled: () => {
          resolve({ paidViaProvider: false, pending: true });
        },
      });
      // Keep setPaid around for callers that want to finalize the
      // modal manually after verifying via LNURL /verify or a
      // kind:9735 subscription.
      void setPaid;
    });
  }
}
