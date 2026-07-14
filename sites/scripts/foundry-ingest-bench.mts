#!/usr/bin/env node
// foundry-ingest-bench.mts — turns one real dcl-scene-bots run into one foundry
// episode: a `trajectory` with its append-only events plus the `bot_report` that
// links them, written in a single transaction.
//
// It reads only what the harness itself wrote — `snapshot.json`, the runner's
// stdout (`run.log`), and the `shots/` directory — and never invents a step, a
// duration or a verdict. A run whose stdout was not captured lands with
// verdict NULL, and the bench page says "snapshot only".
//
//   npm run foundry:ingest-bench -- <evidence-dir> [options]
//
//     --manifest <path>     games/<name>.json, stored verbatim as the tool call
//     --runner <r>          dclbots (default) | arena
//     --slug <slug>         report slug (default: manifest slug / dir prefix)
//     --scene <id>          foundry.scene id to attach to (default: the slug)
//     --log <path>          runner stdout (default: <evidence-dir>/run.log)
//     --exit-code <n>       the runner's exit code (arena: decides the verdict)
//     --ran-at <iso>        when the run happened (default: snapshot/file mtime)
//     --db <url>            postgres url (default $FOUNDRY_DATABASE_URL)
//     --payload-out <path>  where to park the payload if the DB refuses the write
//     --dry-run             build and park the payload, write nothing
//
// Exit codes: 0 written (or parked on --dry-run), 1 bad input, 3 the database
// refused the write and the payload was parked.
import { existsSync, readFileSync, readdirSync, statSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

import { Pool } from "pg";

import {
  buildBenchIngest,
  ingestBenchRun,
  type BenchEvidence,
  type BenchRunner,
  type BenchSnapshot,
} from "../packages/data/src/lib/foundry/bench.server";

type Args = {
  dir: string;
  manifest?: string;
  runner: BenchRunner;
  slug?: string;
  scene?: string;
  log?: string;
  exitCode: number | null;
  ranAt?: string;
  db?: string;
  payloadOut?: string;
  dryRun: boolean;
};

const ARENA_DEFAULT_SLUG = "flagtag-arena";
const ARENA_DEFAULT_SCENE = "flagtag";

function fail(message: string): never {
  console.error(`foundry-ingest-bench: ${message}`);
  process.exit(1);
}

function parseArgs(argv: string[]): Args {
  const positional: string[] = [];
  const out: Partial<Args> = { runner: "dclbots", exitCode: null, dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const next = () => {
      const v = argv[++i];
      if (v === undefined) fail(`${arg} needs a value`);
      return v;
    };
    switch (arg) {
      case "--manifest": out.manifest = next(); break;
      case "--runner": {
        const r = next();
        if (r !== "dclbots" && r !== "arena") fail("--runner is dclbots or arena");
        out.runner = r;
        break;
      }
      case "--slug": out.slug = next(); break;
      case "--scene": out.scene = next(); break;
      case "--log": out.log = next(); break;
      case "--exit-code": {
        const n = Number(next());
        if (!Number.isFinite(n)) fail("--exit-code needs a number");
        out.exitCode = n;
        break;
      }
      case "--ran-at": out.ranAt = next(); break;
      case "--db": out.db = next(); break;
      case "--payload-out": out.payloadOut = next(); break;
      case "--dry-run": out.dryRun = true; break;
      default:
        if (arg !== undefined && arg.startsWith("--")) fail(`unknown flag ${arg}`);
        if (arg !== undefined) positional.push(arg);
    }
  }
  const dir = positional[0];
  if (!dir) fail("usage: foundry:ingest-bench -- <evidence-dir> [--runner arena] [...]");
  return {
    dir,
    manifest: out.manifest,
    runner: out.runner ?? "dclbots",
    slug: out.slug,
    scene: out.scene,
    log: out.log,
    exitCode: out.exitCode ?? null,
    ranAt: out.ranAt,
    db: out.db,
    payloadOut: out.payloadOut,
    dryRun: out.dryRun ?? false,
  };
}

function readJson(file: string): unknown {
  return JSON.parse(readFileSync(file, "utf8"));
}

function readLines(file: string | undefined): string[] {
  if (!file || !existsSync(file)) return [];
  return readFileSync(file, "utf8").split(/\r?\n/);
}

function listShots(dir: string): string[] {
  const shots = path.join(dir, "shots");
  if (!existsSync(shots)) return [];
  return readdirSync(shots)
    .filter((f) => !f.startsWith("."))
    .sort()
    .map((f) => path.join("shots", f));
}

/** The run's own clock, in this order: what the operator passed, what the
 *  snapshot recorded, when the evidence was written. Never `now()` unless the
 *  evidence carries nothing at all. */
function resolveRanAt(args: Args, snapshotFile: string, snapshot: BenchSnapshot | null, logFile: string | undefined): string {
  if (args.ranAt) {
    const parsed = new Date(args.ranAt);
    if (Number.isNaN(parsed.getTime())) fail("--ran-at is not a date");
    return parsed.toISOString();
  }
  const capturedAt = snapshot?.capturedAt;
  if (typeof capturedAt === "number" && Number.isFinite(capturedAt)) {
    return new Date(capturedAt * 1000).toISOString();
  }
  for (const file of [snapshotFile, logFile]) {
    if (file && existsSync(file)) return statSync(file).mtime.toISOString();
  }
  return new Date().toISOString();
}

function slugFromDir(dir: string): string {
  const base = path.basename(path.resolve(dir));
  const m = /^(.*)-\d{6,}$/.exec(base);
  return (m?.[1] ?? base) || "run";
}

function pendingDir(args: Args): string {
  return (
    args.payloadOut ??
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ??
    "/tmp/foundry-pending-ingest"
  );
}

function park(args: Args, payload: unknown, reason: string): string {
  const target = pendingDir(args);
  const isFile = /\.json$/i.test(target);
  const dir = isFile ? path.dirname(target) : target;
  mkdirSync(dir, { recursive: true });
  const file = isFile
    ? target
    : path.join(dir, `bench-${path.basename(path.resolve(args.dir))}.json`);
  writeFileSync(file, JSON.stringify({ kind: "bench-ingest", reason, payload }, null, 2));
  return file;
}

function isPermissionDenied(err: unknown): boolean {
  const e = err as { code?: unknown; message?: unknown } | null;
  if (e?.code === "42501") return true;
  return typeof e?.message === "string" && /permission denied/i.test(e.message);
}

const args = parseArgs(process.argv.slice(2));

if (!existsSync(args.dir)) fail(`no such evidence directory: ${args.dir}`);

const snapshotFile = path.join(args.dir, "snapshot.json");
const snapshot: BenchSnapshot | null = existsSync(snapshotFile)
  ? (readJson(snapshotFile) as BenchSnapshot)
  : null;
if (!snapshot && args.runner === "dclbots") {
  fail(`${snapshotFile} not found — a dclbots run without its snapshot is not evidence`);
}

const logFile = args.log ?? path.join(args.dir, "run.log");
const logLines = readLines(existsSync(logFile) ? logFile : undefined);
const manifest = args.manifest ? readJson(args.manifest) : null;
const manifestSlug =
  manifest && typeof manifest === "object" && typeof (manifest as { slug?: unknown }).slug === "string"
    ? (manifest as { slug: string }).slug
    : null;

const slug =
  args.slug ??
  (args.runner === "arena" ? ARENA_DEFAULT_SLUG : (manifestSlug ?? slugFromDir(args.dir)));
const sceneId =
  args.scene ?? (args.runner === "arena" ? ARENA_DEFAULT_SCENE : (manifestSlug ?? slugFromDir(args.dir)));

const evidence: BenchEvidence = {
  runner: args.runner,
  slug,
  sceneId,
  ranAt: resolveRanAt(args, snapshotFile, snapshot, existsSync(logFile) ? logFile : undefined),
  evidencePath: path.resolve(args.dir),
  manifest,
  snapshot,
  logLines,
  shots: listShots(args.dir),
  exitCode: args.exitCode,
};

const url = args.db ?? process.env.FOUNDRY_DATABASE_URL?.trim() ?? process.env.CATALYST_DATABASE_URL?.trim();

if (args.dryRun || !url) {
  const ingest = buildBenchIngest(evidence);
  const file = park(args, ingest, args.dryRun ? "dry run" : "no database url");
  console.log(
    `foundry-ingest-bench: ${ingest.events.length} event(s) for ${slug} parked at ${file}`,
  );
  if (!url && !args.dryRun) {
    console.error(
      "foundry-ingest-bench: set FOUNDRY_DATABASE_URL (or pass --db) to write it",
    );
    process.exit(1);
  }
  process.exit(0);
}

const pool = new Pool({ connectionString: url, max: 2 });
let ingest = buildBenchIngest(evidence);
let exitCode = 0;

try {
  // A report may not point at a scene that was never imported: the FK would
  // refuse it, and a made-up scene id would be worse than none.
  if (ingest.report.sceneId) {
    const known = await pool.query<{ id: string }>(
      `SELECT id FROM foundry.scene WHERE id = $1`,
      [ingest.report.sceneId],
    );
    if (known.rowCount === 0) {
      console.log(
        `foundry-ingest-bench: scene "${ingest.report.sceneId}" is not in the registry — recording the run without a scene link`,
      );
      ingest = {
        ...ingest,
        trajectory: { ...ingest.trajectory, sceneId: null },
        report: { ...ingest.report, sceneId: null },
      };
    }
  }

  const out = await ingestBenchRun(pool, ingest);
  console.log(
    `foundry-ingest-bench: ${slug} → report ${out.reportId}, trajectory ${out.trajectoryId}, ${out.events} event(s)` +
      (ingest.report.verdict ? `, verdict ${ingest.report.verdict}` : ", verdict not recorded"),
  );
} catch (e) {
  if (isPermissionDenied(e)) {
    const file = park(args, ingest, "database refused the write (permission denied)");
    console.error(`foundry-ingest-bench: permission denied — payload parked at ${file}`);
    exitCode = 3;
  } else {
    console.error("foundry-ingest-bench: failed —", (e as Error).message);
    exitCode = 1;
  }
} finally {
  await pool.end();
}

process.exit(exitCode);
