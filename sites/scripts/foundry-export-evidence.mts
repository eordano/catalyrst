#!/usr/bin/env node
// foundry-export-evidence — writes each scene's measured outcomes into the
// copilot workspace, so a drafting session can reconcile a revision against
// what actually happened instead of revising blind.
//
//   npm run foundry:export-evidence -- [--db <url>] [--workspace <dir>]
//
// The sandbox has no database; evidence reaches it as files on the live
// workspace bind, the same way the demand shelf does. Every value is read
// through the site's own readers — the same bench checklist, hypothesis
// rows, approvals and signals the pages render — so the copilot and the site
// can never disagree about what the evidence says. A scene with no evidence
// gets no file; a reading that fails says so instead of pretending absence.
// No sid ever reaches a file: names, 4-hex badges and counts only.
//
// Intended cadence: the ingest timer, third ExecStart.
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { Pool } from "pg";

import {
  benchChecklist,
  listBenchReports,
} from "../packages/data/src/lib/foundry/bench.server";
import {
  approvalsForDoc,
  type GddApprovalRecord,
} from "../packages/data/src/lib/foundry/gdd-approve.server";
import {
  asksByIds,
  docEditor,
  listGddDocs,
  getGddDoc,
  readingsForDocs,
} from "../packages/data/src/lib/foundry/gdd.server";
import {
  getResponseSignalsPool,
  isResponseSignalsConfigured,
  readResponseSignals,
} from "../packages/data/src/lib/foundry/response.server";
import { listScenes } from "../packages/data/src/lib/foundry/scenes.server";
import { listTrajectories } from "../packages/data/src/lib/foundry/trajectory.server";

function flag(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
}

const dbUrl = flag("db") ?? process.env.FOUNDRY_DATABASE_URL;
if (!dbUrl) {
  console.error("foundry-export-evidence: FOUNDRY_DATABASE_URL is not set");
  process.exit(2);
}
const workspace = flag("workspace") ?? process.env.FOUNDRY_COPILOT_DIRECTORY;
if (!workspace) {
  console.error("foundry-export-evidence: FOUNDRY_COPILOT_DIRECTORY is not set");
  process.exit(2);
}

const db = new Pool({ connectionString: dbUrl });
const dir = join(workspace, "projects", "evidence");
mkdirSync(dir, { recursive: true });

const scenes = await listScenes(db);
const docs = await listGddDocs(db);
const stamp = new Date().toISOString();

const indexRows: string[] = [];
const wanted = new Set(["INDEX.md"]);

for (const scene of scenes) {
  const linkedDocs = docs.filter(
    (d) => d.sceneId === scene.id || scene.gddDocId === d.id,
  );
  const reports = await listBenchReports(db, scene.id);
  const trajectories = await listTrajectories(db, { sceneId: scene.id });
  const readings = await readingsForDocs(
    db,
    docs.map((d) => d.id),
  ).then((rows) => rows.filter((r) => r.sceneId === scene.id));

  if (linkedDocs.length === 0 && reports.length === 0 && readings.length === 0) {
    continue;
  }

  const lines: string[] = [
    "---",
    `scene: ${scene.id}`,
    `title: ${JSON.stringify(scene.title)}`,
    ...(linkedDocs.length > 0
      ? [`docs: [${linkedDocs.map((d) => d.id).join(", ")}]`]
      : []),
    `exported_at: ${stamp}`,
    "---",
    "",
    `# Evidence — ${scene.title}`,
    "",
    "Measured outcomes only, read through the site's own readers. Nothing",
    "here is a judgment; reconcile the doc's hypothesis log against it and",
    "say when a row cannot move because its instrument does not exist yet.",
    "",
  ];

  lines.push("## Bench runs");
  if (reports.length === 0) {
    lines.push("", "No recorded runs. A run that never happened is nothing.", "");
  } else {
    for (const r of reports) {
      const label =
        r.runner === "arena"
          ? `sandbox simulation (exit: ${r.verdict ?? "not recorded"})`
          : (r.verdict ?? "no recorded verdict");
      lines.push("", `### ${r.ranAt} — ${label}`, "");
      if (r.runner === "arena") {
        lines.push(
          "A sandbox simulation: the exit status is not a verdict on the game.",
          "",
        );
        continue;
      }
      const checks = r.trajectoryId ? await benchChecklist(db, r.trajectoryId) : [];
      if (checks.length === 0) {
        lines.push("No per-check breakdown recorded.", "");
      } else {
        lines.push("| check | state | detail |", "|---|---|---|");
        for (const c of checks) {
          lines.push(
            `| ${(c.why || c.kind).replace(/\|/g, "\\|")} | ${c.state} | ${c.detail.replace(/\|/g, "\\|")} |`,
          );
        }
        lines.push("");
      }
    }
  }

  for (const doc of linkedDocs) {
    const full = await getGddDoc(db, doc.id);
    if (!full) continue;
    const approvals: GddApprovalRecord[] = await approvalsForDoc(db, doc.id);
    const editor = full.source === "session" ? await docEditor(db, doc.id) : null;
    lines.push(`## Doc ${doc.id} (v${doc.version}, ${doc.kind})`, "");
    lines.push(
      approvals.length === 0
        ? "No person has approved this version."
        : `Approved by ${approvals.map((a) => `${a.name} (${a.at.slice(0, 10)})`).join(", ")}.`,
    );
    if (full.source === "session") {
      lines.push(`Last edited by ${editor ?? "a visitor"}.`);
    }
    lines.push("", "### Hypothesis log (stored)", "");
    if (full.hypotheses.length === 0) {
      lines.push("No hypotheses stored.", "");
    } else {
      lines.push("| id | status | tested on | if/then |", "|---|---|---|---|");
      for (const h of full.hypotheses) {
        lines.push(
          `| ${h.id} | ${h.status} | ${h.testedOn ?? "not yet"} | ${(h.ifThen ?? "").replace(/\|/g, "\\|").slice(0, 160)} |`,
        );
      }
      lines.push("");
    }
    if (full.groundingRequestIds.length > 0) {
      const asks = await asksByIds(db, full.groundingRequestIds);
      const counts = await db.query<{ request_id: string; n: number }>(
        `SELECT request_id, count(*)::int AS n FROM foundry.pledge
          WHERE request_id = ANY($1) GROUP BY request_id`,
        [full.groundingRequestIds],
      );
      const byId = new Map(counts.rows.map((r) => [r.request_id, Number(r.n)]));
      lines.push("### Demand it answers", "");
      for (const a of asks) {
        lines.push(`- "${a.title}" — ${byId.get(a.id) ?? 0} pledge(s)`);
      }
      lines.push("");
    }
  }

  if (readings.length > 0) {
    lines.push("## Same-concept readings", "");
    for (const r of readings) {
      lines.push(
        `- ${r.docTitle} v${r.docVersion} read as same-concept on ${r.readAt} (${r.confidence}): ${r.rationale}`,
      );
    }
    lines.push("");
  }

  lines.push("## Response signals");
  if (!isResponseSignalsConfigured()) {
    lines.push("", "Signal counts are not connected.", "");
  } else {
    try {
      const s = await readResponseSignals(getResponseSignalsPool(), {
        slug: scene.id,
        trajectoryIds: trajectories.map((t) => t.id),
      });
      lines.push(
        "",
        `${s.distinctVisitors} distinct browser session(s); ${s.visitEvents} visit event(s); ` +
          `${s.replays.reduce((n, r) => n + r.opens, 0)} replay open(s); ${s.downloads} scene-memory bundle download(s).`,
        "A session is a browser cookie — the program cannot yet tell a person from its own automation.",
        "",
      );
    } catch {
      lines.push("", "Signal counts could not be read.", "");
    }
  }

  const name = `${scene.id}.md`;
  wanted.add(name);
  writeFileSync(join(dir, name), lines.join("\n"));
  indexRows.push(
    `| ${name} | ${scene.title.replace(/\|/g, "\\|")} | ${reports.length} | ${linkedDocs.map((d) => d.id).join("<br>") || "—"} |`,
  );
}

writeFileSync(
  join(dir, "INDEX.md"),
  [
    "# Evidence shelf",
    "",
    `Written by the program from its own stored readings at ${stamp}.`,
    "One file per scene that has any evidence. Doc ids in the last column",
    "map a design doc to its scene's file.",
    "",
    "| file | scene | runs | docs |",
    "|---|---|---|---|",
    ...indexRows,
    "",
  ].join("\n"),
);

for (const f of readdirSync(dir)) {
  if (!wanted.has(f)) rmSync(join(dir, f));
}
await db.end();
console.log(`foundry-export-evidence: ${indexRows.length} scene(s) -> ${dir}`);
