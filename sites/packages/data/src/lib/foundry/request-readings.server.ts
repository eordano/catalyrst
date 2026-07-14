import type { Pool, PoolClient } from "pg";

import { getPool } from "./db.server";
import { askIdForUrl } from "./exchange.server";
import type { MarketCellConfidence, MarketCellSlug } from "./types";

// Ask readings: this program's own reading of each imported ask's stored text
// against the three gaming cells and six emotional jobs of the strategy deck
// (11.10, slides 09-10). Every row is a curated judgment made on readAt —
// never a fact the ask carries — and the rows are written only by
// foundry:import-request-readings from its fixture, mirroring the market-cell
// import: idempotent, content-columns-update-only. cell NULL = read, fits no
// single cell; shelfAnswer NULL = no game on the shelf answers it; no row =
// not read.

export type EmotionalJobLetter = "A" | "B" | "C" | "D" | "E" | "F";

export interface RequestReadingRow {
  /** The ask's public permalink — the same key the ask import derives its id
   *  from, so the reading can never attach to a different quote. */
  url: string;
  cell: MarketCellSlug | null;
  /** Comma-joined job letters ("A,C"), empty when the ask names none. */
  jobs: string;
  /** Scene id of the shelf game that answers the ask; null = none does. */
  shelfAnswer: string | null;
  rationale: string;
  confidence: MarketCellConfidence;
  readAt: string;
  basis: string;
  /** The deck's own per-cell crowd range (slide 09), verbatim minus the
   *  trailing sentence period; null when the reading fits no cell. */
  crowdRange: string | null;
}

/**
 * Upserts reading rows keyed by the ask id derived from each row's permalink.
 * Content columns only: a re-import refreshes the judgment in place and never
 * duplicates. A row whose ask is not in the exchange, or whose shelf answer
 * names a scene the registry lacks, is skipped with a warning rather than
 * silently nulled — a judgment is landed whole or not at all.
 */
export async function upsertRequestReadings(
  db: Pool | PoolClient,
  rows: RequestReadingRow[],
): Promise<{ upserted: number; skipped: string[] }> {
  let upserted = 0;
  const skipped: string[] = [];
  for (const row of rows) {
    const requestId = askIdForUrl(row.url);
    const ask = await db.query("SELECT 1 FROM foundry.request WHERE id = $1", [
      requestId,
    ]);
    if ((ask.rowCount ?? 0) === 0) {
      console.warn(
        `request-readings: no foundry.request row for "${row.url}" — reading skipped`,
      );
      skipped.push(row.url);
      continue;
    }
    if (row.shelfAnswer !== null) {
      const scene = await db.query("SELECT 1 FROM foundry.scene WHERE id = $1", [
        row.shelfAnswer,
      ]);
      if ((scene.rowCount ?? 0) === 0) {
        console.warn(
          `request-readings: shelf answer "${row.shelfAnswer}" is not in the scene registry — reading for "${row.url}" skipped`,
        );
        skipped.push(row.url);
        continue;
      }
    }
    await db.query(
      `INSERT INTO foundry.request_reading
         (request_id, cell, jobs, shelf_answer, rationale, confidence, read_at, basis, crowd_range)
       VALUES ($1, $2, $3, $4, $5, $6, $7::date, $8, $9)
       ON CONFLICT (request_id) DO UPDATE SET
         cell         = EXCLUDED.cell,
         jobs         = EXCLUDED.jobs,
         shelf_answer = EXCLUDED.shelf_answer,
         rationale    = EXCLUDED.rationale,
         confidence   = EXCLUDED.confidence,
         read_at      = EXCLUDED.read_at,
         basis        = EXCLUDED.basis,
         crowd_range  = EXCLUDED.crowd_range`,
      [
        requestId,
        row.cell,
        // Canonical ascending, de-duplicated — two authors of the same reading
        // store the same string (the CHECK alone permits 'C,A' and 'A,A').
        [...new Set(row.jobs.split(",").filter(Boolean))].sort().join(","),
        row.shelfAnswer,
        row.rationale,
        row.confidence,
        row.readAt,
        row.basis,
        row.crowdRange,
      ],
    );
    upserted += 1;
  }
  return { upserted, skipped };
}

export interface RequestReading {
  cell: MarketCellSlug | null;
  jobs: EmotionalJobLetter[];
  shelfAnswer: { sceneId: string; title: string } | null;
  rationale: string;
  confidence: MarketCellConfidence;
  readAt: string;
  basis: string;
  crowdRange: string | null;
}

function splitJobs(jobs: string): EmotionalJobLetter[] {
  return jobs === "" ? [] : (jobs.split(",") as EmotionalJobLetter[]);
}

/** The reading for one ask, shelf-answer title joined in; null = not read. */
export async function getRequestReading(
  requestId: string,
  db: Pool | PoolClient = getPool(),
): Promise<RequestReading | null> {
  const res = await db.query<{
    cell: MarketCellSlug | null;
    jobs: string;
    shelf_answer: string | null;
    scene_title: string | null;
    rationale: string;
    confidence: MarketCellConfidence;
    read_at: string;
    basis: string;
    crowd_range: string | null;
  }>(
    `SELECT rr.cell, rr.jobs, rr.shelf_answer, s.title AS scene_title,
            rr.rationale, rr.confidence,
            to_char(rr.read_at, 'YYYY-MM-DD') AS read_at, rr.basis,
            rr.crowd_range
       FROM foundry.request_reading rr
       LEFT JOIN foundry.scene s ON s.id = rr.shelf_answer
      WHERE rr.request_id = $1`,
    [requestId],
  );
  const row = res.rows[0];
  if (!row) return null;
  return {
    cell: row.cell,
    jobs: splitJobs(row.jobs),
    shelfAnswer:
      row.shelf_answer !== null
        ? { sceneId: row.shelf_answer, title: row.scene_title ?? row.shelf_answer }
        : null,
    rationale: row.rationale,
    confidence: row.confidence,
    readAt: row.read_at,
    basis: row.basis,
    crowdRange: row.crowd_range,
  };
}

/** A celled ask reading that names no shelf answer — demand read into a cell
 *  no deployed game covers, seen from the shelf's side. */
export type OpenGroundAsk = {
  requestId: string;
  title: string;
  cell: MarketCellSlug;
  readAt: string;
  confidence: MarketCellConfidence;
  crowdRange: string | null;
};

export async function listOpenGroundAsks(
  db: Pool | PoolClient = getPool(),
): Promise<OpenGroundAsk[]> {
  const res = await db.query<{
    request_id: string;
    title: string;
    cell: MarketCellSlug;
    confidence: MarketCellConfidence;
    read_at: string;
    crowd_range: string | null;
  }>(
    `SELECT rr.request_id, r.title, rr.cell, rr.confidence,
            to_char(rr.read_at, 'YYYY-MM-DD') AS read_at, rr.crowd_range
       FROM foundry.request_reading rr
       JOIN foundry.request r ON r.id = rr.request_id
      WHERE rr.cell IS NOT NULL AND rr.shelf_answer IS NULL
      ORDER BY rr.read_at, rr.request_id`,
  );
  return res.rows.map((r) => ({
    requestId: r.request_id,
    title: r.title,
    cell: r.cell,
    readAt: r.read_at,
    confidence: r.confidence,
    crowdRange: r.crowd_range,
  }));
}

/** An ask whose reading names the given emotional job among its letters. */
export type JobNamedAsk = {
  requestId: string;
  title: string;
  readAt: string;
  confidence: MarketCellConfidence;
};

export async function listAsksNamingJob(
  db: Pool | PoolClient,
  job: EmotionalJobLetter,
): Promise<JobNamedAsk[]> {
  const res = await db.query<{
    request_id: string;
    title: string;
    confidence: MarketCellConfidence;
    read_at: string;
  }>(
    `SELECT rr.request_id, r.title, rr.confidence,
            to_char(rr.read_at, 'YYYY-MM-DD') AS read_at
       FROM foundry.request_reading rr
       JOIN foundry.request r ON r.id = rr.request_id
      WHERE $1 = ANY(string_to_array(rr.jobs, ','))
      ORDER BY rr.read_at, rr.request_id`,
    [job],
  );
  return res.rows.map((r) => ({
    requestId: r.request_id,
    title: r.title,
    readAt: r.read_at,
    confidence: r.confidence,
  }));
}

/** One ask whose reading names a shelf game as its answer, seen from the
 *  game's side — the same judgment the ask page renders, read back. */
export type ShelfAnswerAsk = {
  requestId: string;
  title: string;
  readAt: string;
  /** Real pledge rows on the ask — the demand number the game can cite. */
  pledges: number;
};

export async function listShelfAnswerAsks(
  sceneId: string,
  db: Pool | PoolClient = getPool(),
): Promise<ShelfAnswerAsk[]> {
  const res = await db.query<{
    request_id: string;
    title: string;
    read_at: string;
    pledges: number;
  }>(
    `SELECT rr.request_id, r.title,
            to_char(rr.read_at, 'YYYY-MM-DD') AS read_at,
            (SELECT count(*)::int FROM foundry.pledge pl
              WHERE pl.request_id = rr.request_id) AS pledges
       FROM foundry.request_reading rr
       JOIN foundry.request r ON r.id = rr.request_id
      WHERE rr.shelf_answer = $1
      ORDER BY rr.read_at, rr.request_id`,
    [sceneId],
  );
  return res.rows.map((r) => ({
    requestId: r.request_id,
    title: r.title,
    readAt: r.read_at,
    pledges: Number(r.pledges),
  }));
}
