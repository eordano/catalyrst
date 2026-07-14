import { describe, expect, it } from "vitest";

import {
  readResponseSignals,
  reportLine,
  revisionSplit,
  visitDays,
  type ResponseSignalsDb,
} from "./response.server";
import type { BotReport } from "./types";

// The response read hands the page counts and distinct-badge counts only, with
// the arena rule applied in the words themselves. The fake below dispatches on
// the SQL like the memory tests do; the assertions cover what the reader
// DERIVES (per-day returning math, per-replay grouping, UUID exclusion) and the
// filters the reader actually sends.

describe("visitDays", () => {
  it("counts distinct badges per day, and returning = seen on an earlier day", () => {
    const days = visitDays([
      { day: "2026-08-15", badge: "dccd" },
      { day: "2026-08-15", badge: "5cb9" },
      { day: "2026-08-16", badge: "5cb9" },
      { day: "2026-08-16", badge: "4d2f" },
    ]);
    expect(days).toEqual([
      { day: "2026-08-15", visitors: 2, returning: 0 },
      { day: "2026-08-16", visitors: 2, returning: 1 },
    ]);
  });

  it("a badge repeated within one day is one visitor, not a returner", () => {
    expect(
      visitDays([
        { day: "2026-08-15", badge: "dccd" },
        { day: "2026-08-15", badge: "dccd" },
      ]),
    ).toEqual([{ day: "2026-08-15", visitors: 1, returning: 0 }]);
  });

  it("sorts days even when rows arrive out of order", () => {
    const days = visitDays([
      { day: "2026-08-16", badge: "a1a1" },
      { day: "2026-08-15", badge: "a1a1" },
    ]);
    expect(days.map((d) => d.day)).toEqual(["2026-08-15", "2026-08-16"]);
    expect(days[1]).toMatchObject({ returning: 1 });
  });
});

describe("revisionSplit", () => {
  const days = [
    { day: "2026-08-15", visitors: 2, returning: 0 },
    { day: "2026-08-16", visitors: 3, returning: 1 },
  ];

  it("is null when the deploy predates every measured day — no revision to read", () => {
    expect(revisionSplit("2026-07-05T19:52:10.768Z", days)).toBeNull();
  });

  it("is null for a never-deployed scene", () => {
    expect(revisionSplit(null, days)).toBeNull();
  });

  it("splits only when visits sit on both sides of the deploy day", () => {
    expect(revisionSplit("2026-08-16T09:00:00.000Z", days)).toEqual({
      deployedDay: "2026-08-16",
      before: 2,
      after: 3,
    });
    // All measured visits after the deploy: nothing to compare against.
    expect(revisionSplit("2026-08-15T00:00:00.000Z", days)).toBeNull();
  });
});

function report(overrides: Partial<BotReport>): BotReport {
  return {
    id: "bench-flagtag-smoke",
    sceneId: "flagtag",
    slug: "flagtag",
    runner: "dclbots",
    realm: "http://127.0.0.1:8000",
    ranAt: "2026-08-10T12:00:00.000Z",
    verdict: "fail",
    checksTotal: 2,
    checksFailed: 2,
    checksUnevaluable: 0,
    missingTools: [],
    stubbedTools: [],
    networkWrites: 0,
    shots: [],
    evidencePath: "/evidence/flagtag-smoke",
    trajectoryId: "traj-flagtag-smoke",
    ...overrides,
  };
}

describe("reportLine — the arena rule, server-side", () => {
  it("an arena run reads as a completed simulation, never a pass", () => {
    const line = reportLine(
      report({ runner: "arena", verdict: "pass", checksTotal: null, checksFailed: null }),
    );
    expect(line.text).toBe("completed a sandbox simulation");
    expect(line.text).not.toContain("pass");
  });

  it("a dclbots failure counts its checks in plain words", () => {
    expect(reportLine(report({})).text).toBe("2 of 2 checks failed");
  });

  it("a dclbots pass names its checks", () => {
    expect(
      reportLine(report({ verdict: "pass", checksFailed: 0 })).text,
    ).toBe("passed all 2 checks");
    expect(
      reportLine(report({ verdict: "pass", checksTotal: 1, checksFailed: 0 })).text,
    ).toBe("passed all 1 check");
  });

  it("a run without a stored verdict says so instead of inventing one", () => {
    expect(reportLine(report({ verdict: null })).text).toBe(
      "ran without a recorded verdict",
    );
  });

  it("links the run to its evidence page and its replay", () => {
    const line = reportLine(report({}));
    expect(line.evidenceHref).toBe("/foundry/console/evidence/bench-flagtag-smoke");
    expect(line.replayHref).toBe("/foundry/console/trajectories/traj-flagtag-smoke");
  });

  it("keeps absent links absent", () => {
    const line = reportLine(report({ evidencePath: null, trajectoryId: null }));
    expect(line.evidenceHref).toBeNull();
    expect(line.replayHref).toBeNull();
  });
});

type FakeResponse = { rows: object[] };

function fakeDb(
  handler: (text: string, values: unknown[]) => object[],
): ResponseSignalsDb & { queries: { text: string; values: unknown[] }[] } {
  const queries: { text: string; values: unknown[] }[] = [];
  return {
    queries,
    async query(text: string, values: unknown[] = []): Promise<FakeResponse> {
      queries.push({ text, values });
      return { rows: handler(text, values) };
    },
  } as ResponseSignalsDb & { queries: { text: string; values: unknown[] }[] };
}

describe("readResponseSignals", () => {
  it("aggregates visits, replays and downloads into counts — never badge lists", async () => {
    const db = fakeDb((text) => {
      if (text.includes("fd_game_viewed")) {
        return [
          { day: "2026-08-15", badge: "dccd", n: 2 },
          { day: "2026-08-16", badge: "dccd", n: 1 },
          { day: "2026-08-16", badge: "5cb9", n: 1 },
        ];
      }
      if (text.includes("fd_replay_opened")) {
        return [
          { trajectory_id: "traj-flagtag-arena", event: "fd_replay_opened", n: 3 },
          { trajectory_id: "traj-flagtag-arena", event: "fd_replay_stepped", n: 5 },
        ];
      }
      return [{ n: 7 }];
    });

    const signals = await readResponseSignals(db, {
      slug: "flagtag",
      trajectoryIds: ["traj-flagtag-arena"],
    });

    expect(signals).toEqual({
      visitDays: [
        { day: "2026-08-15", visitors: 1, returning: 0 },
        { day: "2026-08-16", visitors: 2, returning: 1 },
      ],
      visitEvents: 4,
      distinctVisitors: 2,
      replays: [{ trajectoryId: "traj-flagtag-arena", opens: 3, interactions: 5 }],
      downloads: 7,
    });
    // Nothing badge-shaped leaves the reader.
    expect(JSON.stringify(signals)).not.toContain("dccd");
  });

  it("every query excludes pre-badge-fix raw ids and floors at the measured date", async () => {
    const db = fakeDb((text) =>
      text.includes("fd_bundle_downloaded") ? [{ n: 0 }] : [],
    );
    await readResponseSignals(db, { slug: "flagtag", trajectoryIds: ["t1"] });
    expect(db.queries).toHaveLength(3);
    for (const q of db.queries) {
      expect(q.text).toContain("char_length(body->>'anonymousId') <= 4");
      // Downloads floor at the day the GET-minted loader track was removed;
      // visits and replays keep the original measured floor.
      expect(q.values).toContain(
        q.text.includes("fd_bundle_downloaded") ? "2026-08-20" : "2026-08-14",
      );
      // The stale collector's invalid_reason flag is not part of any read.
      expect(q.text).not.toContain("invalid_reason");
    }
  });

  it("skips the replay query when the game has no episodes", async () => {
    const db = fakeDb((text) =>
      text.includes("fd_bundle_downloaded") ? [{ n: 0 }] : [],
    );
    const signals = await readResponseSignals(db, {
      slug: "template-game",
      trajectoryIds: [],
    });
    expect(signals.replays).toEqual([]);
    expect(db.queries.some((q) => q.text.includes("fd_replay_opened"))).toBe(false);
  });
});
