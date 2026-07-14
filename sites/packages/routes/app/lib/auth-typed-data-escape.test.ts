import { describe, expect, it } from "vitest";

import {
  MAX_DISPLAYED_TYPED_DATA_CHARS,
  escapeUnreadableTypedDataText,
  sanitizeTypedDataForDisplay,
  truncateForDisplay,
} from "./auth-typed-data-escape";

// The strings auth/src/components/Pages/RequestPage/Views/typedDataDisplay.spec.ts holds the
// display to, with the escapes it expects: a character that acts on the display is shown, never
// obeyed. The escape spelling differs -- upstream prints one \\uXXXX per UTF-16 unit, this page
// prints the code point as \\u{...} -- and the backslash is escaped here, so a signed string cannot
// spell an escape of its own.
describe("escapeUnreadableTypedDataText", () => {
  it.each([
    [
      "an override that would reorder its neighbours",
      "Send 1 MANA to \u{202e}0xattacker",
      "Send 1 MANA to \\u{202e}0xattacker",
    ],
    [
      "line breaks, tabs and backslashes",
      "allow\n\nall\tnow\r\\done",
      "allow\\n\\nall\\tnow\\r\\\\done",
    ],
    ["a zero-width space", "1\u{200b}", "1\\u{200b}"],
    ["a byte-order mark", "\u{feff}Decentraland", "\\u{feff}Decentraland"],
    ["what decoding invalid UTF-8 produces", "MANA\u{fffd}", "MANA\\u{fffd}"],
    ["a paragraph separator", "one\u{2029}two", "one\\u{2029}two"],
    ["a private-use code point", "\u{f0000}", "\\u{f0000}"],
  ])("shows %s as a visible escape", (_label, text, expected) => {
    expect(escapeUnreadableTypedDataText(text)).toBe(expected);
  });

  it("leaves readable text, the ordinary space included, exactly as it was signed", () => {
    const readable = "Send 1 MANA to 0xdead - approve until 2026/12/31 (fee 0.5%)";
    expect(escapeUnreadableTypedDataText(readable)).toBe(readable);
    expect(escapeUnreadableTypedDataText("")).toBe("");
  });

  // Dropped from the class in auth #489 (shared/text.ts): a no-break, figure, narrow or ideographic
  // space is ordinary text in the languages that use it, and the classification rule this page gates
  // on (isOpaqueSignatureMessage) already reads it that way. Escaping it here would have made this
  // page disagree with its own gate.
  it("leaves spaces that are not the ordinary space as they were signed", () => {
    for (const spaced of [
      "Send\u{a0}1 MANA to\u{2007}you",
      "Trusted\u{202f}App",
      "Decentraland\u{3000}",
    ]) {
      expect(escapeUnreadableTypedDataText(spaced)).toBe(spaced);
    }
  });

  it("escapes every occurrence, not just the first", () => {
    expect(escapeUnreadableTypedDataText("\u{202e}a\u{202e}b\u{202e}")).toBe(
      "\\u{202e}a\\u{202e}b\\u{202e}",
    );
  });
});

describe("sanitizeTypedDataForDisplay", () => {
  it("escapes field names the way it escapes the text they carry", () => {
    expect(sanitizeTypedDataForDisplay({ "spen\u{202e}der": "0x\u{200b}dead" })).toEqual({
      "spen\\u{202e}der": "0x\\u{200b}dead",
    });
  });

  it("walks nested structs and arrays", () => {
    expect(
      sanitizeTypedDataForDisplay({
        checks: { externalChecks: [{ memo: "a\u{202e}b" }, ["c\u{200b}d"]] },
      }),
    ).toEqual({ checks: { externalChecks: [{ memo: "a\\u{202e}b" }, ["c\\u{200b}d"]] } });
  });

  it("leaves everything that is not text untouched", () => {
    expect(
      sanitizeTypedDataForDisplay({ qty: 10, required: true, target: null, missing: undefined }),
    ).toEqual({ qty: 10, required: true, target: null, missing: undefined });
  });

  // The backslash is escaped too, so a string that spells an escape and the character it names can
  // never be shown the same way.
  it("keeps a signed string that spells an escape apart from the character it names", () => {
    expect(sanitizeTypedDataForDisplay({ memo: "\\u{202e}" })).toEqual({
      memo: "\\\\u{202e}",
    });
    expect(sanitizeTypedDataForDisplay({ memo: "\u{202e}" })).toEqual({ memo: "\\u{202e}" });
  });
});

// Escaping expands text: a single override becomes eight characters, and JSON.stringify escapes
// the backslashes again. The cap is measured on the text as it will be shown, and it bounds the
// display only -- nothing here decides whether a request is accepted.
describe("truncateForDisplay", () => {
  it("leaves text at or under the cap byte-identical", () => {
    const justUnder = "a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS - 1);
    expect(truncateForDisplay(justUnder)).toBe(justUnder);
    const exact = "a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS);
    expect(truncateForDisplay(exact)).toBe(exact);
    expect(truncateForDisplay("")).toBe("");
  });

  it("cuts text past the cap and says the page is not showing all of it", () => {
    const shown = truncateForDisplay("a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS + 1));
    expect(shown).toContain("truncated");
    expect(shown).toContain("your wallet shows what it signs");
    expect(shown.startsWith("a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS))).toBe(true);
    expect(shown.length).toBeLessThan(MAX_DISPLAYED_TYPED_DATA_CHARS + 200);
  });

  it("bounds a payload that only exceeds the cap once it is escaped", () => {
    const overrides = "\u{202e}".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS / 4);
    expect(overrides.length).toBeLessThan(MAX_DISPLAYED_TYPED_DATA_CHARS);
    const escaped = escapeUnreadableTypedDataText(overrides);
    expect(escaped.length).toBeGreaterThan(MAX_DISPLAYED_TYPED_DATA_CHARS);
    expect(truncateForDisplay(escaped).length).toBeLessThan(MAX_DISPLAYED_TYPED_DATA_CHARS + 200);
  });
});
