// Mirrors the escaping rule of auth/src/shared/text.ts (revealHiddenCharacters, what replaced
// typedDataReview.ts in #489): how a signed string may be shown. Only the display changes -- the
// request is never rewritten, so the bytes the wallet signs are exactly the bytes that arrived.

// The characters a reader cannot see, or that lay out the text around them: controls, format
// characters such as the bidi overrides that reorder their neighbours, surrogates, private-use and
// unassigned code points, the line and paragraph separators, and U+FFFD (what decoding invalid
// UTF-8 produces). Space separators are deliberately NOT here: a no-break or figure space is
// ordinary text in the languages that use it, and the classification rule this page gates on reads
// it the same way (see UNREADABLE_CHARACTER_RE in auth-request-params.ts). The backslash is escaped
// too, so a signed string cannot spell out an escape of its own and the display stays unambiguous.
const UNREADABLE_CHARACTER_RE = /(?! )[\\\p{C}\p{Zl}\p{Zp}\uFFFD]/gu;
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

// Escaping expands text, so this bound is measured on the text as it will be shown. Upstream moved
// its own bound off the display and onto validation in #489 (MAX_SIGNATURE_PAYLOAD_CHARS in
// signMethodGuard.ts, ported as the same constant in auth-request-params.ts), which refuses an
// oversized payload before anything parses it. This stays as the display backstop behind that
// reject: escaping a payload at the cap still multiplies its length, and a param that never reached
// the signature guard (a transaction preview, an unparsable string) is bounded by nothing else.
export const MAX_DISPLAYED_TYPED_DATA_CHARS = 512 * 1024;

const TRUNCATION_NOTICE =
  "\n... (truncated; this page cannot show the whole payload -- your wallet shows what it signs)";

export function truncateForDisplay(text: string): string {
  if (text.length <= MAX_DISPLAYED_TYPED_DATA_CHARS) return text;
  return text.slice(0, MAX_DISPLAYED_TYPED_DATA_CHARS) + TRUNCATION_NOTICE;
}
