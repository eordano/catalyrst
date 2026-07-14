import type { Pool, PoolClient } from "pg";

import type { MarketCellConfidence, MarketCellSlug } from "./types";

// Market-cell classifications: this program's own reading of each game's
// observable mechanics against the three gaming cells of the strategy deck's
// slide "09 | MARKET-CELL PORTFOLIO". Every row is a curated judgment made on
// classifiedAt — never a fact the deployment entity carries — and the rows are
// written only by foundry:import-market-cells from its fixture, mirroring the
// asks import: idempotent, content-columns-update-only.

export type { MarketCellConfidence, MarketCellSlug };

export interface MarketCellRow {
  sceneId: string;
  /** null = examined and honestly unclassifiable — the deck itself treats cell
   *  fit as a gate a concept can fail. */
  cell: MarketCellSlug | null;
  rationale: string;
  confidence: MarketCellConfidence;
  classifiedAt: string;
  basis: string;
}

/**
 * Upserts classification rows keyed by scene id. Content columns only: a
 * re-import refreshes the judgment in place and never duplicates. A row naming
 * a scene the registry does not hold is skipped with a warning rather than
 * failing the batch — the classification of the other games is not hostage to
 * one missing scene row.
 */
export async function upsertMarketCells(
  db: Pool | PoolClient,
  rows: MarketCellRow[],
): Promise<{ upserted: number; skipped: string[] }> {
  let upserted = 0;
  const skipped: string[] = [];
  for (const row of rows) {
    const scene = await db.query("SELECT 1 FROM foundry.scene WHERE id = $1", [
      row.sceneId,
    ]);
    if ((scene.rowCount ?? 0) === 0) {
      console.warn(
        `market-cells: no foundry.scene row for "${row.sceneId}" — classification skipped`,
      );
      skipped.push(row.sceneId);
      continue;
    }
    await db.query(
      `INSERT INTO foundry.scene_market_cell
         (scene_id, cell, rationale, confidence, classified_at, basis)
       VALUES ($1, $2, $3, $4, $5::date, $6)
       ON CONFLICT (scene_id) DO UPDATE SET
         cell          = EXCLUDED.cell,
         rationale     = EXCLUDED.rationale,
         confidence    = EXCLUDED.confidence,
         classified_at = EXCLUDED.classified_at,
         basis         = EXCLUDED.basis`,
      [row.sceneId, row.cell, row.rationale, row.confidence, row.classifiedAt, row.basis],
    );
    upserted += 1;
  }
  return { upserted, skipped };
}
