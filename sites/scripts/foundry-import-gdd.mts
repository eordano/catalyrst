#!/usr/bin/env node
// foundry-import-gdd — lands real design documents in foundry.gdd_doc.
//
//   npm run foundry:import-gdd -- <path.md>... [--workspace <dir>] [--db <url>]
//                                 [--payload-out <path>]
//
// Two sources, both real: vendored fixtures (documents written elsewhere and
// posted to Slack, each carrying its permalink in frontmatter) and the copilot
// workspace, scanned for the shortGDDs the skills have drafted there.
//
// The parse is structural, never editorial: honesty markers are counted per
// `## ` section, and hypothesis statuses are read out of experiment filenames —
// the state machine the skill maintains. No status, count or claim is ever
// inferred from prose.
//
// A document is linked to a game only by an explicit `scene_id` in its own
// frontmatter. Name-matching guesswork would attach the wrong doc to the wrong
// game the first time two titles rhymed.
//
// If the connection has no rights to write, the rows are saved as JSON and the
// script exits 3 rather than retrying.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import {
  readGddFile,
  scanWorkspace,
  upsertGddDoc,
  type ParsedGddDoc,
} from "../packages/data/src/lib/foundry/gdd.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const EXIT_PARKED = 3;

function flag(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
}

const FLAGS_WITH_VALUES = new Set(["--db", "--payload-out", "--workspace"]);

function positionals(): string[] {
  const args = process.argv.slice(2);
  const out: string[] = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (FLAGS_WITH_VALUES.has(arg)) {
      i += 1;
      continue;
    }
    if (arg.startsWith("--")) continue;
    out.push(arg);
  }
  return out;
}

const url =
  flag("db")?.trim() ||
  process.env.FOUNDRY_DATABASE_URL?.trim() ||
  process.env.CATALYST_DATABASE_URL?.trim();

if (!url) {
  console.error(
    "foundry-import-gdd: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  process.exit(1);
}

const workspace = flag("workspace")?.trim();
const files = positionals();

if (files.length === 0 && !workspace) {
  console.error(
    "foundry-import-gdd: name at least one .md file, or pass --workspace <dir> to scan a copilot workspace",
  );
  console.error(
    "  e.g. npm run foundry:import-gdd -- packages/data/src/fixtures/gdd/*.md",
  );
  process.exit(1);
}

// Sorted by version so a document that supersedes another lands after it: the
// foreign key is real and refuses the reverse order.
const docs: ParsedGddDoc[] = [
  ...files.map((f) => readGddFile(isAbsolute(f) ? f : join(process.cwd(), f))),
  ...(workspace ? scanWorkspace(workspace) : []),
].sort((a, b) => a.version - b.version || a.id.localeCompare(b.id));

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-import-gdd.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

function park(e: unknown): never {
  const out = payloadPath();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, `${JSON.stringify({ gdd_doc: docs }, null, 2)}\n`);
  console.error(`foundry-import-gdd: refused by the database — ${(e as Error).message}`);
  console.error(`foundry-import-gdd: ${docs.length} document(s) parked at ${out}`);
  process.exit(EXIT_PARKED);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  for (const doc of docs) {
    await upsertGddDoc(pool, doc);
    const t = doc.honesty.totals;
    console.log(
      `foundry-import-gdd: ${doc.id} — ${doc.honesty.sections.length} sections, ` +
        `${t.open} [OPEN] / ${t.tbd} TBD / ${t.hypothesis} [HYPOTHESIS] / ` +
        `${t.agentDecided} [agent-decided], ${doc.hypotheses.length} hypotheses`,
    );
  }
  console.log(`foundry-import-gdd: ${docs.length} document(s) upserted`);
} catch (e) {
  if (isPermissionDenied(e)) park(e);
  console.error("foundry-import-gdd: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
