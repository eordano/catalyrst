import type { Pool } from "pg";

import type { FoundryScene, SceneSource } from "./types";

// The v3 registry is a read layer over rows written by `foundry:import-real`.
// Nothing here computes, projects or fills a gap: a column that is NULL in the
// worlds mirror stays NULL all the way to the page. `title` is the title the
// deployment entity carries — not a name this program chose.
const SCENE_COLUMNS = `s.id, s.title, s.world_name, s.entity_id,
       s.deployed_at, s.size_bytes, s.parcels, s.repo_path, s.bot_manifest,
       s.source, s.source_note, s.gdd_doc_id, s.created_at`;

type SceneDbRow = {
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
  };
}

/** Newest deployment first; the repo-only template sorts last, where it belongs. */
export async function listScenes(db: Pool): Promise<FoundryScene[]> {
  const res = await db.query<SceneDbRow>(
    `SELECT ${SCENE_COLUMNS}
       FROM foundry.scene s
      ORDER BY s.deployed_at DESC NULLS LAST, s.id`,
  );
  return res.rows.map(toScene);
}

/** `slug` is the primary key: scene ids are the slugs the import wrote. */
export async function getScene(db: Pool, slug: string): Promise<FoundryScene | null> {
  const res = await db.query<SceneDbRow>(
    `SELECT ${SCENE_COLUMNS} FROM foundry.scene s WHERE s.id = $1`,
    [slug],
  );
  const row = res.rows[0];
  return row ? toScene(row) : null;
}
