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

/** Reverse byte-order of a hex string (internal ↔ display order for hash-based IDs). */
export function reverseHex(hex: string): string {
  return (hex.match(/.{2}/g) || []).reverse().join("");
}
