// Mirrors the escaping rule of auth/src/shared/auth/typedDataReview.ts: how a signed string may be
// shown. Only the display changes -- the request is never rewritten, so the bytes the wallet signs
// are exactly the bytes that arrived.

// Characters a rendered string cannot show faithfully: controls, format characters such as the bidi
// overrides that reorder their neighbours, every separator but the ordinary space (a no-break or
// figure space looks identical to one and signs differently), surrogates, private-use and
// unassigned code points, and U+FFFD (what decoding invalid UTF-8 produces). The backslash is
// escaped too, so a signed string cannot spell out an escape of its own and the display stays
// unambiguous.
const UNREADABLE_CHARACTER_RE = /(?! )[\\\p{C}\p{Z}\uFFFD]/gu;
const SHORT_ESCAPES: ReadonlyMap<string, string> = new Map([
  ["\\", "\\\\"],
  ["\t", "\\t"],
  ["\n", "\\n"],
  ["\r", "\\r"],
]);

export function escapeUnreadableTypedDataText(text: string): string {
  return text.replace(
    UNREADABLE_CHARACTER_RE,
    (character) =>
      SHORT_ESCAPES.get(character) ?? `\\u{${(character.codePointAt(0) ?? 0).toString(16)}}`,
  );
}

// A field or type name is signed text like any other, so a key is escaped the way its value is.
// Escaping the backslash keeps the mapping one to one, so two names can never collapse into one.
export function sanitizeTypedDataForDisplay(value: unknown): unknown {
  if (typeof value === "string") return escapeUnreadableTypedDataText(value);
  if (Array.isArray(value)) return value.map(sanitizeTypedDataForDisplay);
  if (typeof value === "object" && value !== null) {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([key, entry]) => [
        escapeUnreadableTypedDataText(key),
        sanitizeTypedDataForDisplay(entry),
      ]),
    );
  }
  return value;
}

// Escaping expands text, so the bound upstream's review carries (typedDataReview.ts's
// MAX_TOTAL_LENGTH) is measured on the text as it will be shown. It bounds the DISPLAY only: a
// payload past it is still accepted and still signed, because every typed-data request already
// reaches the user behind the "effects cannot be verified" acknowledgment.
export const MAX_DISPLAYED_TYPED_DATA_CHARS = 512 * 1024;

const TRUNCATION_NOTICE =
  "\n... (truncated; this page cannot show the whole payload -- your wallet shows what it signs)";

export function truncateForDisplay(text: string): string {
  if (text.length <= MAX_DISPLAYED_TYPED_DATA_CHARS) return text;
  return text.slice(0, MAX_DISPLAYED_TYPED_DATA_CHARS) + TRUNCATION_NOTICE;
}
