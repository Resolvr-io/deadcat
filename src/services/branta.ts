const BRANTA_PRODUCTION_URL = "https://guardrail.branta.pro";
const BRANTA_STAGING_URL = "https://staging.guardrail.branta.pro";

export type BrantaVerifyStatus = "idle" | "checking" | "verified" | "not_found";

export type BrantaVerifyResult = {
  found: boolean;
  platform?: string;
  description?: string;
};

export async function verifyAddress(
  address: string,
  isMainnet: boolean,
): Promise<BrantaVerifyResult> {
  const baseUrl = isMainnet ? BRANTA_PRODUCTION_URL : BRANTA_STAGING_URL;
  try {
    const resp = await fetch(
      `${baseUrl}/v2/payments/${encodeURIComponent(address)}`,
      { headers: { Accept: "application/json" } },
    );
    if (!resp.ok) return { found: false };
    const payments = (await resp.json()) as Array<{
      destinations?: unknown[];
      platform?: string;
      description?: string;
    }>;
    if (!Array.isArray(payments) || payments.length === 0) return { found: false };
    const first = payments[0];
    return {
      found: true,
      platform: first.platform,
      description: first.description,
    };
  } catch {
    return { found: false };
  }
}

// Debounce helper: returns a function that delays calling `fn` until `ms` ms
// after the last invocation. Cancels any pending call when called again.
export function debounce<T extends unknown[]>(
  fn: (...args: T) => void,
  ms: number,
): (...args: T) => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  return (...args: T) => {
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, ms);
  };
}
