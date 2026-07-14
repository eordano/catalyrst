import { describe, expect, it, vi } from "vitest";

import type { Pool } from "pg";

import {
  listSceneJobs,
  replaceSceneJobs,
  type EmotionalJobFixtureRow,
} from "./emotional-jobs.server";

// Enough of Postgres to exercise the replace-set path in memory: scene-existence
// lookups answer from a fixed registry, DELETE clears a scene's stored rows, and
// INSERT appends — the same replace-then-insert the real import runs in one tx.
function stubDb(sceneIds: string[]) {
  const stored = new Map<string, unknown[][]>();
  const db = {
    query: vi.fn(async (sql: string, values: unknown[] = []) => {
      if (/SELECT 1 FROM foundry\.scene/.test(sql)) {
        return { rowCount: sceneIds.includes(values[0] as string) ? 1 : 0, rows: [] };
      }
      if (/DELETE FROM foundry\.scene_emotional_job/.test(sql)) {
        stored.delete(values[0] as string);
        return { rowCount: 0, rows: [] };
      }
      if (/INSERT INTO foundry\.scene_emotional_job/.test(sql)) {
        const rows = stored.get(values[0] as string) ?? [];
        rows.push(values);
        stored.set(values[0] as string, rows);
        return { rowCount: 1, rows: [] };
      }
      if (/FROM foundry\.scene_emotional_job/.test(sql)) {
        const rows = (stored.get(values[0] as string) ?? []).map((v) => ({
          job: v[1],
          rationale: v[2],
          confidence: v[3],
          // The real query formats read_at with to_char; the stub hands the
          // stored value back under the aliased column name.
          read_at: v[4],
          basis: v[5],
        }));
        return { rowCount: rows.length, rows };
      }
      throw new Error(`unexpected SQL: ${sql}`);
    }),
  };
  return { db: db as unknown as Pool, stored };
}

function row(overrides: Partial<EmotionalJobFixtureRow>): EmotionalJobFixtureRow {
  return {
    sceneId: "flagtag",
    job: "E",
    rationale: "A rationale grounded in observed design.",
    confidence: "inferred",
    readAt: "2026-08-16",
    basis: "This program's own reading — not a fact the deployment entity carries.",
    ...overrides,
  };
}

describe("replaceSceneJobs", () => {
  it("replaces a scene's whole set — a re-read that drops a job removes its row", async () => {
    const { db, stored } = stubDb(["flagtag"]);

    const first = await replaceSceneJobs(db, [row({ job: "E" }), row({ job: "F" })]);
    expect(first).toEqual({ scenes: 1, rows: 2, skipped: [] });
    expect(stored.get("flagtag")).toHaveLength(2);

    const second = await replaceSceneJobs(db, [row({ job: "E" })]);
    expect(second).toEqual({ scenes: 1, rows: 1, skipped: [] });
    expect(stored.get("flagtag")).toHaveLength(1);
    expect(stored.get("flagtag")?.[0]?.[1]).toBe("E");
  });

  it("skips a row naming a scene the registry does not hold, with a warning", async () => {
    const { db, stored } = stubDb(["flagtag"]);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      const out = await replaceSceneJobs(db, [
        row({}),
        row({ sceneId: "not-a-game", job: "D" }),
      ]);
      expect(out).toEqual({ scenes: 1, rows: 1, skipped: ["not-a-game"] });
      expect(stored.has("not-a-game")).toBe(false);
      expect(warn).toHaveBeenCalledOnce();
    } finally {
      warn.mockRestore();
    }
  });

  it("stores 'read, serves none' as a row with a NULL job, not as absence", async () => {
    const { db, stored } = stubDb(["cleantheclub"]);
    await replaceSceneJobs(db, [row({ sceneId: "cleantheclub", job: null })]);
    expect(stored.get("cleantheclub")).toHaveLength(1);
    expect(stored.get("cleantheclub")?.[0]?.[1]).toBeNull();
  });
});

describe("listSceneJobs", () => {
  it("round-trips a none-row and maps read_at through as readAt", async () => {
    const { db } = stubDb(["cleantheclub"]);
    await replaceSceneJobs(db, [row({ sceneId: "cleantheclub", job: null })]);

    const reads = await listSceneJobs(db, "cleantheclub");
    expect(reads).toEqual([
      {
        job: null,
        rationale: "A rationale grounded in observed design.",
        confidence: "inferred",
        readAt: "2026-08-16",
        basis: "This program's own reading — not a fact the deployment entity carries.",
      },
    ]);
  });

  it("returns an empty array for a scene never read — nothing fabricated", async () => {
    const { db } = stubDb(["flagtag"]);
    expect(await listSceneJobs(db, "flagtag")).toEqual([]);
  });
});
