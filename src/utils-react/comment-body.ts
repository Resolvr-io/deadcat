/**
 * Tokenizes a comment body into a mix of plain-text and URL segments.
 * URLs are NEVER auto-linked — the renderer wraps them in a button that
 * opens a confirm dialog before leaving the app.
 */

export type BodySegment =
  | { kind: "text"; value: string }
  | { kind: "url"; value: string };

// Matches http(s) URLs. Stops at whitespace or a handful of characters
// that would obviously not be part of a URL (quotes, angle brackets,
// backticks). Trailing punctuation like a period or comma often *looks*
// like part of a URL — we trim those below so a sentence ending in a
// URL doesn't render as "https://example.com." with the period inside
// the clickable range.
const URL_RE = /(https?:\/\/[^\s<>"'`]+)/gi;
const TRAILING_PUNCT = /[.,;:!?)\]}]+$/;

export function parseCommentBody(text: string): BodySegment[] {
  if (!text) return [];
  const segments: BodySegment[] = [];
  let lastIndex = 0;
  for (const match of text.matchAll(URL_RE)) {
    const start = match.index ?? 0;
    // Flush preceding plain text.
    if (start > lastIndex) {
      segments.push({ kind: "text", value: text.slice(lastIndex, start) });
    }
    let url = match[0];
    // Trim trailing punctuation that almost certainly isn't part of the
    // URL. Push the trimmed punctuation back onto the text stream.
    const trailMatch = url.match(TRAILING_PUNCT);
    let trailing = "";
    if (trailMatch) {
      trailing = trailMatch[0];
      url = url.slice(0, url.length - trailing.length);
    }
    segments.push({ kind: "url", value: url });
    if (trailing) {
      segments.push({ kind: "text", value: trailing });
    }
    lastIndex = start + match[0].length;
  }
  if (lastIndex < text.length) {
    segments.push({ kind: "text", value: text.slice(lastIndex) });
  }
  return segments;
}
