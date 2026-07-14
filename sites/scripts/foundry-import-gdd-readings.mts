#!/usr/bin/env node
// foundry-import-gdd-readings — brings the foundry database to the current
// shape and lands the doc↔game same-concept readings into it.
//
//   npm run foundry:import-gdd-readings -- [--db <url>] [--payload-out <path>]
//
// The rows are packages/data/src/fixtures/foundry-gdd-scene-readings.json:
// this program's own dated adjacency judgments that a design doc and a
// deployed game share a concept. A reading is NEVER written into
// scene.gdd_doc_id or gdd_doc.scene_id — no surface may claim the game
// implements the doc from one of these rows. They land through
// upsertGddSceneReadings; a row naming a doc or scene the registry lacks is
// skipped with a warning. Idempotent: re-running refreshes the content columns
// in place.
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
  upsertGddSceneReadings,
  type GddSceneReadingRow,
} from "../packages/data/src/lib/foundry/gdd.server";
import { provisionFoundry } from "../packages/data/src/lib/foundry/seed.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const FIXTURE = join(
  SITES,
  "packages/data/src/fixtures/foundry-gdd-scene-readings.json",
);
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
    "foundry-import-gdd-readings: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  console.error(
    '  dev: url="$(pg_tmp -t -w 3600)" && FOUNDRY_DATABASE_URL="$url" npm run foundry:import-gdd-readings',
  );
  process.exit(1);
}

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-import-gdd-readings.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  // 42501 insufficient_privilege, 42P01 undefined_table on a schema we cannot
  // create — both mean "this connection is not allowed to land these rows".
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

const readings = JSON.parse(readFileSync(FIXTURE, "utf8")) as GddSceneReadingRow[];

function park(e: unknown): never {
  const out = payloadPath();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(readings, null, 2)}\n`);
  console.error(
    `foundry-import-gdd-readings: refused by the database — ${(e as Error).message}`,
  );
  console.error(`foundry-import-gdd-readings: rows parked at ${out}`);
  console.error(
    "foundry-import-gdd-readings: an operator with write rights can land them; nothing was partially applied by this script.",
  );
  process.exit(EXIT_PARKED);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  const applied = await provisionFoundry(pool);
  console.log(
    applied.length > 0
      ? `foundry-import-gdd-readings: applied migrations ${applied.join(", ")}`
      : "foundry-import-gdd-readings: schema already current",
  );

  const { upserted, skipped } = await upsertGddSceneReadings(pool, readings);
  console.log(
    `foundry-import-gdd-readings: ${upserted} readings upserted` +
      (skipped.length > 0 ? ` (${skipped.length} skipped: ${skipped.join(", ")})` : ""),
  );
} catch (e) {
  if (isPermissionDenied(e)) park(e);
  console.error("foundry-import-gdd-readings: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
