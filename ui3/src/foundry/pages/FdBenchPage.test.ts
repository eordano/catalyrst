import { describe, expect, it } from "vitest";

import { checksLabel, type FdBenchReportVM } from "./FdBenchPage";

function report(overrides: Partial<FdBenchReportVM>): FdBenchReportVM {
  return {
    id: "bench-1",
    slug: "flagtag",
    runner: "dclbots",
    realm: "127.0.0.1:8000",
    ranAt: "2026-08-07T00:00:00.000Z",
    verdict: "fail",
    checksTotal: 5,
    checksFailed: 0,
    checksUnevaluable: 0,
    missingTools: [],
    stubbedTools: [],
    networkWrites: 0,
    shots: 0,
    evidence: null,
    replayHref: null,
    gameHref: null,
    ...overrides,
  };
}

describe("checksLabel", () => {
  it("names the cannot-evaluate policy when every failure is unevaluable", () => {
    expect(
      checksLabel(report({ checksTotal: 5, checksFailed: 5, checksUnevaluable: 5 })),
    ).toBe("5 checks could not be evaluated — counted as failed");
    expect(
      checksLabel(report({ checksTotal: 5, checksFailed: 2, checksUnevaluable: 2 })),
    ).toBe(
      "3 of 5 checks passed — 2 checks could not be evaluated — counted as failed",
    );
  });

  it("keeps the plain form for mixed or real failures", () => {
    expect(
      checksLabel(report({ checksTotal: 5, checksFailed: 3, checksUnevaluable: 1 })),
    ).toBe("2 of 5 checks passed");
    expect(
      checksLabel(report({ checksTotal: 4, checksFailed: 1, checksUnevaluable: 0 })),
    ).toBe("3 of 4 checks passed");
  });

  it("keeps the plain form when nothing failed", () => {
    expect(
      checksLabel(report({ checksTotal: 4, checksFailed: 0, checksUnevaluable: 0 })),
    ).toBe("4 of 4 checks passed");
  });

  it("still tells an arena verdict apart from an unrecorded one", () => {
    expect(checksLabel(report({ checksTotal: null, verdict: "pass" }))).toBe(
      "verdict from the runner's exit status — no per-check breakdown recorded",
    );
    expect(checksLabel(report({ checksTotal: null, verdict: null }))).toBe(
      "snapshot only — verdict not recorded",
    );
  });
});
