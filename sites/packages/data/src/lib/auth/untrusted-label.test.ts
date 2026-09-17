import { describe, expect, it } from "vitest";

import {
  MAX_UNTRUSTED_LABEL_CHARS,
  formatUntrustedLabel,
  revealUnreadableCharacters,
} from "./untrusted-label";

describe("revealUnreadableCharacters", () => {
  it.each([
    [
      "an override that would reorder its neighbours",
      "Decentraland\u{202e}Support",
      "Decentraland\\u{202e}Support",
    ],
    ["a zero-width space", "Decentra\u{200b}land", "Decentra\\u{200b}land"],
    ["a byte-order mark", "\u{feff}Decentraland", "\\u{feff}Decentraland"],
    ["what decoding invalid UTF-8 produces", "MANA\u{fffd}", "MANA\\u{fffd}"],
    ["a line separator", "one\u{2028}two", "one\\u{2028}two"],
    ["a backslash, so an escape it spells stays apart", "\\u{202e}", "\\\\u{202e}"],
  ])("shows %s as a visible escape", (_label, text, expected) => {
    expect(revealUnreadableCharacters(text)).toBe(expected);
  });

  it("leaves the whitespace a reader can see exactly as it was written", () => {
    const laid = "line one\nline\ttwo\r\nline three";
    expect(revealUnreadableCharacters(laid)).toBe(laid);
  });
});

describe("formatUntrustedLabel", () => {
  it("trims what surrounds the label", () => {
    expect(formatUntrustedLabel("  Decentraland  ")).toBe("Decentraland");
  });

  it("cuts a label that would push the facts around it out of view", () => {
    const long = "a".repeat(MAX_UNTRUSTED_LABEL_CHARS * 3);
    const shown = formatUntrustedLabel(long);
    expect(shown.length).toBe(MAX_UNTRUSTED_LABEL_CHARS);
    expect(shown.endsWith("\u{2026}")).toBe(true);
  });

  it("cuts to the cut length it is asked for", () => {
    expect(formatUntrustedLabel("abcdefghij", 4)).toBe("abc\u{2026}");
    expect(formatUntrustedLabel("abcd", 4)).toBe("abcd");
  });

  it("escapes before it cuts, so the cut bounds what is shown", () => {
    const shown = formatUntrustedLabel(`${"\u{202e}".repeat(8)}0xattacker`);
    expect(shown).not.toContain("\u{202e}");
    expect(shown.length).toBe(MAX_UNTRUSTED_LABEL_CHARS);
  });

  it("never cuts a code point in half", () => {
    const shown = formatUntrustedLabel("\u{1f600}".repeat(60), 5);
    expect(shown).toBe("\u{1f600}\u{1f600}\u{2026}");
    expect(shown).not.toContain("\u{fffd}");
  });

  it("bounds the cut in the units the space it is shown in is measured by", () => {
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
