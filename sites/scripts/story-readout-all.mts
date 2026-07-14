#!/usr/bin/env tsx
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { parseStory } from "../packages/core/src/lib/experiments/context";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SITES = path.resolve(HERE, "..");
const STORIES_DIR = path.resolve(HERE, "../packages/features/src/stories");

function env(name: string): string | undefined {
  try {
    return typeof process !== "undefined" ? process.env?.[name] : undefined;
  } catch {
    return undefined;
  }
}

type Opts = {
  includeDrafts: boolean;
  alpha: number;
  json: boolean;
};

const USAGE =
  "usage: npm run story:readout:all -- [--include-drafts] [--alpha 0.05] [--json]\n" +
  "env:   TELEMETRY_URL=http://127.0.0.1:5150 TELEMETRY_ADMIN_TOKEN=<token> npm run story:readout:all\n" +
  "       (token = CATALYRST_TELEMETRY_ADMIN_TOKEN in the telemetry service env; /dash/sql is gated)\n" +
  "Runs story:readout for every non-draft story.md (all 130+ with --include-drafts)\n" +
  "and prints an aligned digest table to stderr plus one JSON line to stdout.\n";

function parseArgs(argv: string[]): Opts {
  const opts: Opts = { includeDrafts: false, alpha: 0.05, json: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    if (a === "--include-drafts") opts.includeDrafts = true;
    else if (a === "--json") opts.json = true;
    else if (a === "--alpha") {
      const alpha = Number(argv[++i]);
      if (Number.isFinite(alpha) && alpha > 0 && alpha < 1) opts.alpha = alpha;
    } else if (a === "--help" || a === "-h") {
      process.stderr.write(USAGE);
      process.exit(0);
    } else {
      process.stderr.write(`unknown flag: ${a}\n${USAGE}`);
      process.exit(2);
    }
  }
  return opts;
}

function findStoryDirs(): string[] {
  const dirs: string[] = [];
  (function walk(dir: string): void {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name === "story.md") dirs.push(dir);
    }
  })(STORIES_DIR);
  return dirs.sort();
}

function relId(dir: string): string {
  return path.relative(STORIES_DIR, dir).split(path.sep).join("/");
}

type DigestRow = {
  id: string;
  key: string;
  trials: string;
  verdict: string;
  report: Record<string, unknown>;
};

function runReadout(id: string, alpha: number): DigestRow {
  const args = ["run", "--silent", "story:readout", "--", id, "--json"];
  if (alpha !== 0.05) args.push("--alpha", String(alpha));
  const r = spawnSync("npm", args, {
    cwd: SITES,
    encoding: "utf8",
    timeout: 60_000,
  });
  const lastLine = (r.stdout ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .pop();
  if (r.status !== 0 || !lastLine) {
    const detail = r.error
      ? r.error.message
      : `story:readout exit ${r.status}`;
    return { id, key: "-", trials: "-", verdict: "ERROR", report: { id, error: detail } };
  }
  let report: Record<string, unknown>;
  try {
    report = JSON.parse(lastLine) as Record<string, unknown>;
  } catch {
    return {
      id,
      key: "-",
      trials: "-",
      verdict: "ERROR",
      report: { id, error: `unparseable readout output: ${lastLine.slice(0, 120)}` },
    };
  }
  if (typeof report.error === "string") {
    return { id, key: "-", trials: "-", verdict: "NO DATA", report };
  }
  const variants = Array.isArray(report.variants)
    ? (report.variants as Array<{ variant: string; den: number }>)
    : [];
  const trials = variants.map((v) => `${v.variant}:${v.den}`).join(" ") || "-";
  return {
    id,
    key: String(report.experimentKey ?? "-"),
    trials,
    verdict: String(report.verdict ?? "-"),
    report,
  };
}

function printTable(rows: DigestRow[]): void {
  const header = { id: "story", key: "key", trials: "trials per arm", verdict: "verdict" };
  const all = [header, ...rows];
  const w = {
    id: Math.max(...all.map((r) => r.id.length)),
    key: Math.max(...all.map((r) => r.key.length)),
    trials: Math.max(...all.map((r) => r.trials.length)),
  };
  const lines = all.map(
    (r) =>
      `${r.id.padEnd(w.id)}  ${r.key.padEnd(w.key)}  ${r.trials.padEnd(w.trials)}  ${r.verdict}`,
  );
  process.stderr.write(lines.join("\n") + "\n");
}

function main(): void {
  const opts = parseArgs(process.argv.slice(2));

  if (!env("TELEMETRY_URL")) {
    process.stderr.write(
      "TELEMETRY_URL is not set -- cannot read experiment data.\n" +
        "Set TELEMETRY_URL to your catalyrst-telemetry base URL (and\n" +
        "TELEMETRY_ADMIN_TOKEN to its admin token -- /dash/sql is gated) and re-run, e.g.\n" +
        "  TELEMETRY_URL=http://127.0.0.1:5150 TELEMETRY_ADMIN_TOKEN=... npm run story:readout:all\n",
    );
    process.exit(1);
  }

  const rows: DigestRow[] = [];
  for (const dir of findStoryDirs()) {
    const id = relId(dir);
    let status: string;
    try {
      status = parseStory(dir).status;
    } catch (err) {
      rows.push({
        id,
        key: "-",
        trials: "-",
        verdict: "PARSE ERROR",
        report: { id, error: err instanceof Error ? err.message : String(err) },
      });
      continue;
    }
    if (!opts.includeDrafts && status === "draft") continue;
    rows.push(runReadout(id, opts.alpha));
  }

  if (!opts.json) printTable(rows);
  process.stdout.write(
    JSON.stringify({
      alpha: opts.alpha,
      count: rows.length,
      stories: rows.map((r) => r.report),
    }) + "\n",
  );
}

main();
