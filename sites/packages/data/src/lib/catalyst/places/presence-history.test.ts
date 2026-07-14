import { describe, expect, it } from "vitest";

import {
  bucketize,
  clampHistoryLimit,
  fetchWorldHistory,
  HISTORY_MAX_LIMIT,
  occupiedLabel,
} from "./presence-history";
import { loadWorldOccupancyHistory } from "./presence-history.server";

const CADENCE = 300;

function rowsEvery5Min(start: string, counts: number[]): { taken_at: string; count: number }[] {
  const t0 = Date.parse(start);
  return counts.map((count, i) => ({
    taken_at: new Date(t0 + i * CADENCE * 1000).toISOString(),
    count,
  }));
}

describe("clampHistoryLimit", () => {
  it("clamps to [1, 5000] so the query string never lies about what was asked", () => {
    expect(clampHistoryLimit(0)).toBe(1);
    expect(clampHistoryLimit(-7)).toBe(1);
    expect(clampHistoryLimit(999_999)).toBe(HISTORY_MAX_LIMIT);
    expect(clampHistoryLimit(2016)).toBe(2016);
    expect(clampHistoryLimit(2016.9)).toBe(2016);
    expect(clampHistoryLimit(Number.NaN)).toBe(200);
  });

  it("is applied to the outgoing request", async () => {
    const seen: string[] = [];
    const spy = (async (url: string) => {
      seen.push(url);
      return new Response(JSON.stringify({ history: [] }), { status: 200 });
    }) as unknown as typeof fetch;
    await fetchWorldHistory("a.dcl.eth", 10_000, { fetchImpl: spy });
    expect(seen[0]).toContain("limit=5000");
    expect(seen[0]).not.toContain("limit=10000");
  });
});

describe("bucketize \u{2014} a gap is not a zero", () => {
  it("emits null buckets across a deliberate 40-minute hole and exactly one gap band", () => {
    const before = rowsEvery5Min("2026-07-20T00:00:00.000Z", [2, 3, 1]);
    // ...40 minutes of nothing (8 cadence buckets)...
    const after = rowsEvery5Min("2026-07-20T00:50:00.000Z", [4, 5]);
    const h = bucketize([...after, ...before], CADENCE); // deliberately unsorted

    // samples at 00:00 / 00:05 / 00:10, then nothing until 00:50 / 00:55
    expect(h.points).toHaveLength(12);
    const nulls = h.points.filter((p) => p.value === null);
    expect(nulls).toHaveLength(7);
    expect(h.gapBands).toEqual([{ fromIndex: 3, toIndex: 9 }]);

    // no interpolation and no zero-filling
    expect(h.points.map((p) => p.value)).toEqual([
      2, 3, 1, null, null, null, null, null, null, null, 4, 5,
    ]);
    expect(h.firstSeen).toBe("2026-07-20T00:00:00.000Z");
    expect(h.lastSeen).toBe("2026-07-20T00:55:00.000Z");
  });

  it("never extends the series past the last sample", () => {
    const h = bucketize(rowsEvery5Min("2026-07-20T00:00:00.000Z", [1, 2]), CADENCE);
    expect(h.points).toHaveLength(2);
    expect(h.lastSeen).toBe("2026-07-20T00:05:00.000Z");
    expect(h.gapBands).toEqual([]);
  });

  it("counts occupied snapshots without relabelling them visits", () => {
    const h = bucketize(
      rowsEvery5Min("2026-07-20T00:00:00.000Z", [0, 2, 0, 3, 0]),
      CADENCE,
    );
    expect(h.sampleCount).toBe(5);
    expect(h.occupiedCount).toBe(2);
    expect(h.peak).toBe(3);
    expect(occupiedLabel(h)).toBe("2 of 5");
  });

  it("reports peak as null when there is nothing to peak at", () => {
    const h = bucketize([], CADENCE);
    expect(h.peak).toBeNull();
    expect(h.points).toEqual([]);
    expect(h.firstSeen).toBeNull();
    expect(h.sampleCount).toBe(0);
  });

  it("keeps a real zero as a zero", () => {
    const h = bucketize(rowsEvery5Min("2026-07-20T00:00:00.000Z", [0, 0]), CADENCE);
    expect(h.points.map((p) => p.value)).toEqual([0, 0]);
    expect(h.peak).toBe(0);
    expect(h.occupiedCount).toBe(0);
  });

  it("drops rows with an unparseable timestamp rather than bucketing them at 1970", () => {
    const h = bucketize(
      [
        { taken_at: "", count: 9 },
        ...rowsEvery5Min("2026-07-20T00:00:00.000Z", [1]),
      ],
      CADENCE,
    );
    expect(h.sampleCount).toBe(1);
    expect(h.peak).toBe(1);
  });

  it("takes the later row when two land in one bucket \u{2014} not their maximum", () => {
    const h = bucketize(
      [
        { taken_at: "2026-07-20T00:00:10.000Z", count: 9 },
        { taken_at: "2026-07-20T00:04:00.000Z", count: 2 },
      ],
      CADENCE,
    );
    expect(h.points).toEqual([{ date: "2026-07-20T00:00:00.000Z", value: 2 }]);
  });
});

describe("loadWorldOccupancyHistory", () => {
  it("an empty history is no-sample with the reason, not a zero series", async () => {
    const d = await loadWorldOccupancyHistory("a.dcl.eth", 5000, {
      fetchImpl: (async () =>
        new Response(JSON.stringify({ world: "a.dcl.eth", history: [] }), {
          status: 200,
        })) as unknown as typeof fetch,
    });
    expect(d.state).toBe("no-sample");
    if (d.state !== "no-sample") throw new Error("unreachable");
    expect(d.note).toContain("has not been in the poll set");
    expect(Object.keys(d)).not.toContain("value");
  });

  it("a live history is sampled, dated by its newest row", async () => {
    const history = [
      { taken_at: "2026-08-01T09:50:32.602452Z", world_name: "a.dcl.eth", count: 2, live_users: 3 },
      { taken_at: "2026-08-01T09:45:32.602452Z", world_name: "a.dcl.eth", count: 1, live_users: 1 },
    ];
    const d = await loadWorldOccupancyHistory("a.dcl.eth", 5000, {
      fetchImpl: (async () =>
        new Response(JSON.stringify({ history }), { status: 200 })) as unknown as typeof fetch,
    });
    expect(d.state).toBe("sampled");
    if (d.state !== "sampled") throw new Error("unreachable");
    expect(d.takenAt).toBe("2026-08-01T09:50:32.602Z");
    expect(d.value.peak).toBe(2);
    expect(d.value.sampleCount).toBe(2);
  });

  it("a 500 degrades to unavailable, never to an empty chart", async () => {
    const d = await loadWorldOccupancyHistory("a.dcl.eth", 5000, {
      fetchImpl: (async () =>
        new Response("nope", { status: 500 })) as unknown as typeof fetch,
    });
    expect(d.state).toBe("unavailable");
    expect(Object.keys(d)).not.toContain("value");
  });
});
