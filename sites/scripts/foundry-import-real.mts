#!/usr/bin/env node
// foundry-import-real — brings the foundry database to the v3 shape and imports
// the real game registry into it.
//
//   npm run foundry:import-real -- [--db <url>] [--payload-out <path>] [--migrate-only]
//
// The registry is packages/data/src/fixtures/foundry-real.json: eight rows read
// out of our worlds mirror plus the SDK7 starter that lives in this repository.
// Nothing else is written — no requests, no pledges, no trajectories, no bot
// reports. Idempotent, so the main loop can run it on every deploy.
//
// If the connection has no rights to write, the rows it would have written are
// saved as JSON and the script exits 3 rather than retrying: an operator lands
// them later, and nothing is silently skipped. Set FOUNDRY_PENDING_INGEST_DIR
// to choose where those payloads go.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import {
  importReal,
  loadRealFixture,
  provisionFoundry,
  realImportPayload,
} from "../packages/data/src/lib/foundry/seed.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
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
    "foundry-import-real: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  console.error(
    '  dev: url="$(pg_tmp -t -w 3600)" && FOUNDRY_DATABASE_URL="$url" npm run foundry:import-real',
  );
  process.exit(1);
}

const migrateOnly = process.argv.includes("--migrate-only");

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-import-real.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  // 42501 insufficient_privilege, 42P01 undefined_table on a schema we cannot
  // create — both mean "this connection is not allowed to land these rows".
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

function park(e: unknown): never {
  const out = payloadPath();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(realImportPayload(), null, 2)}\n`);
  console.error(`foundry-import-real: refused by the database — ${(e as Error).message}`);
  console.error(`foundry-import-real: rows parked at ${out}`);
  console.error(
    "foundry-import-real: an operator with write rights can land them; nothing was partially applied by this script.",
  );
  process.exit(EXIT_PARKED);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  const applied = await provisionFoundry(pool);
  console.log(
    applied.length > 0
      ? `foundry-import-real: applied migrations ${applied.join(", ")}`
      : "foundry-import-real: schema already at v3",
  );

  if (migrateOnly) {
    console.log("foundry-import-real: --migrate-only, no rows imported");
  } else {
    const fixture = loadRealFixture();
    const summary = await importReal(pool, fixture);
    console.log(
      `foundry-import-real: ${summary.scenes} scenes upserted, ` +
        `${summary.changelog} new deployment entries ` +
        `(source: ${fixture.generatedFrom.source}, read ${fixture.generatedFrom.readAt})`,
    );

    const counts = await pool.query<{ scenes: number; entries: number }>(
      `SELECT (SELECT count(*)::int FROM foundry.scene)           AS scenes,
              (SELECT count(*)::int FROM foundry.scene_changelog) AS entries`,
    );
    console.log(
      `foundry-import-real: registry now holds ${counts.rows[0].scenes} scenes, ${counts.rows[0].entries} changelog entries`,
    );
  }
} catch (e) {
  if (isPermissionDenied(e)) park(e);
  console.error("foundry-import-real: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
