const HTML_ESCAPE_MAP: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

const HTML_ESCAPE_RE = /[&<>"']/g;
const ATTR_ESCAPE_RE = /[&<>"'`]/g;

export function escapeHtml(value: unknown): string {
  return String(value).replace(HTML_ESCAPE_RE, (ch) => HTML_ESCAPE_MAP[ch]);
}

export function escapeAttr(value: unknown): string {
  return String(value)
    .replace(ATTR_ESCAPE_RE, (ch) => {
      if (ch === "`") return "&#96;";
      return HTML_ESCAPE_MAP[ch] ?? ch;
    })
    .replace(/\n/g, "&#10;")
    .replace(/\r/g, "&#13;");
}
