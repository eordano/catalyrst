#!/usr/bin/env node
// foundry-export-demand — writes the exchange's open asks into the copilot
// workspace so a sandboxed session can start from demand.
//
//   npm run foundry:export-demand -- [--db <url>] [--workspace <dir>]
//
// The sandbox has no database and no network beyond the model gateway; the one
// way demand reaches it is as files on the workspace bind. This script renders
// projects/demand/INDEX.md plus one <ask-id>.md per OPEN ask, ordered exactly
// as the board orders them (pledges first, then source recency) so the copilot
// and the site never disagree about what ranks first. Stale files for asks no
// longer open are removed. No sid ever reaches a file: imported asks carry
// their public author handle, visitor asks the claimed persona name or the
// honest "a visitor".
//
// Intended cadence: the same timer as the usage ingest, so pledge counts stay
// fresh while a session runs (the workspace bind is live).
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { Pool } from "pg";

function flag(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
}

const dbUrl = flag("db") ?? process.env.FOUNDRY_DATABASE_URL;
if (!dbUrl) {
  console.error("foundry-export-demand: FOUNDRY_DATABASE_URL is not set");
  process.exit(2);
}
const workspace = flag("workspace") ?? process.env.FOUNDRY_COPILOT_DIRECTORY;
if (!workspace) {
  console.error("foundry-export-demand: FOUNDRY_COPILOT_DIRECTORY is not set");
  process.exit(2);
}

interface DemandRow {
  id: string;
  title: string;
  body: string;
  author: string;
  origin: string;
  source_url: string | null;
  pledges: number;
  cell: string | null;
  briefs: number;
}

const pool = new Pool({ connectionString: dbUrl });
const res = await pool.query<DemandRow>(
  `SELECT r.id, r.title, r.body, r.origin, r.source_url,
          CASE
            WHEN r.origin = 'import' THEN r.source
            ELSE COALESCE(p.display_name, 'a visitor')
          END AS author,
          (SELECT count(*)::int FROM foundry.pledge pl
            WHERE pl.request_id = r.id) AS pledges,
          rr.cell AS cell,
          (SELECT count(*)::int FROM foundry.gdd_doc d
            WHERE d.grounding_request_ids @> to_jsonb(r.id)) AS briefs
     FROM foundry.request r
     LEFT JOIN foundry.persona p ON p.sid = r.sid
     LEFT JOIN foundry.request_reading rr ON rr.request_id = r.id
    WHERE r.status = 'open'
    ORDER BY pledges DESC, COALESCE(r.sourced_at, r.created_at) DESC, r.id`,
);
await pool.end();

const dir = join(workspace, "projects", "demand");
mkdirSync(dir, { recursive: true });

const stamp = new Date().toISOString();
const lines: string[] = [
  "# Demand shelf",
  "",
  `Written by the program from the exchange's stored asks at ${stamp}.`,
  "Ordered as the board orders them: pledges first, then source recency.",
  "Each row's file carries the full ask. `briefs` counts design docs whose",
  "stored grounding keys already name the ask — 0 means nobody has drafted",
  "from this demand yet.",
  "",
  "| rank | file | title | pledges | briefs | cell |",
  "|---|---|---|---|---|---|",
];
for (const [i, r] of res.rows.entries()) {
  lines.push(
    `| ${i + 1} | ${r.id}.md | ${r.title.replace(/\|/g, "\\|")} | ${r.pledges} | ${r.briefs} | ${r.cell ?? "—"} |`,
  );
}
writeFileSync(join(dir, "INDEX.md"), lines.join("\n") + "\n");

const wanted = new Set(["INDEX.md"]);
for (const r of res.rows) {
  const name = `${r.id}.md`;
  wanted.add(name);
  const doc = [
    "---",
    `id: ${r.id}`,
    `title: ${JSON.stringify(r.title)}`,
    `author: ${JSON.stringify(r.author)}`,
    `origin: ${r.origin}`,
    `pledges: ${r.pledges}`,
    `briefs: ${r.briefs}`,
    ...(r.cell ? [`cell: ${r.cell}`] : []),
    ...(r.source_url ? [`source_url: ${r.source_url}`] : []),
    "---",
    "",
    `# ${r.title}`,
    "",
    r.body.trim(),
    "",
  ].join("\n");
  writeFileSync(join(dir, name), doc);
}
for (const f of readdirSync(dir)) {
  if (!wanted.has(f)) rmSync(join(dir, f));
}
console.log(`foundry-export-demand: ${res.rows.length} open asks -> ${dir}`);
