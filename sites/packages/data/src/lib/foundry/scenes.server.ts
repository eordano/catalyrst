import type { Pool } from "pg";

import type {
  FoundryScene,
  MarketCellConfidence,
  MarketCellSlug,
  SceneMarketCell,
  SceneSource,
} from "./types";

// The v3 registry is a read layer over rows written by `foundry:import-real`.
// Nothing here computes, projects or fills a gap: a column that is NULL in the
// worlds mirror stays NULL all the way to the page. `title` is the title the
// deployment entity carries — not a name this program chose. The market-cell
// join reads this program's own classification rows (foundry:import-market-cells)
// and carries them as what they are, never as deployment facts; the doc link is
// operator-set or the newest doc declaring this scene.
const SCENE_COLUMNS = `s.id, s.title, s.world_name, s.entity_id,
       s.deployed_at, s.size_bytes, s.parcels, s.repo_path, s.bot_manifest,
       s.source, s.source_note,
       coalesce(s.gdd_doc_id, (SELECT d.id FROM foundry.gdd_doc d
          WHERE d.scene_id = s.id
          ORDER BY d.created_at DESC LIMIT 1)) AS gdd_doc_id,
       s.created_at,
       s.description, s.thumbnail_url,
       mc.cell AS mc_cell, mc.rationale AS mc_rationale,
       mc.confidence AS mc_confidence,
       to_char(mc.classified_at, 'YYYY-MM-DD') AS mc_classified_at,
       mc.basis AS mc_basis`;

const SCENE_JOINS = `LEFT JOIN foundry.scene_market_cell mc ON mc.scene_id = s.id`;

export type SceneMarketCellColumns = {
  mc_cell: string | null;
  mc_rationale: string | null;
  mc_confidence: string | null;
  mc_classified_at: string | null;
  mc_basis: string | null;
};

type SceneDbRow = SceneMarketCellColumns & {
  id: string;
  title: string;
  world_name: string | null;
  entity_id: string | null;
  deployed_at: Date | string | null;
  size_bytes: string | number | null;
  parcels: number | null;
  repo_path: string | null;
  bot_manifest: string | null;
  source: string;
  source_note: string | null;
  gdd_doc_id: string | null;
  created_at: Date | string | null;
  description: string | null;
  thumbnail_url: string | null;
};

function iso(value: Date | string | null): string | null {
  if (value === null) return null;
  return value instanceof Date ? value.toISOString() : String(value);
}

function num(value: string | number | null): number | null {
  if (value === null) return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

function source(value: string): SceneSource {
  return value === "worlds-mirror" ? "worlds-mirror" : "repo";
}

/**
 * The LEFT-JOIN row back into the honest shape: no classification row at all
 * means null ("not yet read"), while a row whose `cell` is NULL stays a row —
 * "read, and honestly unclassifiable". rationale/confidence/basis are NOT NULL
 * in the table, so their absence is the row's absence.
 */
export function sceneMarketCell(r: SceneMarketCellColumns): SceneMarketCell | null {
  if (
    r.mc_rationale === null ||
    r.mc_confidence === null ||
    r.mc_classified_at === null ||
    r.mc_basis === null
  ) {
    return null;
  }
  return {
    cell: r.mc_cell as MarketCellSlug | null,
    rationale: r.mc_rationale,
    confidence: r.mc_confidence as MarketCellConfidence,
    classifiedAt: r.mc_classified_at,
    basis: r.mc_basis,
  };
}

function toScene(r: SceneDbRow): FoundryScene {
  return {
    id: r.id,
    title: r.title,
    worldName: r.world_name,
    entityId: r.entity_id,
    deployedAt: iso(r.deployed_at),
    sizeBytes: num(r.size_bytes),
    parcels: r.parcels === null ? null : Number(r.parcels),
    repoPath: r.repo_path,
    botManifest: r.bot_manifest,
    source: source(r.source),
    sourceNote: r.source_note ?? "",
    gddDocId: r.gdd_doc_id,
    importedAt: iso(r.created_at),
    description: r.description,
    thumbnailUrl: r.thumbnail_url,
    marketCell: sceneMarketCell(r),
  };
}

/** Newest deployment first; the repo-only template sorts last, where it belongs. */
export async function listScenes(db: Pool): Promise<FoundryScene[]> {
  const res = await db.query<SceneDbRow>(
    `SELECT ${SCENE_COLUMNS}
       FROM foundry.scene s
       ${SCENE_JOINS}
      ORDER BY s.deployed_at DESC NULLS LAST, s.id`,
  );
  return res.rows.map(toScene);
}

export interface SceneChangelogRow {
  at: string;
  origin: "import" | "visitor";
  note: string;
  sourceNote: string;
}

/** The scene's recorded life, oldest first — rows the import and visitors
 *  wrote, verbatim. */
export async function listSceneChangelog(
  db: Pool,
  sceneId: string,
): Promise<SceneChangelogRow[]> {
  const res = await db.query<{
    at: Date | string;
    origin: string;
    note: string;
    source_note: string;
  }>(
    `SELECT at, origin, note, source_note
       FROM foundry.scene_changelog
      WHERE scene_id = $1
      ORDER BY at, id`,
    [sceneId],
  );
  return res.rows.map((r) => ({
    at: iso(r.at) ?? "",
    origin: r.origin === "visitor" ? "visitor" : "import",
    note: r.note,
    sourceNote: r.source_note,
  }));
}

/** `slug` is the primary key: scene ids are the slugs the import wrote. */
export async function getScene(db: Pool, slug: string): Promise<FoundryScene | null> {
  const res = await db.query<SceneDbRow>(
    `SELECT ${SCENE_COLUMNS} FROM foundry.scene s ${SCENE_JOINS} WHERE s.id = $1`,
    [slug],
  );
  const row = res.rows[0];
  return row ? toScene(row) : null;
}

/** scene_id → the most recent RECORDED activity instant across the sources
 *  that write about a scene (worlds-mirror changelog entries and bot-report
 *  runs). Deploy time is compared in by the caller from the scene row itself.
 *  Scenes with no recorded activity are absent — never an invented date. */
export async function lastMovedByScene(db: Pool): Promise<Map<string, string>> {
  // Scene MOVEMENT only: redeploys, steward notes, stewardship changes. The
  // harness's own bot runs are the program learning about a static scene, not
  // the scene moving — counting them meant every "moved" date on the live
  // shelf was the program's own activity (observed 2026-08-20; the deck's
  // guardrail: no synthetic audience masquerading as demand). Run activity
  // still shows honestly through the runs chip and its dated verdict.
  const res = await db.query<{ scene_id: string; at: Date | string }>(
    `SELECT scene_id, max(at) AS at FROM (
       SELECT scene_id, at FROM foundry.scene_changelog
       UNION ALL
       SELECT subject AS scene_id, at FROM foundry.action_log
        WHERE action = ANY('{claim_steward,release_steward,offer_transfer,revoke_transfer,accept_transfer}')
     ) t GROUP BY scene_id`,
  );
  const out = new Map<string, string>();
  for (const r of res.rows) {
    out.set(
      r.scene_id,
      r.at instanceof Date ? r.at.toISOString() : String(r.at),
    );
  }
  return out;
}
