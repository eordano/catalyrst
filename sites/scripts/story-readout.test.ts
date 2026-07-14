import { describe, expect, it, vi } from "vitest";

import { buildCountsSql, mapSqlRowsToCounts } from "./story-readout";

describe("buildCountsSql", () => {
  it("filters by source/exp_key and an escaped event IN-list, grouped by variant+event", () => {
    const sql = buildCountsSql("gv_vote_flow", [
      "experiment_exposed",
      "gv_vote_completed",
    ]);
    expect(sql).toContain("source = 'segment'");
    expect(sql).toContain("body->'properties'->>'exp_key' = 'gv_vote_flow'");
    expect(sql).toContain(
      "body->>'event' IN ('experiment_exposed', 'gv_vote_completed')",
    );
    expect(sql).toContain("GROUP BY 1, 2");
  });

  it("doubles single quotes in exp_key and event names (no injection)", () => {
    const sql = buildCountsSql("o'brien", ["a'b"]);
    expect(sql).toContain("= 'o''brien'");
    expect(sql).toContain("IN ('a''b')");
  });

  it("throws when exp_key or an event contains ';'", () => {
    expect(() => buildCountsSql("x; DROP", ["e"])).toThrow();
    expect(() => buildCountsSql("ok", ["e; DELETE"])).toThrow();
  });
});

describe("mapSqlRowsToCounts", () => {
  it("maps {variant,event,c} rows into the Counts shape", () => {
    const counts = mapSqlRowsToCounts([
      { variant: "control", event: "experiment_exposed", c: 3 },
      { variant: "control", event: "gv_vote_completed", c: 1 },
      { variant: "guided", event: "experiment_exposed", c: 3 },
      { variant: "guided", event: "gv_vote_completed", c: 2 },
    ]);
    expect(counts).toEqual({
      control: { experiment_exposed: 3, gv_vote_completed: 1 },
      guided: { experiment_exposed: 3, gv_vote_completed: 2 },
    });
  });

  it("skips rows with empty/missing variant or event and returns {} for no rows", () => {
    expect(
      mapSqlRowsToCounts([
        { variant: "", event: "x", c: 5 },
        { variant: "v", event: null, c: 5 },
      ]),
    ).toEqual({});
    expect(mapSqlRowsToCounts([])).toEqual({});
  });
});

describe("collectCounts via POST /dash/sql (mocked fetch)", () => {
  it("posts the built SQL and maps the response into Counts", async () => {
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body));
      expect(init?.method).toBe("POST");
      expect(String(_url)).toMatch(/\/dash\/sql$/);
      expect(body.sql).toContain("body->'properties'->>'exp_key' = 'demo_key'");
      return new Response(
        JSON.stringify({
          columns: ["c", "event", "variant"],
          rows: [
            { variant: "control", event: "experiment_exposed", c: 2 },
            { variant: "control", event: "hit", c: 1 },
          ],
          truncated: false,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    });
    const prev = process.env.TELEMETRY_URL;
    process.env.TELEMETRY_URL = "http://telemetry.test";
    try {
      const sql = buildCountsSql("demo_key", ["experiment_exposed", "hit"]);
      const res = await fetchMock("http://telemetry.test/dash/sql", {
        method: "POST",
        body: JSON.stringify({ sql }),
      });
      const payload = (await res.json()) as { rows: Array<Record<string, unknown>> };
      expect(mapSqlRowsToCounts(payload.rows)).toEqual({
        control: { experiment_exposed: 2, hit: 1 },
      });
    } finally {
      if (prev === undefined) delete process.env.TELEMETRY_URL;
      else process.env.TELEMETRY_URL = prev;
    }
  });
});
