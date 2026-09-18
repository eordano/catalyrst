import { describe, expect, it } from "vitest";

import { composeTrace, parseErrorName, truncateOneLine } from "./ErrorPage";

describe("truncateOneLine", () => {
  it("collapses whitespace, truncates with an ellipsis, and falls back for empty input", () => {
    expect(truncateOneLine("hello\n   world\t!")).toBe("hello world !");
    const out = truncateOneLine("a".repeat(500), 200);
    expect(out.length).toBe(200);
    expect(out.endsWith("\u{2026}")).toBe(true);
    expect(truncateOneLine("")).toBe("An unexpected error occurred.");
    expect(truncateOneLine("   \n  ")).toBe("An unexpected error occurred.");
  });
});

describe("parseErrorName", () => {
  it("extracts the error class from the first line, defaulting to Error", () => {
    expect(parseErrorName("TypeError: cannot read x\n  at foo")).toBe("TypeError");
    expect(parseErrorName("RangeError: bad\n  at bar")).toBe("RangeError");
    expect(parseErrorName("just some message")).toBe("Error");
    expect(parseErrorName("")).toBe("Error");
  });
});

describe("composeTrace", () => {
  it("appends URL and timestamp beneath the detail, with placeholders when empty", () => {
    const out = composeTrace("TypeError: boom\n  at x", "https://catalyst.example.com/p", "2026-07-06T00:00:00Z");
    expect(out).toContain("TypeError: boom");
    expect(out).toContain("URL:  https://catalyst.example.com/p");
    expect(out).toContain("Time: 2026-07-06T00:00:00Z");
    const empty = composeTrace("", "", "t");
    expect(empty).toContain("No further detail was captured.");
    expect(empty).toContain("URL:  (unknown)");
  });
});
