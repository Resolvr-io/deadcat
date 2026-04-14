import { createAvatar } from "@dicebear/core";
import { thumbs } from "@dicebear/collection";

const BG_COLORS = [
  "1e3a5f", "3b1f5e", "1f4e3d", "4a1f2e", "2d3a1f",
  "1f2d4a", "4a3b1f", "1f4a3b", "3a1f4a", "4e2d1f",
];

const SHAPE_COLORS = [
  "34d399", "60a5fa", "f472b6", "fb923c", "a78bfa",
  "facc15", "38bdf8", "4ade80", "f87171", "e879f9",
];

/** Deterministic index from seed string. */
function seedHash(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) {
    h = (Math.imul(31, h) + seed.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

/** Generate a deterministic DiceBear SVG avatar from a seed (npub or name). */
export function generateAvatarSvg(seed: string): string {
  const h = seedHash(seed);
  const bg = BG_COLORS[h % BG_COLORS.length];
  const shape = SHAPE_COLORS[h % SHAPE_COLORS.length];
  return createAvatar(thumbs, {
    seed,
    size: 80,
    backgroundColor: [bg],
    shapeColor: [shape],
    eyesColor: [bg],
    mouthColor: [bg],
  }).toString();
}

/** Inline SVG as a data URI for use in img src. */
export function generateAvatarDataUri(seed: string): string {
  const svg = generateAvatarSvg(seed);
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}
