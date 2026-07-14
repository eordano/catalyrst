#!/usr/bin/env node
// foundry-import-emotional-jobs — brings the foundry database to the current
// shape and lands the emotional-job reads into it.
//
//   npm run foundry:import-emotional-jobs -- [--db <url>] [--payload-out <path>]
//
// The rows are packages/data/src/fixtures/foundry-emotional-jobs.json: this
// program's own curated judgments, dated 2026-08-16, reading each game's
// observable design against the six emotional jobs of the strategy deck's
// slide "10 | EMOTIONAL WHITE SPACE". A job of null is an honest verdict —
// read, and serves none of the six — not a gap. They land as
// foundry.scene_emotional_job rows through replaceSceneJobs and never touch a
// scene row. Idempotent: re-running replaces each scene's whole set, so a
// re-read that drops a job removes its row.
//
// If the connection has no rights to write, the rows it would have written are
// saved as JSON and the script exits 3 rather than retrying: an operator lands
// them later, and nothing is silently skipped. Set FOUNDRY_PENDING_INGEST_DIR
// to choose where those payloads go.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import {
  replaceSceneJobs,
  type EmotionalJobFixtureRow,
} from "../packages/data/src/lib/foundry/emotional-jobs.server";
import { provisionFoundry } from "../packages/data/src/lib/foundry/seed.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const FIXTURE = join(SITES, "packages/data/src/fixtures/foundry-emotional-jobs.json");
const EXIT_PARKED = 3;

function flag(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
}

const url =
  flag("db")?.trim() ||
  process.env.FOUNDRY_DATABASE_URL?.trim() ||
  process.env.CATALYST_DATABASE_URL?.trim();

if (!url) {
  console.error(
    "foundry-import-emotional-jobs: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  console.error(
    '  dev: url="$(pg_tmp -t -w 3600)" && FOUNDRY_DATABASE_URL="$url" npm run foundry:import-emotional-jobs',
  );
  process.exit(1);
}

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-import-emotional-jobs.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  // 42501 insufficient_privilege, 42P01 undefined_table on a schema we cannot
  // create — both mean "this connection is not allowed to land these rows".
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

const reads = JSON.parse(readFileSync(FIXTURE, "utf8")) as EmotionalJobFixtureRow[];

function park(e: unknown): never {
  const out = payloadPath();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(reads, null, 2)}\n`);
  console.error(
    `foundry-import-emotional-jobs: refused by the database — ${(e as Error).message}`,
  );
  console.error(`foundry-import-emotional-jobs: rows parked at ${out}`);
  console.error(
    "foundry-import-emotional-jobs: an operator with write rights can land them; nothing was partially applied by this script.",
  );
  process.exit(EXIT_PARKED);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  const applied = await provisionFoundry(pool);
  console.log(
    applied.length > 0
      ? `foundry-import-emotional-jobs: applied migrations ${applied.join(", ")}`
      : "foundry-import-emotional-jobs: schema already current",
  );

  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const { scenes, rows, skipped } = await replaceSceneJobs(client, reads);
    await client.query("COMMIT");
    console.log(
      `foundry-import-emotional-jobs: ${rows} rows replaced across ${scenes} scenes` +
        (skipped.length > 0 ? ` (${skipped.length} skipped: ${skipped.join(", ")})` : ""),
    );
  } catch (e) {
    try {
      await client.query("ROLLBACK");
    } catch {
      // the connection is already gone; the original error is the useful one
    }
    throw e;
  } finally {
    client.release();
  }

  const counts = await pool.query<{
    total: number;
    none_rows: number;
    scenes_read: number;
    job_a: number;
  }>(
    `SELECT count(*)::int AS total,
            count(*) FILTER (WHERE job IS NULL)::int AS none_rows,
            count(DISTINCT scene_id)::int AS scenes_read,
            count(*) FILTER (WHERE job = 'A')::int AS job_a
       FROM foundry.scene_emotional_job`,
  );
  const { total, none_rows, scenes_read, job_a } = counts.rows[0];
  console.log(
    `foundry-import-emotional-jobs: ${total} job rows across ${scenes_read} scenes read (${none_rows} read as serving none); job A — the deck's mandatory persistence job — is served by ${job_a} of them`,
  );
} catch (e) {
  if (isPermissionDenied(e)) park(e);
  console.error("foundry-import-emotional-jobs: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
