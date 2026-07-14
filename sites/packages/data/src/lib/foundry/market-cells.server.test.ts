import { describe, expect, it, vi } from "vitest";

import type { Pool } from "pg";

import { upsertMarketCells, type MarketCellRow } from "./market-cells.server";
import { sceneMarketCell } from "./scenes.server";

// Enough of Postgres to exercise the upsert path in memory: scene-existence
// lookups answer from a fixed registry, and the ON CONFLICT upsert is modelled
// as a map keyed by scene_id — the same key the real table's PK enforces.
function stubDb(sceneIds: string[]) {
  const stored = new Map<string, unknown[]>();
  const db = {
    query: vi.fn(async (sql: string, values: unknown[] = []) => {
      if (/SELECT 1 FROM foundry\.scene/.test(sql)) {
        return { rowCount: sceneIds.includes(values[0] as string) ? 1 : 0, rows: [] };
      }
      if (/INSERT INTO foundry\.scene_market_cell/.test(sql)) {
        stored.set(values[0] as string, values);
        return { rowCount: 1, rows: [] };
      }
      throw new Error(`unexpected SQL: ${sql}`);
    }),
  };
  return { db: db as unknown as Pool, stored };
}

function row(overrides: Partial<MarketCellRow>): MarketCellRow {
  return {
    sceneId: "flagtag",
    cell: "creator-led-social-competition",
    rationale: "A rationale grounded in observed mechanics.",
    confidence: "inferred",
    classifiedAt: "2026-08-16",
    basis: "This program's own reading — not a fact the deployment entity carries.",
    ...overrides,
  };
}

describe("upsertMarketCells", () => {
  it("upserts by scene id — a re-run refreshes in place, never duplicates", async () => {
    const { db, stored } = stubDb(["flagtag", "fortunes"]);
    const rows = [row({}), row({ sceneId: "fortunes", cell: null })];

    const first = await upsertMarketCells(db, rows);
    expect(first).toEqual({ upserted: 2, skipped: [] });
    expect(stored.size).toBe(2);

    const second = await upsertMarketCells(
      db,
      rows.map((r) => ({ ...r, rationale: "A revised reading." })),
    );
    expect(second.upserted).toBe(2);
    expect(stored.size).toBe(2);
    expect(stored.get("flagtag")?.[2]).toBe("A revised reading.");
  });

  it("skips a row naming a scene the registry does not hold, with a warning", async () => {
    const { db, stored } = stubDb(["flagtag"]);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const out = await upsertMarketCells(db, [row({}), row({ sceneId: "not-a-game" })]);
      expect(out).toEqual({ upserted: 1, skipped: ["not-a-game"] });
      expect(stored.has("not-a-game")).toBe(false);
      expect(warn).toHaveBeenCalledOnce();
    } finally {
      warn.mockRestore();
    }
  });

  it("stores an unclassifiable verdict as a row with a NULL cell, not as absence", async () => {
    const { db, stored } = stubDb(["fortunes"]);
    await upsertMarketCells(db, [row({ sceneId: "fortunes", cell: null })]);
    expect(stored.get("fortunes")?.[1]).toBeNull();
  });
});

describe("sceneMarketCell", () => {
  const joined = {
    mc_cell: "creator-led-social-competition",
    mc_rationale: "A rationale.",
    mc_confidence: "evidence-backed",
    mc_classified_at: "2026-08-16",
    mc_basis: "This program's own reading.",
  };

  it("maps a joined classification row through verbatim", () => {
    expect(sceneMarketCell(joined)).toEqual({
      cell: "creator-led-social-competition",
      rationale: "A rationale.",
      confidence: "evidence-backed",
      classifiedAt: "2026-08-16",
      basis: "This program's own reading.",
    });
  });

  it("keeps 'read, unclassifiable' (cell NULL) distinct from 'not yet read' (no row)", () => {
    // A row with a NULL cell is still a judgment — it survives as an object.
    expect(sceneMarketCell({ ...joined, mc_cell: null })).toEqual({
      cell: null,
      rationale: "A rationale.",
      confidence: "evidence-backed",
      classifiedAt: "2026-08-16",
      basis: "This program's own reading.",
    });
    // No row at all (every LEFT-JOIN column NULL) is null — nothing fabricated.
    expect(
      sceneMarketCell({
        mc_cell: null,
        mc_rationale: null,
        mc_confidence: null,
        mc_classified_at: null,
        mc_basis: null,
      }),
    ).toBeNull();
  });
});
