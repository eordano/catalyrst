#!/usr/bin/env node
// foundry-import-market-cells — brings the foundry database to the current
// shape and lands the market-cell classifications into it.
//
//   npm run foundry:import-market-cells -- [--db <url>] [--payload-out <path>]
//
// The rows are packages/data/src/fixtures/foundry-market-cells.json: this
// program's own curated judgments, dated 2026-08-16, reading each game's
// observable mechanics against the three gaming cells of the strategy deck's
// slide "09 | MARKET-CELL PORTFOLIO". A cell of null is an honest verdict —
// examined and unclassifiable — not a gap. They land as
// foundry.scene_market_cell rows through upsertMarketCells and never touch a
// scene row. Idempotent: re-running refreshes the content columns in place.
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
  upsertMarketCells,
  type MarketCellRow,
} from "../packages/data/src/lib/foundry/market-cells.server";
import { provisionFoundry } from "../packages/data/src/lib/foundry/seed.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const FIXTURE = join(SITES, "packages/data/src/fixtures/foundry-market-cells.json");
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
    "foundry-import-market-cells: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  console.error(
    '  dev: url="$(pg_tmp -t -w 3600)" && FOUNDRY_DATABASE_URL="$url" npm run foundry:import-market-cells',
  );
  process.exit(1);
}

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-import-market-cells.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  // 42501 insufficient_privilege, 42P01 undefined_table on a schema we cannot
  // create — both mean "this connection is not allowed to land these rows".
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

const cells = JSON.parse(readFileSync(FIXTURE, "utf8")) as MarketCellRow[];

function park(e: unknown): never {
  const out = payloadPath();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify(cells, null, 2)}\n`);
  console.error(
    `foundry-import-market-cells: refused by the database — ${(e as Error).message}`,
  );
  console.error(`foundry-import-market-cells: rows parked at ${out}`);
  console.error(
    "foundry-import-market-cells: an operator with write rights can land them; nothing was partially applied by this script.",
  );
  process.exit(EXIT_PARKED);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  const applied = await provisionFoundry(pool);
  console.log(
    applied.length > 0
      ? `foundry-import-market-cells: applied migrations ${applied.join(", ")}`
      : "foundry-import-market-cells: schema already current",
  );

  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const { upserted, skipped } = await upsertMarketCells(client, cells);
    await client.query("COMMIT");
    console.log(
      `foundry-import-market-cells: ${upserted} classifications upserted` +
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

  const counts = await pool.query<{ total: number; in_cell: number; scenes: number }>(
    `SELECT count(*)::int AS total,
            count(*) FILTER (WHERE cell IS NOT NULL)::int AS in_cell,
            (SELECT count(*)::int FROM foundry.scene) AS scenes
       FROM foundry.scene_market_cell`,
  );
  const { total, in_cell, scenes } = counts.rows[0];
  console.log(
    `foundry-import-market-cells: the registry now holds ${total} classifications (${in_cell} into a cell, ${total - in_cell} unclassified) of ${scenes} scenes`,
  );
} catch (e) {
  if (isPermissionDenied(e)) park(e);
  console.error("foundry-import-market-cells: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
