import { describe, expect, it } from "vitest";

import { rowStamp } from "./FdTimelinePage";

describe("rowStamp", () => {
  it("stamps a timed row in the one instant format, whatever its year", () => {
    expect(rowStamp("2026-08-15T14:03:00.000Z")).toBe("Aug 15, 2026 · 14:03 UTC");
    expect(rowStamp("2021-03-16T14:03:00.000Z")).toBe("Mar 16, 2021 · 14:03 UTC");
  });

  it("stamps a date-only row as the day alone — no invented midnight", () => {
    // dateOnly is a stored fact on the row (set by the importer), not a sniff
    // of the timestamp: a genuine midnight event keeps its clock time because
    // its row carries dateOnly=false.
    expect(rowStamp("2021-03-16T00:00:00.000Z", true)).toBe("Mar 16, 2021");
    expect(rowStamp("2026-08-15T00:00:00.000Z", true)).toBe("Aug 15, 2026");
    expect(rowStamp("2026-08-15T00:00:00.000Z", false)).toBe(
      "Aug 15, 2026 · 00:00 UTC",
    );
  });
});
