import { describe, expect, it } from "vitest";

import { runVerdictReading } from "@ui/foundry/components/FdVerdictPill";

// The one presentation function for a run's stored verdict (shelf card, game
// header, run cards, timeline). These pin the branch the live data cannot
// currently exercise — a run with GENUINE failures — next to the harness-gap
// branch the 08-18 fix exists for, so neither can silently regress.

describe("runVerdictReading", () => {
  it("keeps a genuine failure a red 'failed', counting only the genuine ones", () => {
    const r = runVerdictReading({
      verdict: "fail",
      checksFailed: 3,
      checksTotal: 6,
      checksUnevaluable: 1,
    });
    expect(r.verdict).toBe("fail");
    expect(r.label).toBe("failed");
    expect(r.detail).toBe("2 of 6 checks failed, 1 more not evaluable");
  });

  it("turns harness-gap-only failures into a neutral verified count", () => {
    const r = runVerdictReading({
      verdict: "fail",
      checksFailed: 4,
      checksTotal: 6,
      checksUnevaluable: 4,
    });
    expect(r.verdict).toBe("watch");
    expect(r.label).toBe("2 of 6 verified");
    expect(r.detail).toBe(
      "4 of 6 checks could not be evaluated (harness gaps) — no genuine failures",
    );
  });

  it("stays 'failed' when the counts are unrecorded — never invents neutrality", () => {
    const r = runVerdictReading({
      verdict: "fail",
      checksFailed: null,
      checksTotal: null,
      checksUnevaluable: null,
    });
    expect(r.verdict).toBe("fail");
    expect(r.label).toBe("failed");
    expect(r.detail).toBeNull();
  });

  it("a pass stays a pass and counts what passed", () => {
    const r = runVerdictReading({
      verdict: "pass",
      checksFailed: 0,
      checksTotal: 6,
      checksUnevaluable: 0,
    });
    expect(r.verdict).toBe("pass");
    expect(r.label).toBe("passed");
    expect(r.detail).toBe("6 of 6 checks passed");
  });

  it("all checks unevaluable reads as zero verified, not as failure", () => {
    const r = runVerdictReading({
      verdict: "fail",
      checksFailed: 2,
      checksTotal: 2,
      checksUnevaluable: 2,
    });
    expect(r.verdict).toBe("watch");
    expect(r.label).toBe("0 of 2 verified");
  });
});
