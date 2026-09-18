import { describe, expect, it } from "vitest";

import {
  MAX_UNTRUSTED_LABEL_CHARS,
  formatUntrustedLabel,
  revealUnreadableCharacters,
} from "./untrusted-label";

describe("revealUnreadableCharacters", () => {
  it("shows every unreadable character as a visible escape and leaves visible whitespace alone", () => {
    const cases: [string, string, string][] = [
      ["reorder override", "Decentraland\u{202e}Support", "Decentraland\\u{202e}Support"],
      ["zero-width space", "Decentra\u{200b}land", "Decentra\\u{200b}land"],
      ["byte-order mark", "\u{feff}Decentraland", "\\u{feff}Decentraland"],
      ["replacement char", "MANA\u{fffd}", "MANA\\u{fffd}"],
      ["line separator", "one\u{2028}two", "one\\u{2028}two"],
      ["backslash", "\\u{202e}", "\\\\u{202e}"],
    ];
    const wrong = cases
      .filter(([, text, expected]) => revealUnreadableCharacters(text) !== expected)
      .map(([label]) => label);
    expect(wrong).toEqual([]);
    const laid = "line one\nline\ttwo\r\nline three";
    expect(revealUnreadableCharacters(laid)).toBe(laid);
  });
});

describe("formatUntrustedLabel", () => {
  it("trims, cuts to the default or requested length with an ellipsis, and escapes before cutting", () => {
    expect(formatUntrustedLabel("  Decentraland  ")).toBe("Decentraland");
    const shown = formatUntrustedLabel("a".repeat(MAX_UNTRUSTED_LABEL_CHARS * 3));
    expect(shown.length).toBe(MAX_UNTRUSTED_LABEL_CHARS);
    expect(shown.endsWith("\u{2026}")).toBe(true);
    expect(formatUntrustedLabel("abcdefghij", 4)).toBe("abc\u{2026}");
    expect(formatUntrustedLabel("abcd", 4)).toBe("abcd");
    const escaped = formatUntrustedLabel(`${"\u{202e}".repeat(8)}0xattacker`);
    expect(escaped).not.toContain("\u{202e}");
    expect(escaped.length).toBe(MAX_UNTRUSTED_LABEL_CHARS);
  });

  it("never cuts a code point in half and bounds the cut in UTF-16 units", () => {
    const shown = formatUntrustedLabel("\u{1f600}".repeat(60), 5);
    expect(shown).toBe("\u{1f600}\u{1f600}\u{2026}");
    expect(shown).not.toContain("\u{fffd}");
    const astral = formatUntrustedLabel("\u{1f600}".repeat(MAX_UNTRUSTED_LABEL_CHARS));
    expect(astral.length).toBeLessThanOrEqual(MAX_UNTRUSTED_LABEL_CHARS);
    expect(astral.endsWith("\u{2026}")).toBe(true);
    expect(astral).not.toContain("\u{fffd}");
  });

  it("has nothing to show for what is not text", () => {
    for (const value of [undefined, null, 7, {}, []]) {
      expect(formatUntrustedLabel(value)).toBe("");
    }
  });
});
