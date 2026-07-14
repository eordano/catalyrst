import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

import type { Pool } from "pg";

import { runMigrations, type FoundryMigration } from "./db.server";
import type {
  FoundryRealFixture,
  FoundryRealSceneRow,
  ImportSummary,
} from "./types";

// The file name is v2's; the contents are not. There is no seed any more — this
// module provisions the schema and imports a registry that was read out of the
// worlds mirror. It is reachable only from scripts and the e2e harness, which
// is why it may read files off disk.

const SCHEMA = fileURLToPath(new URL("./schema.sql", import.meta.url));
const MIGRATIONS_DIR = fileURLToPath(new URL("./migrations", import.meta.url));
const FIXTURE = fileURLToPath(
  new URL("../../fixtures/foundry-real.json", import.meta.url),
);

const DEPLOY_NOTE = "Deployed to Worlds";

export function loadRealFixture(): FoundryRealFixture {
  return JSON.parse(readFileSync(FIXTURE, "utf8")) as FoundryRealFixture;
}

export function loadMigrations(): FoundryMigration[] {
  return readdirSync(MIGRATIONS_DIR)
    .filter((f) => f.endsWith(".sql"))
    .sort()
    .map((f) => ({
      name: f.replace(/\.sql$/, ""),
      sql: readFileSync(`${MIGRATIONS_DIR}/${f}`, "utf8"),
    }));
}

export async function applyFoundrySchema(pool: Pool): Promise<void> {
  await pool.query(readFileSync(SCHEMA, "utf8"));
}

export async function migrateFoundry(pool: Pool): Promise<string[]> {
  return runMigrations(pool, loadMigrations());
}

/**
 * Brings a database to the v3 shape and leaves it EMPTY.
 *
 * Migrations run first so a live v2 database sheds its fiction before
 * schema.sql tries to create v3 tables on top of v2 column types; on a database
 * that never had v2 the migration returns immediately and schema.sql does the
 * whole install.
 */
export async function provisionFoundry(pool: Pool): Promise<string[]> {
  const applied = await migrateFoundry(pool);
  await applyFoundrySchema(pool);
  return applied;
}

/** The rows importReal would write, without touching a database. */
export function realImportPayload(
  fixture: FoundryRealFixture = loadRealFixture(),
): {
  generatedFrom: FoundryRealFixture["generatedFrom"];
  scene: FoundryRealSceneRow[];
  scene_changelog: {
    scene_id: string;
    at: string;
    note: string;
    source_note: string;
    origin: "import";
  }[];
} {
  return {
    generatedFrom: fixture.generatedFrom,
    scene: fixture.scenes,
    scene_changelog: fixture.scenes
      .filter((s) => s.deployedAt !== null && s.entityId !== null)
      .map((s) => ({
        scene_id: s.id,
        at: s.deployedAt as string,
        note: DEPLOY_NOTE,
        source_note: `worlds mirror entity ${s.entityId as string}`,
        origin: "import" as const,
      })),
  };
}

/**
 * Upserts the registry and one changelog entry per real deployment.
 *
 * Writes `scene` and `scene_changelog` and nothing else: requests, pledges,
 * trajectories, bot reports and llm usage all have real writers of their own,
 * and an import that invented any of them would be the exact thing v3 exists to
 * remove. Idempotent — re-running updates the mirror-derived columns in place
 * and never duplicates a changelog row, and it leaves `gdd_doc_id` alone so an
 * operator link survives the next import.
 */
export async function importReal(
  pool: Pool,
  fixture: FoundryRealFixture = loadRealFixture(),
): Promise<ImportSummary> {
  let scenes = 0;
  let changelog = 0;

  for (const s of fixture.scenes) {
    await pool.query(
      `INSERT INTO foundry.scene
         (id, title, world_name, entity_id, deployed_at, size_bytes, parcels,
          repo_path, bot_manifest, source, source_note)
       VALUES ($1,$2,$3,$4,$5::timestamptz,$6,$7,$8,$9,$10,$11)
       ON CONFLICT (id) DO UPDATE SET
         title        = EXCLUDED.title,
         world_name   = EXCLUDED.world_name,
         entity_id    = EXCLUDED.entity_id,
         deployed_at  = EXCLUDED.deployed_at,
         size_bytes   = EXCLUDED.size_bytes,
         parcels      = EXCLUDED.parcels,
         repo_path    = EXCLUDED.repo_path,
         bot_manifest = EXCLUDED.bot_manifest,
         source       = EXCLUDED.source,
         source_note  = EXCLUDED.source_note`,
      [
        s.id,
        s.title,
        s.worldName,
        s.entityId,
        s.deployedAt,
        s.sizeBytes,
        s.parcels,
        s.repoPath,
        s.botManifest,
        s.source,
        s.sourceNote,
      ],
    );
    scenes += 1;

    if (s.deployedAt === null || s.entityId === null) continue;

    const inserted = await pool.query(
      `INSERT INTO foundry.scene_changelog (scene_id, at, note, source_note, origin)
       SELECT $1, $2::timestamptz, $3, $4, 'import'
       WHERE NOT EXISTS (
         SELECT 1 FROM foundry.scene_changelog
          WHERE scene_id = $1 AND at = $2::timestamptz AND note = $3
       )`,
      [s.id, s.deployedAt, DEPLOY_NOTE, `worlds mirror entity ${s.entityId}`],
    );
    changelog += inserted.rowCount ?? 0;
  }

  return { scenes, changelog };
}

/** Provision + import, for fresh installs and the e2e harness. */
export async function bootstrapFoundry(pool: Pool): Promise<ImportSummary> {
  await provisionFoundry(pool);
  return importReal(pool);
}
