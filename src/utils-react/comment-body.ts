/**
 * Tokenizes a body of text (comment, profile about, etc.) into a mix
 * of plain-text, URL, and Nostr-URI segments. URLs are NEVER
 * auto-linked — the renderer wraps them in a button that opens a
 * confirm dialog before leaving the app. Nostr URIs referencing a
 * pubkey (`nostr:npub1…` or `nostr:nprofile1…`) resolve to a mention
 * chip showing the recipient's display name once their kind:0 is
 * fetched; other Nostr URI forms (nevent, note, naddr) render as a
 * neutral chip with a truncated identifier for now.
 */

import { nprofileToHex, npubToHex } from "../utils/crypto";

export type NostrRefKind = "npub" | "nprofile" | "nevent" | "note" | "naddr";

export type BodySegment =
  | { kind: "text"; value: string }
  | { kind: "url"; value: string }
  | {
      kind: "nostrMention";
      /** Original `nostr:npub1…` or `nostr:nprofile1…` source, with the
       *  `nostr:` prefix included. Useful for tooltips. */
      raw: string;
      /** 64-char hex pubkey resolved from the bech32 payload. */
      pubkeyHex: string;
    }
  | {
      kind: "nostrEntity";
      /** Original `nostr:…` source including prefix. */
      raw: string;
      /** Bech32 entity (e.g. `nevent1…`). */
      entity: string;
      subtype: NostrRefKind;
    };

// Matches http(s) URLs. Stops at whitespace or a handful of characters
// that would obviously not be part of a URL (quotes, angle brackets,
// backticks). Trailing punctuation like a period or comma often *looks*
// like part of a URL — we trim those below so a sentence ending in a
// URL doesn't render as "https://example.com." with the period inside
// the clickable range.
const URL_RE = /(https?:\/\/[^\s<>"'`]+)/gi;
// Matches `nostr:<bech32-entity>` where the entity starts with one of
// the known NIP-19 prefixes followed by the bech32 separator and the
// payload. Case-insensitive because npub/NPUB are both valid.
const NOSTR_URI_RE = /nostr:((?:npub|nprofile|nevent|note|naddr)1[0-9a-z]+)/gi;
const TRAILING_PUNCT = /[.,;:!?)\]}]+$/;

type Hit = {
  start: number;
  end: number;
  build: () => BodySegment[];
};

function urlHit(match: RegExpMatchArray): Hit {
  const raw = match[0];
  const start = match.index ?? 0;
  return {
    start,
    end: start + raw.length,
    build: () => {
      let url = raw;
      const trailMatch = url.match(TRAILING_PUNCT);
      let trailing = "";
      if (trailMatch) {
        trailing = trailMatch[0];
        url = url.slice(0, url.length - trailing.length);
      }
      const out: BodySegment[] = [{ kind: "url", value: url }];
      if (trailing) out.push({ kind: "text", value: trailing });
      return out;
    },
  };
}

function nostrHit(match: RegExpMatchArray): Hit | null {
  const raw = match[0];
  const entity = match[1];
  const start = match.index ?? 0;
  const subtype = entity.slice(0, entity.indexOf("1")) as NostrRefKind;
  let pubkeyHex: string | null = null;
  if (subtype === "npub") pubkeyHex = npubToHex(entity);
  else if (subtype === "nprofile") pubkeyHex = nprofileToHex(entity);

  return {
    start,
    end: start + raw.length,
    build: () => {
      if (pubkeyHex) {
        return [{ kind: "nostrMention", raw, pubkeyHex }];
      }
      // nevent/note/naddr (or malformed npub/nprofile) — fall back to
      // a neutral entity chip. Renderer shows a truncated identifier.
      return [{ kind: "nostrEntity", raw, entity, subtype }];
    },
  };
}

function collectHits(text: string): Hit[] {
  const hits: Hit[] = [];
  for (const m of text.matchAll(URL_RE)) hits.push(urlHit(m));
  for (const m of text.matchAll(NOSTR_URI_RE)) {
    const h = nostrHit(m);
    if (h) hits.push(h);
  }
  hits.sort((a, b) => a.start - b.start);
  // Drop overlaps (shouldn't happen with current patterns, but cheap
  // insurance if someone's bio contains `https://nostr:npub1…`).
  const merged: Hit[] = [];
  let cursor = 0;
  for (const h of hits) {
    if (h.start >= cursor) {
      merged.push(h);
      cursor = h.end;
    }
  }
  return merged;
}

export function parseCommentBody(text: string): BodySegment[] {
  if (!text) return [];
  const hits = collectHits(text);
  const segments: BodySegment[] = [];
  let lastIndex = 0;
  for (const hit of hits) {
    if (hit.start > lastIndex) {
      segments.push({ kind: "text", value: text.slice(lastIndex, hit.start) });
    }
    segments.push(...hit.build());
    lastIndex = hit.end;
  }
  if (lastIndex < text.length) {
    segments.push({ kind: "text", value: text.slice(lastIndex) });
  }
  return segments;
}
