import { describe, expect, it } from "vitest";

import {
  benchIdsFor,
  buildBenchIngest,
  parseCheckLines,
  parseFinalVerdict,
  trimSnapshot,
  type BenchEvidence,
} from "./bench.server";

// The strings below are the harness's own output shape, verbatim:
// `dclbots/checks.py:Result.line()` writes "[PASS|FAIL] <kind>: <detail>" with
// "  (<why>)" appended only on failure, and `run.py:report()` closes with
// "<slug>: PASS (N checks)" / "<slug>: FAIL (k of N checks)" after two spaces of
// indent. If either format moves upstream, these tests are what notices.

const RUN_LOG = [
  "",
  "  [PASS] log: 1 line matches 'round start'",
  "  [FAIL] component-present: no Transform on 512  (entity pages were truncated)",
  "  [PASS] distinct-writers: 3 distinct writer(s), wanted >= 2",
  "",
  "flagtag: FAIL (1 of 3 checks)",
  "",
];

function evidence(over: Partial<BenchEvidence> = {}): BenchEvidence {
  return {
    runner: "dclbots",
    slug: "flagtag",
    sceneId: "flagtag",
    ranAt: "2026-08-15T00:00:00.000Z",
    evidencePath: "/srv/evidence/flagtag-1755200000",
    manifest: { slug: "flagtag" },
    snapshot: { logWindow: {}, networkWrites: [], missingTools: [], stubbedTools: {} },
    logLines: RUN_LOG,
    shots: [],
    exitCode: null,
    ...over,
  };
}

describe("parseCheckLines", () => {
  it("reads kind, detail and the failure's why", () => {
    expect(parseCheckLines(RUN_LOG)).toEqual([
      { kind: "log", pass: true, detail: "1 line matches 'round start'", why: "" },
      {
        kind: "component-present",
        pass: false,
        detail: "no Transform on 512",
        why: "entity pages were truncated",
      },
      {
        kind: "distinct-writers",
        pass: true,
        detail: "3 distinct writer(s), wanted >= 2",
        why: "",
      },
    ]);
  });

  it("keeps a passing detail that ends in its own parenthesis", () => {
    const [check] = parseCheckLines(["  [PASS] distinct-writers: 2 writer(s)"]);
    expect(check).toEqual({
      kind: "distinct-writers",
      pass: true,
      detail: "2 writer(s)",
      why: "",
    });
  });

  it("ignores the runner's prose", () => {
    expect(parseCheckLines(["connecting to explorer", "flagtag: PASS (3 checks)"])).toEqual(
      [],
    );
  });
});

describe("parseFinalVerdict", () => {
  it("reads the failing summary", () => {
    expect(parseFinalVerdict(RUN_LOG)).toEqual({
      slug: "flagtag",
      verdict: "fail",
      checksTotal: 3,
      checksFailed: 1,
    });
  });

  it("reads the passing summary, where no count of failures is printed", () => {
    expect(parseFinalVerdict(["skychaser: PASS (12 checks)"])).toEqual({
      slug: "skychaser",
      verdict: "pass",
      checksTotal: 12,
      checksFailed: 0,
    });
  });

  it("returns null when the runner never closed", () => {
    expect(parseFinalVerdict(["  [PASS] log: fine"])).toBeNull();
  });
});

describe("trimSnapshot", () => {
  it("caps the network writes and reports both the cap and the true total", () => {
    const writes = Array.from({ length: 600 }, (_, i) => ({ entity: i }));
    const trimmed = trimSnapshot({ networkWrites: writes });
    expect((trimmed.networkWrites as unknown[]).length).toBe(500);
    expect(trimmed.networkWritesTotal).toBe(600);
    expect(trimmed.networkWritesCapped).toBe(500);
  });

  it("does not claim a cap it did not apply", () => {
    const trimmed = trimSnapshot({ networkWrites: [{ entity: 1 }] });
    expect(trimmed.networkWritesTotal).toBe(1);
    expect(trimmed).not.toHaveProperty("networkWritesCapped");
  });

  it("drops nothing it was given and invents nothing it was not", () => {
    expect(trimSnapshot({})).toEqual({});
  });
});

describe("buildBenchIngest — dclbots", () => {
  it("maps one turn, the manifest, the snapshot and one event per verdict", () => {
    const { events, report, trajectory } = buildBenchIngest(evidence());
    expect(events.map((e) => e.type)).toEqual([
      "turn/start",
      "tool/call",
      "obs/snapshot",
      "check/verdict",
      "check/verdict",
      "check/verdict",
      "turn/end",
    ]);
    expect(events.map((e) => e.seq)).toEqual([0, 1, 2, 3, 4, 5, 6]);
    expect(events.every((e) => e.time === "2026-08-15T00:00:00.000Z")).toBe(true);
    expect(events[1]?.data).toEqual({
      callId: "manifest",
      name: "dclbots.run",
      arguments: { slug: "flagtag" },
    });
    expect(trajectory.finishReason).toEqual({
      kind: "error",
      detail: "1 of 3 checks failed",
    });
    expect(report.verdict).toBe("fail");
    expect(report.checksTotal).toBe(3);
    expect(report.checksFailed).toBe(1);
    expect(report.trajectoryId).toBe(trajectory.id);
  });

  it("records a passing run as completed", () => {
    const { report, trajectory } = buildBenchIngest(
      evidence({ logLines: ["  [PASS] log: fine", "flagtag: PASS (1 checks)"] }),
    );
    expect(trajectory.finishReason).toEqual({ kind: "completed" });
    expect(report.verdict).toBe("pass");
    expect(report.checksFailed).toBe(0);
  });

  it("leaves the verdict unrecorded when the runner's stdout was never captured", () => {
    const { report, trajectory, events } = buildBenchIngest(evidence({ logLines: [] }));
    expect(report.verdict).toBeNull();
    expect(report.checksTotal).toBeNull();
    expect(trajectory.finishReason.kind).toBe("interrupted");
    expect(events.some((e) => e.type === "check/verdict")).toBe(false);
  });

  it("flattens the tool inventory without dropping why a stub is a stub", () => {
    const { report } = buildBenchIngest(
      evidence({
        snapshot: {
          missingTools: ["scene.click"],
          stubbedTools: { "ui.type": "returns empty" },
          networkWrites: [{ entity: 1 }, { entity: 2 }],
        },
      }),
    );
    expect(report.missingTools).toEqual(["scene.click"]);
    expect(report.stubbedTools).toEqual(["ui.type: returns empty"]);
    expect(report.networkWrites).toBe(2);
  });
});

describe("buildBenchIngest — arena", () => {
  const arena = evidence({
    runner: "arena",
    slug: "flagtag-arena",
    manifest: null,
    snapshot: null,
    logLines: ["flagtag sandbox: seed 7", "", "  0xseat1   chase   225.8s"],
    exitCode: 0,
  });

  it("stores every printed line verbatim and synthesises no metric", () => {
    const { events, report } = buildBenchIngest(arena);
    expect(events.map((e) => e.type)).toEqual([
      "turn/start",
      "obs/snapshot",
      "obs/snapshot",
      "turn/end",
    ]);
    expect(events[1]?.data).toEqual({ line: "flagtag sandbox: seed 7" });
    expect(events[2]?.data).toEqual({ line: "  0xseat1   chase   225.8s" });
    expect(report.verdict).toBe("pass");
    expect(report.networkWrites).toBeNull();
  });

  it("takes the verdict from the exit code", () => {
    expect(buildBenchIngest({ ...arena, exitCode: 2 }).report.verdict).toBe("fail");
    expect(buildBenchIngest({ ...arena, exitCode: null }).report.verdict).toBeNull();
  });
});

describe("benchIdsFor", () => {
  it("is stable per evidence directory, so re-ingesting updates one episode", () => {
    const a = benchIdsFor("/srv/evidence/flagtag-1755200000", "2026-08-15T00:00:00.000Z");
    const b = benchIdsFor("/srv/evidence/flagtag-1755200000/", "2026-08-15T09:00:00.000Z");
    expect(a).toEqual(b);
    expect(a.trajectoryId).toBe("traj-flagtag-1755200000");
    expect(a.reportId).toBe("bench-flagtag-1755200000");
  });
});
