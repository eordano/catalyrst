import { describe, expect, it } from "vitest";

import {
  MAX_DISPLAYED_TYPED_DATA_CHARS,
  escapeUnreadableTypedDataText,
  sanitizeTypedDataForDisplay,
  truncateForDisplay,
} from "./auth-typed-data-escape";

describe("escapeUnreadableTypedDataText", () => {
  it("shows every unreadable code point as a visible escape, every occurrence included", () => {
    const unreadable: [string, string, string][] = [
      [
        "an override that would reorder its neighbours",
        "Send 1 MANA to \u{202e}0xattacker",
        "Send 1 MANA to \\u{202e}0xattacker",
      ],
      [
        "a backslash, so the escapes around it cannot be forged",
        "allow\\u{202e}done",
        "allow\\\\u{202e}done",
      ],
      ["a zero-width space", "1\u{200b}", "1\\u{200b}"],
      ["a byte-order mark", "\u{feff}Decentraland", "\\u{feff}Decentraland"],
      ["what decoding invalid UTF-8 produces", "MANA\u{fffd}", "MANA\\u{fffd}"],
      ["a paragraph separator", "one\u{2029}two", "one\\u{2029}two"],
      ["a private-use code point", "\u{f0000}", "\\u{f0000}"],
      ["every occurrence", "\u{202e}a\u{202e}b\u{202e}", "\\u{202e}a\\u{202e}b\\u{202e}"],
    ];
    for (const [label, text, expected] of unreadable) {
      expect(escapeUnreadableTypedDataText(text), label).toBe(expected);
    }
  });

  it("leaves readable text, its line breaks and tabs, and its uncommon spaces exactly as they were signed", () => {
    for (const signed of [
      "Order\n\ttoken: MANA\r\n\tamount: 1",
      "Send 1 MANA to 0xdead - approve until 2026/12/31 (fee 0.5%)",
      "",
      "Send\u{a0}1 MANA to\u{2007}you",
      "Trusted\u{202f}App",
      "Decentraland\u{3000}",
    ]) {
      expect(escapeUnreadableTypedDataText(signed), JSON.stringify(signed)).toBe(signed);
    }
  });
});

describe("sanitizeTypedDataForDisplay", () => {
  it("escapes field names and nested text alike and leaves everything that is not text untouched", () => {
    expect(sanitizeTypedDataForDisplay({ "spen\u{202e}der": "0x\u{200b}dead" })).toEqual({
      "spen\\u{202e}der": "0x\\u{200b}dead",
    });
    expect(
      sanitizeTypedDataForDisplay({
        checks: { externalChecks: [{ memo: "a\u{202e}b" }, ["c\u{200b}d"]] },
      }),
    ).toEqual({ checks: { externalChecks: [{ memo: "a\\u{202e}b" }, ["c\\u{200b}d"]] } });
    expect(
      sanitizeTypedDataForDisplay({ qty: 10, required: true, target: null, missing: undefined }),
    ).toEqual({ qty: 10, required: true, target: null, missing: undefined });
  });

  it("keeps a signed string that spells an escape apart from the character it names", () => {
    expect(sanitizeTypedDataForDisplay({ memo: "\\u{202e}" })).toEqual({
      memo: "\\\\u{202e}",
    });
    expect(sanitizeTypedDataForDisplay({ memo: "\u{202e}" })).toEqual({ memo: "\\u{202e}" });
  });
});

describe("truncateForDisplay", () => {
  it("leaves text at or under the cap byte-identical and cuts text past it, saying the page is not showing all of it", () => {
    const justUnder = "a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS - 1);
    expect(truncateForDisplay(justUnder)).toBe(justUnder);
    const exact = "a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS);
    expect(truncateForDisplay(exact)).toBe(exact);
    expect(truncateForDisplay("")).toBe("");

    const shown = truncateForDisplay("a".repeat(MAX_DISPLAYED_TYPED_DATA_CHARS + 1));
    expect(shown).toContain("truncated");
    expect(shown).toContain("your wallet shows what it signs");
    expect(shown.startsWith(exact)).toBe(true);
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
