#!/usr/bin/env node
// foundry-bench-e2e — proves the bench→evidence loop end to end on a
// throwaway database: provisions the foundry schema, registers the three
// bench scenes, runs the REAL foundry-ingest-bench CLI over the given
// evidence directories, runs the REAL foundry-export-evidence CLI into a
// temp workspace, and prints what the readers and the evidence shelf say.
// Zero writes anywhere but the throwaway cluster and the temp dir.
//
//   scripts/e2e-pg.sh npx tsx scripts/foundry-bench-e2e.mts <evidence-dir>...
//
// Refuses to run without SITES_E2E_PG_URL so it can never be pointed at a
// real database by accident; e2e-pg.sh provides it.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import { benchChecklist, listBenchReports } from "../packages/data/src/lib/foundry/bench.server";
import { provisionFoundry } from "../packages/data/src/lib/foundry/seed.server";

const SITES = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

const url = process.env.SITES_E2E_PG_URL?.trim();
if (!url) {
  console.error(
    "foundry-bench-e2e: SITES_E2E_PG_URL is not set — run under the throwaway cluster:\n" +
      "  scripts/e2e-pg.sh npx tsx scripts/foundry-bench-e2e.mts <evidence-dir>...",
  );
  process.exit(2);
}

const dirs = process.argv.slice(2);
if (dirs.length === 0) {
  console.error("foundry-bench-e2e: pass the evidence directories foundry-bench-scenes.sh staged");
  process.exit(2);
}

const SCENES: Record<string, string> = {
  "relay-gardens": "Relay Gardens",
  "echo-duel": "Echo Duel",
  "caravan-ledger": "Caravan Ledger",
};

function slugOf(dir: string): string {
  const base = path.basename(path.resolve(dir));
  return /^(.*)-\d{6,}$/.exec(base)?.[1] ?? base;
}

const db = new Pool({ connectionString: url, max: 2 });
const migrations = await provisionFoundry(db);
for (const [id, title] of Object.entries(SCENES)) {
  await db.query(
    `INSERT INTO foundry.scene (id, title, source, source_note)
     VALUES ($1, $2, 'repo', 'foundry-bench-e2e fixture')
     ON CONFLICT (id) DO NOTHING`,
    [id, title],
  );
}
console.log(`provisioned foundry (${migrations.length} migration(s)) + ${Object.keys(SCENES).length} scene rows`);

for (const dir of dirs) {
  const manifest = path.join(SITES, "scripts", "bench", `${slugOf(dir)}.json`);
  const out = execFileSync(
    "npx",
    ["tsx", "scripts/foundry-ingest-bench.mts", dir, "--manifest", manifest, "--db", url],
    { cwd: SITES, encoding: "utf8" },
  );
  process.stdout.write(out);
}

const workspace = mkdtempSync(path.join(tmpdir(), "foundry-bench-e2e-"));
execFileSync("npx", ["tsx", "scripts/foundry-export-evidence.mts"], {
  cwd: SITES,
  encoding: "utf8",
  stdio: "inherit",
  env: { ...process.env, FOUNDRY_DATABASE_URL: url, FOUNDRY_COPILOT_DIRECTORY: workspace },
});

for (const id of Object.keys(SCENES)) {
  const reports = await listBenchReports(db, id);
  for (const r of reports) {
    console.log(
      `\n${id}: report ${r.id} runner=${r.runner} realm=${r.realm} verdict=${r.verdict} ` +
        `checks ${r.checksFailed}/${r.checksTotal} failed (${r.checksUnevaluable} unevaluable)`,
    );
    const rows = r.trajectoryId ? await benchChecklist(db, r.trajectoryId) : [];
    for (const c of rows) console.log(`  [${c.state}] ${c.kind}: ${c.detail}`);
  }
}

console.log(`\nevidence shelf at ${workspace}/projects/evidence:`);
console.log(readFileSync(path.join(workspace, "projects", "evidence", "INDEX.md"), "utf8"));
await db.end();
