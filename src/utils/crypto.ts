export function hexToBytes(hex: string): number[] {
  const bytes: number[] = [];
  for (let i = 0; i < hex.length; i += 2) {
    bytes.push(parseInt(hex.substring(i, i + 2), 16));
  }
  return bytes;
}

const BECH32_CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
function bech32Polymod(values: number[]): number {
  const GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
  let chk = 1;
  for (const v of values) {
    const b = chk >> 25;
    chk = ((chk & 0x1ffffff) << 5) ^ v;
    for (let i = 0; i < 5; i++) if ((b >> i) & 1) chk ^= GEN[i];
  }
  return chk;
}
function bech32Encode(hrp: string, data5bit: number[]): string {
  const hrpExpand = [...hrp]
    .map((c) => c.charCodeAt(0) >> 5)
    .concat([0])
    .concat([...hrp].map((c) => c.charCodeAt(0) & 31));
  const values = hrpExpand.concat(data5bit);
  const polymod = bech32Polymod(values.concat([0, 0, 0, 0, 0, 0])) ^ 1;
  const checksum = Array.from(
    { length: 6 },
    (_, i) => (polymod >> (5 * (5 - i))) & 31,
  );
  return `${hrp}1${data5bit
    .concat(checksum)
    .map((d) => BECH32_CHARSET[d])
    .join("")}`;
}
function convertBits(
  data: number[],
  fromBits: number,
  toBits: number,
  pad: boolean,
): number[] {
  let acc = 0,
    bits = 0;
  const ret: number[] = [];
  const maxv = (1 << toBits) - 1;
  for (const value of data) {
    acc = (acc << fromBits) | value;
    bits += fromBits;
    while (bits >= toBits) {
      bits -= toBits;
      ret.push((acc >> bits) & maxv);
    }
  }
  if (pad && bits > 0) ret.push((acc << (toBits - bits)) & maxv);
  return ret;
}
export function hexToNpub(hex: string): string {
  return bech32Encode("npub", convertBits(hexToBytes(hex), 8, 5, true));
}

/**
 * Decode a bech32 string into its human-readable part + 5-bit data
 * payload, verifying the checksum. Returns null when the input is
 * malformed or the checksum doesn't match.
 */
export function bech32Decode(
  encoded: string,
): { hrp: string; data: number[] } | null {
  const lower = encoded.toLowerCase();
  const sep = lower.lastIndexOf("1");
  if (sep < 1 || sep + 7 > lower.length) return null;
  const hrp = lower.slice(0, sep);
  const dataPart = lower.slice(sep + 1);
  const data: number[] = [];
  for (const ch of dataPart) {
    const idx = BECH32_CHARSET.indexOf(ch);
    if (idx < 0) return null;
    data.push(idx);
  }
  const hrpExpand = [...hrp]
    .map((c) => c.charCodeAt(0) >> 5)
    .concat([0])
    .concat([...hrp].map((c) => c.charCodeAt(0) & 31));
  if (bech32Polymod(hrpExpand.concat(data)) !== 1) return null;
  return { hrp, data: data.slice(0, data.length - 6) };
}

/**
 * Decode an `npub1…` bech32 pubkey into 64-char lowercase hex.
 * Returns null when the input is not a valid npub.
 */
export function npubToHex(npub: string): string | null {
  const decoded = bech32Decode(npub);
  if (!decoded || decoded.hrp !== "npub") return null;
  const bytes = convertBits(decoded.data, 5, 8, false);
  if (bytes.length !== 32) return null;
  return bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Decode an `nprofile1…` bech32 TLV — the pubkey lives in TLV entry
 * type 0 (length 32). Relay hints (type 1) are ignored. Returns the
 * 64-char hex pubkey or null if the payload is malformed.
 */
export function nprofileToHex(nprofile: string): string | null {
  const decoded = bech32Decode(nprofile);
  if (!decoded || decoded.hrp !== "nprofile") return null;
  const bytes = convertBits(decoded.data, 5, 8, false);
  // TLV: type(1) length(1) value(length)
  let i = 0;
  while (i + 2 <= bytes.length) {
    const type = bytes[i];
    const len = bytes[i + 1];
    if (i + 2 + len > bytes.length) return null;
    if (type === 0 && len === 32) {
      return bytes
        .slice(i + 2, i + 2 + 32)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
    }
    i += 2 + len;
  }
  return null;
}

/** Reverse byte-order of a hex string (internal ↔ display order for hash-based IDs). */
export function reverseHex(hex: string): string {
  return (hex.match(/.{2}/g) || []).reverse().join("");
}
