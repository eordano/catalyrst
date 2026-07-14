import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it, vi } from "vitest";

import type { Pool } from "pg";

import { askIdForUrl, type ImportedAsk } from "./exchange.server";
import {
  getRequestReading,
  listShelfAnswerAsks,
  upsertRequestReadings,
  type RequestReadingRow,
} from "./request-readings.server";

const FIXTURES = fileURLToPath(new URL("../../fixtures/", import.meta.url));

// Enough of Postgres to exercise the upsert path in memory: ask- and
// scene-existence lookups answer from fixed registries, and the ON CONFLICT
// upsert is modelled as a map keyed by request_id — the same key the real
// table's PK enforces.
function stubDb(askUrls: string[], sceneIds: string[]) {
  const askIds = askUrls.map(askIdForUrl);
  const stored = new Map<string, unknown[]>();
  const db = {
    query: vi.fn(async (sql: string, values: unknown[] = []) => {
      if (/SELECT 1 FROM foundry\.request/.test(sql)) {
        return { rowCount: askIds.includes(values[0] as string) ? 1 : 0, rows: [] };
      }
      if (/SELECT 1 FROM foundry\.scene/.test(sql)) {
        return { rowCount: sceneIds.includes(values[0] as string) ? 1 : 0, rows: [] };
      }
      if (/INSERT INTO foundry\.request_reading/.test(sql)) {
        stored.set(values[0] as string, values);
        return { rowCount: 1, rows: [] };
      }
      if (/WHERE rr\.shelf_answer = \$1/.test(sql)) {
        // The real query JOINs foundry.request for the title; the stub
        // registry knows only ids, so the title mirrors the request id.
        const rows = [...stored.values()]
          .filter((row) => row[3] === (values[0] as string))
          .map((row) => ({
            request_id: row[0],
            title: row[0],
            read_at: row[6],
            // The real query subselects a pledge count; the stub holds none.
            pledges: 0,
          }))
          .sort((a, b) =>
            `${a.read_at}${a.request_id}` < `${b.read_at}${b.request_id}` ? -1 : 1,
          );
        return { rowCount: rows.length, rows };
      }
      if (/FROM foundry\.request_reading/.test(sql)) {
        const row = stored.get(values[0] as string);
        if (!row) return { rowCount: 0, rows: [] };
        const shelf = row[3] as string | null;
        return {
          rowCount: 1,
          rows: [
            {
              cell: row[1],
              jobs: row[2],
              shelf_answer: shelf,
              // The real query LEFT-joins foundry.scene for the title; the
              // stub registry knows only ids, so the title mirrors the id.
              scene_title: shelf !== null && sceneIds.includes(shelf) ? shelf : null,
              rationale: row[4],
              confidence: row[5],
              read_at: row[6],
              basis: row[7],
              crowd_range: row[8],
            },
          ],
        };
      }
      throw new Error(`unexpected SQL: ${sql}`);
    }),
  };
  return { db: db as unknown as Pool, stored };
}

const GOLF_URL = "https://forum.decentraland.org/t/dao-01b1065-what-about-golfcraft/24930";
const LOBBY_URL =
  "https://www.reddit.com/r/decentraland/comments/s0m01z/what_are_you_actively_playing_on_decentraland_i/";

function row(overrides: Partial<RequestReadingRow>): RequestReadingRow {
  return {
    url: GOLF_URL,
    cell: "creator-led-social-competition",
    jobs: "F",
    shelfAnswer: null,
    rationale: "A rationale quoting the ask's stored text.",
    confidence: "inferred",
    readAt: "2026-08-17",
    basis: "This program's own reading — not a fact the ask carries.",
    crowdRange: "6–24 active players + spectators",
    ...overrides,
  };
}

describe("upsertRequestReadings", () => {
  it("upserts by the ask id derived from the permalink — a re-run refreshes in place", async () => {
    const { db, stored } = stubDb([GOLF_URL, LOBBY_URL], ["flagtag"]);
    const rows = [
      row({}),
      row({ url: LOBBY_URL, cell: null, jobs: "", shelfAnswer: "flagtag" }),
    ];

    const first = await upsertRequestReadings(db, rows);
    expect(first).toEqual({ upserted: 2, skipped: [] });
    expect(stored.size).toBe(2);
    expect(stored.has(askIdForUrl(GOLF_URL))).toBe(true);

    const second = await upsertRequestReadings(
      db,
      rows.map((r) => ({ ...r, rationale: "A revised reading." })),
    );
    expect(second.upserted).toBe(2);
    expect(stored.size).toBe(2);
    expect(stored.get(askIdForUrl(GOLF_URL))?.[4]).toBe("A revised reading.");
  });

  it("skips a reading whose ask is not in the exchange, with a warning", async () => {
    const { db, stored } = stubDb([GOLF_URL], []);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const out = await upsertRequestReadings(db, [
        row({}),
        row({ url: "https://example.com/never-imported" }),
      ]);
      expect(out).toEqual({
        upserted: 1,
        skipped: ["https://example.com/never-imported"],
      });
      expect(stored.has(askIdForUrl("https://example.com/never-imported"))).toBe(false);
      expect(warn).toHaveBeenCalledOnce();
    } finally {
      warn.mockRestore();
    }
  });

  it("skips a reading whose shelf answer names a scene the registry lacks — never nulls a judgment", async () => {
    const { db, stored } = stubDb([GOLF_URL], ["flagtag"]);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const out = await upsertRequestReadings(db, [
        row({ shelfAnswer: "not-a-game" }),
      ]);
      expect(out).toEqual({ upserted: 0, skipped: [GOLF_URL] });
      expect(stored.size).toBe(0);
      expect(warn).toHaveBeenCalledOnce();
    } finally {
      warn.mockRestore();
    }
  });
});

describe("getRequestReading", () => {
  it("round-trips a stored row, splitting the jobs letters", async () => {
    const { db } = stubDb([GOLF_URL], ["flagtag"]);
    await upsertRequestReadings(db, [
      row({ jobs: "A,C", shelfAnswer: "flagtag" }),
    ]);
    const reading = await getRequestReading(askIdForUrl(GOLF_URL), db);
    expect(reading).toEqual({
      cell: "creator-led-social-competition",
      jobs: ["A", "C"],
      shelfAnswer: { sceneId: "flagtag", title: "flagtag" },
      rationale: "A rationale quoting the ask's stored text.",
      confidence: "inferred",
      readAt: "2026-08-17",
      basis: "This program's own reading — not a fact the ask carries.",
      crowdRange: "6–24 active players + spectators",
    });
  });

  it("keeps a null-cell row as a judgment (empty jobs stay []), and no row is null", async () => {
    const { db } = stubDb([GOLF_URL], []);
    await upsertRequestReadings(db, [
      row({ cell: null, jobs: "", crowdRange: null }),
    ]);
    const reading = await getRequestReading(askIdForUrl(GOLF_URL), db);
    expect(reading?.cell).toBeNull();
    expect(reading?.jobs).toEqual([]);
    expect(reading?.shelfAnswer).toBeNull();
    // No cell, no range: the deck's per-cell comparator does not apply.
    expect(reading?.crowdRange).toBeNull();

    expect(await getRequestReading("ask-0000000000000000", db)).toBeNull();
  });
});

describe("listShelfAnswerAsks", () => {
  it("returns the asks whose reading names the game, and [] for a game no reading names", async () => {
    const { db } = stubDb([GOLF_URL, LOBBY_URL], ["flagtag", "fastlane"]);
    await upsertRequestReadings(db, [
      row({ shelfAnswer: "fastlane" }),
      row({
        url: LOBBY_URL,
        cell: null,
        jobs: "",
        shelfAnswer: "flagtag",
        crowdRange: null,
      }),
    ]);

    expect(await listShelfAnswerAsks("flagtag", db)).toEqual([
      {
        requestId: askIdForUrl(LOBBY_URL),
        title: askIdForUrl(LOBBY_URL),
        readAt: "2026-08-17",
        pledges: 0,
      },
    ]);
    expect(await listShelfAnswerAsks("fastlane", db)).toEqual([
      {
        requestId: askIdForUrl(GOLF_URL),
        title: askIdForUrl(GOLF_URL),
        readAt: "2026-08-17",
        pledges: 0,
      },
    ]);
    expect(await listShelfAnswerAsks("skychaser", db)).toEqual([]);
  });
});

describe("the readings fixture", () => {
  const readings = JSON.parse(
    readFileSync(`${FIXTURES}foundry-request-readings.json`, "utf8"),
  ) as RequestReadingRow[];
  const asks = JSON.parse(
    readFileSync(`${FIXTURES}foundry-asks.json`, "utf8"),
  ) as ImportedAsk[];
  const scenes = (
    JSON.parse(readFileSync(`${FIXTURES}foundry-real.json`, "utf8")) as {
      scenes: { id: string }[];
    }
  ).scenes;

  it("reads every imported ask exactly once, by its verbatim permalink", () => {
    const askIds = new Set(asks.map((a) => askIdForUrl(a.url)));
    const readIds = readings.map((r) => askIdForUrl(r.url));
    expect(new Set(readIds).size).toBe(readings.length);
    for (const [i, id] of readIds.entries()) {
      expect(askIds.has(id), `readings[${i}] url matches no imported ask`).toBe(true);
    }
    expect(readings.length).toBe(asks.length);
  });

  it("names only shelf scenes the registry holds, and keeps rationales chip-sized", () => {
    const sceneIds = new Set(scenes.map((s) => s.id));
    for (const r of readings) {
      if (r.shelfAnswer !== null) {
        expect(sceneIds.has(r.shelfAnswer), `${r.shelfAnswer} not in the registry`).toBe(
          true,
        );
      }
      expect(r.rationale.length).toBeLessThanOrEqual(140);
      expect(r.jobs).toMatch(/^([A-F](,[A-F])*)?$/);
    }
  });

  it("carries the deck's slide-09 range on every cell reading, and none on a cell-null one", () => {
    // The deck's own strings, verbatim minus the trailing sentence period. The
    // labs cell's "2–12 contributors + observers" has no ask yet, so no row
    // may carry it — a range is never rendered without a reading behind it.
    const DECK_RANGES: Record<string, string> = {
      "creator-led-social-competition": "6–24 active players + spectators",
      "community-operated-game-clubs": "8–50 recurring participants",
    };
    for (const r of readings) {
      if (r.cell === null) {
        expect(r.crowdRange, `${r.url} fits no cell — no range applies`).toBeNull();
      } else {
        expect(r.crowdRange, `${r.url} carries its cell's deck range`).toBe(
          DECK_RANGES[r.cell],
        );
      }
    }
  });
});
