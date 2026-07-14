import { describe, expect, it } from "vitest";

import { composeTrace, parseErrorName, truncateOneLine } from "./ErrorPage";

describe("truncateOneLine", () => {
  it("collapses whitespace to a single line", () => {
    expect(truncateOneLine("hello\n   world\t!")).toBe("hello world !");
  });

  it("truncates long messages with an ellipsis", () => {
    const long = "a".repeat(500);
    const out = truncateOneLine(long, 200);
    expect(out.length).toBe(200);
    expect(out.endsWith("\u{2026}")).toBe(true);
  });

  it("falls back for empty input", () => {
    expect(truncateOneLine("")).toBe("An unexpected error occurred.");
    expect(truncateOneLine("   \n  ")).toBe("An unexpected error occurred.");
  });
});

describe("parseErrorName", () => {
  it("extracts the error class from the first line", () => {
    expect(parseErrorName("TypeError: cannot read x\n  at foo")).toBe("TypeError");
    expect(parseErrorName("RangeError: bad\n  at bar")).toBe("RangeError");
  });

  it("defaults to Error when no name is present", () => {
    expect(parseErrorName("just some message")).toBe("Error");
    expect(parseErrorName("")).toBe("Error");
  });
});

describe("composeTrace", () => {
  it("appends URL and timestamp beneath the detail", () => {
    const out = composeTrace("TypeError: boom\n  at x", "https://catalyst.example.com/p", "2026-07-06T00:00:00Z");
    expect(out).toContain("TypeError: boom");
    expect(out).toContain("URL:  https://catalyst.example.com/p");
    expect(out).toContain("Time: 2026-07-06T00:00:00Z");
  });

  it("handles empty detail gracefully", () => {
    const out = composeTrace("", "", "t");
    expect(out).toContain("No further detail was captured.");
    expect(out).toContain("URL:  (unknown)");
  });
});
