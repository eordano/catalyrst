import { beforeEach, describe, expect, it, vi } from "vitest";

import { createSeries, deriveOccurrences, listUpcoming } from "./sessions.server";
import { FoundryStateError } from "./db.server";

const state = vi.hoisted(() => ({
  series: [] as Record<string, unknown>[],
  sceneIds: [] as string[],
}));

// Enough of Postgres to exercise the read filter and the scene pre-check in
// memory: getPool/withTx hand back one fake query dispatcher, everything else in
// db.server (FoundryStateError included) stays real.
vi.mock("./db.server", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./db.server")>();
  async function query(
    text: string,
    values: unknown[] = [],
  ): Promise<{ rows: unknown[]; rowCount: number }> {
    if (text.includes("FROM foundry.session_series") && text.includes("SELECT ss.id")) {
      const rows = text.includes("ss.scene_id = $1")
        ? state.series.filter((s) => s.scene_id === values[0])
        : state.series;
      return { rows, rowCount: rows.length };
    }
    if (text.includes("FROM foundry.session_rsvp")) return { rows: [], rowCount: 0 };
    if (text.includes("foundry.persona")) return { rows: [], rowCount: 0 };
    if (text.includes("FROM foundry.scene WHERE id = $1")) {
      const found = state.sceneIds.includes(String(values[0]));
      return { rows: found ? [{ one: 1 }] : [], rowCount: found ? 1 : 0 };
    }
    if (text.includes("INSERT INTO foundry.session_series")) {
      return { rows: [{ id: "ser-1" }], rowCount: 1 };
    }
    if (text.includes("INSERT INTO foundry.action_log")) return { rows: [], rowCount: 1 };
    throw new Error(`unexpected query: ${text}`);
  }
  return {
    ...actual,
    getPool: () => ({ query }),
    withTx: async (fn: (client: unknown) => Promise<unknown>) => fn({ query }),
    assertRate: () => {},
  };
});

vi.mock("./roles.server", () => ({
  requireHost: async () => {},
}));

const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;

describe("deriveOccurrences", () => {
  const now = Date.parse("2026-08-16T12:00:00.000Z");
  const horizonEnd = now + 28 * DAY_MS;

  it("a weekly series that started a year ago still yields the horizon's occurrences", () => {
    const firstAt = now - 52 * WEEK_MS + 60 * 60 * 1000;
    const occs = deriveOccurrences(firstAt, "weekly", now, horizonEnd);
    expect(occs).toHaveLength(4);
    for (const occ of occs) {
      const t = Date.parse(occ);
      expect(t).toBeGreaterThanOrEqual(now);
      expect(t).toBeLessThanOrEqual(horizonEnd);
      expect((t - firstAt) % WEEK_MS).toBe(0);
    }
  });

  it("a weekly series starting inside the horizon yields each 7-day step in range", () => {
    const firstAt = now + 3 * DAY_MS;
    const occs = deriveOccurrences(firstAt, "weekly", now, horizonEnd);
    expect(occs).toEqual([
      new Date(firstAt).toISOString(),
      new Date(firstAt + WEEK_MS).toISOString(),
      new Date(firstAt + 2 * WEEK_MS).toISOString(),
      new Date(firstAt + 3 * WEEK_MS).toISOString(),
    ]);
  });

  it("a once series yields its date only while inside the horizon", () => {
    const inRange = now + 2 * DAY_MS;
    expect(deriveOccurrences(inRange, "once", now, horizonEnd)).toEqual([
      new Date(inRange).toISOString(),
    ]);
    expect(deriveOccurrences(now - DAY_MS, "once", now, horizonEnd)).toEqual([]);
    expect(deriveOccurrences(horizonEnd + DAY_MS, "once", now, horizonEnd)).toEqual([]);
  });
});

function seriesRow(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: "ser-flagtag",
    title: "Flag Tag night",
    body: "",
    scene_id: "flagtag",
    scene_title: "Flag Tag",
    cadence: "once",
    first_at: new Date(Date.now() + 3 * DAY_MS).toISOString(),
    duration_minutes: 60,
    created_by_sid: "sid-host",
    retired_at: null,
    ...over,
  };
}

describe("listUpcoming sceneId filter", () => {
  beforeEach(() => {
    state.series = [
      seriesRow(),
      seriesRow({
        id: "ser-maze",
        title: "Maze night",
        scene_id: "maze",
        scene_title: "Maze",
      }),
    ];
    state.sceneIds = ["flagtag", "maze"];
  });

  it("returns only the named scene's occurrences", async () => {
    const occs = await listUpcoming("sid-viewer", "flagtag");
    expect(occs).toHaveLength(1);
    expect(occs[0].sceneId).toBe("flagtag");
    expect(occs[0].seriesId).toBe("ser-flagtag");
  });

  it("without a sceneId still returns the whole calendar", async () => {
    const occs = await listUpcoming("sid-viewer");
    expect(occs.map((o) => o.sceneId).sort()).toEqual(["flagtag", "maze"]);
  });
});

describe("createSeries scene pre-check", () => {
  beforeEach(() => {
    state.series = [];
    state.sceneIds = ["flagtag"];
  });

  const input = {
    title: "Game night",
    body: "",
    cadence: "weekly" as const,
    firstAt: new Date(Date.now() + DAY_MS).toISOString(),
    durationMinutes: 60,
  };

  it("refuses a scene id that is not in the games registry", async () => {
    await expect(
      createSeries({ sid: "sid-host", input: { ...input, sceneId: "not-a-scene" } }),
    ).rejects.toThrow(FoundryStateError);
    await expect(
      createSeries({ sid: "sid-host", input: { ...input, sceneId: "not-a-scene" } }),
    ).rejects.toThrow("That scene is not in the games registry.");
  });

  it("accepts a registered scene id", async () => {
    await expect(
      createSeries({ sid: "sid-host", input: { ...input, sceneId: "flagtag" } }),
    ).resolves.toEqual({ id: "ser-1" });
  });
});
