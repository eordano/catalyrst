
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

export const MAX_DISPLAYED_TYPED_DATA_CHARS = 512 * 1024;

const TRUNCATION_NOTICE =
  "\n... (truncated; this page cannot show the whole payload -- your wallet shows what it signs)";

export function truncateForDisplay(text: string): string {
  if (text.length <= MAX_DISPLAYED_TYPED_DATA_CHARS) return text;
  return text.slice(0, MAX_DISPLAYED_TYPED_DATA_CHARS) + TRUNCATION_NOTICE;
}
