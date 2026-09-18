import { describe, expect, it } from "vitest";

import { buildCountsSql, mapSqlRowsToCounts } from "./story-readout";

describe("buildCountsSql", () => {
  it("filters by source/exp_key and a quote-escaped event IN-list grouped by variant+event, and refuses ';'", () => {
    const sql = buildCountsSql("gv_vote_flow", ["experiment_exposed", "gv_vote_completed"]);
    expect(sql).toContain("source = 'segment'");
    expect(sql).toContain("body->'properties'->>'exp_key' = 'gv_vote_flow'");
    expect(sql).toContain("body->>'event' IN ('experiment_exposed', 'gv_vote_completed')");
    expect(sql).toContain("GROUP BY 1, 2");

    const escaped = buildCountsSql("o'brien", ["a'b"]);
    expect(escaped).toContain("= 'o''brien'");
    expect(escaped).toContain("IN ('a''b')");

    expect(() => buildCountsSql("x; DROP", ["e"])).toThrow();
    expect(() => buildCountsSql("ok", ["e; DELETE"])).toThrow();
  });
});

describe("mapSqlRowsToCounts", () => {
  it("maps {variant,event,c} rows into the Counts shape and skips rows missing a variant or event", () => {
    expect(
      mapSqlRowsToCounts([
        { variant: "control", event: "experiment_exposed", c: 3 },
        { variant: "control", event: "gv_vote_completed", c: 1 },
        { variant: "guided", event: "experiment_exposed", c: 3 },
        { variant: "guided", event: "gv_vote_completed", c: 2 },
      ]),
    ).toEqual({
      control: { experiment_exposed: 3, gv_vote_completed: 1 },
      guided: { experiment_exposed: 3, gv_vote_completed: 2 },
    });
    expect(
      mapSqlRowsToCounts([
        { variant: "", event: "x", c: 5 },
        { variant: "v", event: null, c: 5 },
      ]),
    ).toEqual({});
    expect(mapSqlRowsToCounts([])).toEqual({});
  });
});
