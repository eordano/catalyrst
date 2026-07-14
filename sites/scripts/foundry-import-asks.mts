#!/usr/bin/env node
// foundry-import-asks — brings the foundry database to the current shape and
// lands the curated exchange asks into it.
//
//   npm run foundry:import-asks -- [--db <url>] [--payload-out <path>]
//
// The asks are packages/data/src/fixtures/foundry-asks.json: verbatim quotes
// from public forum posts, each carrying its author handle, permalink and
// original post date. They land as foundry.request rows with origin='import'
// and a NULL sid through upsertImportedAsks — the same path the e2e suite
// exercises. Idempotent: re-running refreshes the content columns and never
// touches a row's moderation status.
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
  upsertImportedAsks,
  type ImportedAsk,
} from "../packages/data/src/lib/foundry/exchange.server";
import { provisionFoundry } from "../packages/data/src/lib/foundry/seed.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const FIXTURE = join(SITES, "packages/data/src/fixtures/foundry-asks.json");
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
    "foundry-import-asks: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  console.error(
    '  dev: url="$(pg_tmp -t -w 3600)" && FOUNDRY_DATABASE_URL="$url" npm run foundry:import-asks',
  );
  process.exit(1);
}

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-import-asks.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  // 42501 insufficient_privilege, 42P01 undefined_table on a schema we cannot
  // create — both mean "this connection is not allowed to land these rows".
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

const asks = JSON.parse(readFileSync(FIXTURE, "utf8")) as ImportedAsk[];

function park(e: unknown): never {
  const out = payloadPath();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(asks, null, 2)}\n`);
  console.error(`foundry-import-asks: refused by the database — ${(e as Error).message}`);
  console.error(`foundry-import-asks: rows parked at ${out}`);
  console.error(
    "foundry-import-asks: an operator with write rights can land them; nothing was partially applied by this script.",
  );
  process.exit(EXIT_PARKED);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  const applied = await provisionFoundry(pool);
  console.log(
    applied.length > 0
      ? `foundry-import-asks: applied migrations ${applied.join(", ")}`
      : "foundry-import-asks: schema already current",
  );

  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const { upserted } = await upsertImportedAsks(client, asks);
    await client.query("COMMIT");
    console.log(`foundry-import-asks: ${upserted} asks upserted`);
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

  const counts = await pool.query<{ imported: number; total: number }>(
    `SELECT count(*) FILTER (WHERE origin = 'import')::int AS imported,
            count(*)::int AS total
       FROM foundry.request`,
  );
  console.log(
    `foundry-import-asks: the board now holds ${counts.rows[0].imported} imported asks of ${counts.rows[0].total} requests`,
  );
} catch (e) {
  if (isPermissionDenied(e)) park(e);
  console.error("foundry-import-asks: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
