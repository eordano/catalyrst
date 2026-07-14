import type { Pool, PoolClient } from "pg";

import type { EmotionalJob, MarketCellConfidence, SceneEmotionalJobRead } from "./types";

// Emotional-job reads: this program's own reading of each game's observable
// design against the six emotional jobs of the strategy deck's slide
// "10 | EMOTIONAL WHITE SPACE". Every row is a curated judgment made on
// readAt — never a fact the deployment entity carries — and the rows are
// written only by foundry:import-emotional-jobs from its fixture. Unlike the
// market-cell upsert, a re-import REPLACES a scene's whole set: the fixture is
// the whole judgment, so a re-read that drops a job must remove its row.

export type { EmotionalJob, SceneEmotionalJobRead };

export interface EmotionalJobFixtureRow {
  sceneId: string;
  /** null = read, and honestly serves none of the six jobs. */
  job: EmotionalJob | null;
  rationale: string;
  confidence: MarketCellConfidence;
  readAt: string;
  basis: string;
}

/**
 * Replaces each named scene's job set with the fixture's rows: delete then
 * insert, so a re-read that drops a job removes its row rather than leaving a
 * stale judgment behind. Rows naming a scene the registry does not hold are
 * skipped with a warning rather than failing the batch — the reading of the
 * other games is not hostage to one missing scene row. The caller wraps the
 * whole call in a transaction, so a scene is never left half-replaced.
 */
export async function replaceSceneJobs(
  db: Pool | PoolClient,
  rows: EmotionalJobFixtureRow[],
): Promise<{ scenes: number; rows: number; skipped: string[] }> {
  const byScene = new Map<string, EmotionalJobFixtureRow[]>();
  for (const row of rows) {
    const list = byScene.get(row.sceneId);
    if (list) list.push(row);
    else byScene.set(row.sceneId, [row]);
  }

  let scenes = 0;
  let inserted = 0;
  const skipped: string[] = [];
  for (const [sceneId, sceneRows] of byScene) {
    const scene = await db.query("SELECT 1 FROM foundry.scene WHERE id = $1", [sceneId]);
    if ((scene.rowCount ?? 0) === 0) {
      console.warn(
        `emotional-jobs: no foundry.scene row for "${sceneId}" — reading skipped`,
      );
      skipped.push(sceneId);
      continue;
    }
    await db.query("DELETE FROM foundry.scene_emotional_job WHERE scene_id = $1", [
      sceneId,
    ]);
    for (const row of sceneRows) {
      await db.query(
        `INSERT INTO foundry.scene_emotional_job
           (scene_id, job, rationale, confidence, read_at, basis)
         VALUES ($1, $2, $3, $4, $5::date, $6)`,
        [sceneId, row.job, row.rationale, row.confidence, row.readAt, row.basis],
      );
      inserted += 1;
    }
    scenes += 1;
  }
  return { scenes, rows: inserted, skipped };
}

/**
 * The distinct jobs served anywhere in the registry, whether any reading
 * exists at all, and how many scenes carry a reading — the data behind a
 * "what no game here serves yet" line. Zero rows means the registry has never
 * been read: the caller renders that absence rather than an all-six gap list
 * nobody measured.
 */
export async function listServedJobs(
  db: Pool | PoolClient,
): Promise<{ anyRead: boolean; served: EmotionalJob[]; scenesRead: number }> {
  const res = await db.query(
    `SELECT count(*)::int AS total,
            count(DISTINCT scene_id)::int AS scenes_read,
            array_agg(DISTINCT job) FILTER (WHERE job IS NOT NULL) AS served
       FROM foundry.scene_emotional_job`,
  );
  const row = res.rows[0] as
    | { total: number; scenes_read: number; served: EmotionalJob[] | null }
    | undefined;
  return {
    anyRead: Number(row?.total ?? 0) > 0,
    served: row?.served ?? [],
    scenesRead: Number(row?.scenes_read ?? 0),
  };
}

/**
 * One scene's reading, jobs first and the "serves none" row (job NULL) last.
 * An empty array means the scene has never been read — nothing is fabricated
 * to fill it.
 */
export async function listSceneJobs(
  db: Pool | PoolClient,
  sceneId: string,
): Promise<SceneEmotionalJobRead[]> {
  const res = await db.query(
    `SELECT job, rationale, confidence,
            to_char(read_at, 'YYYY-MM-DD') AS read_at, basis
       FROM foundry.scene_emotional_job
      WHERE scene_id = $1
      ORDER BY job NULLS LAST`,
    [sceneId],
  );
  return res.rows.map(
    (r: {
      job: EmotionalJob | null;
      rationale: string;
      confidence: MarketCellConfidence;
      read_at: string;
      basis: string;
    }) => ({
      job: r.job,
      rationale: r.rationale,
      confidence: r.confidence,
      readAt: r.read_at,
      basis: r.basis,
    }),
  );
}
