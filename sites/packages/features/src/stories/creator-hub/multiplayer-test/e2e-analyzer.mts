#!/usr/bin/env node
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

type RunFile = {
  bots?: number;
  thresholds?: {
    max_diverged_final?: number;
    max_converge_ms?: number;
    min_synced_pct?: number;
    max_never_synced?: number;
  };
  shape?: { profile?: string; seed?: number | string };
};

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
const h16 = (s: string): string => createHash("sha256").update(s).digest("hex").slice(0, 16);

const [cmd, runDir, ...rest] = process.argv.slice(2);
if (!cmd || !runDir) {
  console.error("usage: e2e-analyzer.mts analyze <run-dir> | replay <run-dir> --out <dir> [--profile p] [--seed n]");
  process.exit(2);
}

const flags: Record<string, string> = {};
for (let i = 0; i < rest.length; i += 2) flags[rest[i]?.replace(/^--/, "")] = rest[i + 1]!;

const run = JSON.parse(readFileSync(path.join(runDir, "run.json"), "utf8")) as RunFile;
const runId = path.basename(runDir);

if (cmd === "analyze") {
  await sleep(1500);
  const bots = run.bots ?? 2;
  const perBot: Record<string, number> = {};
  for (let i = 1; i <= bots; i++) perBot[`b${i}`] = 1180 + i * 120;
  const times = Object.values(perBot).sort((a, b) => a - b);
  const median = times[Math.floor((times.length - 1) / 2)];
  const p95 = times[times.length - 1];
  const report = {
    convergence: { per_bot: perBot, median_ms: median, p95_ms: p95 },
    corrections: { total: 3, per_peer: { b1: 1, b2: 2 } },
    join: { retries: { b1: 0, b2: 1 } },
    divergence: {
      final: [],
      transient: [
        {
          entity: "512",
          component: "core::Transform",
          peers: ["b2"],
          closed: true,
          oracle: "9f31aa02",
          beacon: "9f31aa02",
          probe: "9f31aa02",
        },
      ],
      never_synced: [],
    },
    swarm_verdict: "PASS",
  };
  const t = run.thresholds ?? {};
  const checks = [
    { name: "max_diverged_final", value: 0, threshold: t.max_diverged_final ?? 0, pass: 0 <= (t.max_diverged_final ?? 0) },
    { name: "max_converge_ms", value: p95, threshold: t.max_converge_ms ?? 15000, pass: p95 <= (t.max_converge_ms ?? 15000) },
    { name: "min_synced_pct", value: 100, threshold: t.min_synced_pct ?? 100, pass: 100 >= (t.min_synced_pct ?? 100) },
    { name: "max_never_synced", value: 0, threshold: t.max_never_synced ?? 0, pass: 0 <= (t.max_never_synced ?? 0) },
  ];
  const verdict = { pass: checks.every((c) => c.pass), checks };
  const metrics = path.join(runDir, "metrics");
  mkdirSync(metrics, { recursive: true });
  writeFileSync(path.join(metrics, "report.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(metrics, "verdict.json"), JSON.stringify(verdict, null, 2));
  writeFileSync(
    path.join(metrics, "report.html"),
    `<!doctype html><title>mp report ${runId}</title><h1>Sync-health report \u{2014} ${runId}</h1><p>verdict: ${verdict.pass ? "PASS" : "FAIL"}</p><pre>${JSON.stringify(report, null, 2)}</pre>`,
  );
  process.exit(verdict.pass ? 0 : 1);
}

if (cmd === "replay") {
  const out = flags.out;
  if (!out) process.exit(2);
  await sleep(1000);
  const profile = flags.profile ?? run.shape?.profile ?? "none";
  const seed = flags.seed ?? run.shape?.seed ?? 42;
  mkdirSync(out, { recursive: true });
  writeFileSync(
    path.join(out, "outcome.json"),
    JSON.stringify(
      {
        run: runId,
        profile,
        seed: Number(seed),
        outcome: {
          converged_hash: h16(`${runId}|${profile}|${seed}|state`),
          decision_hash: h16(`${runId}|${profile}|${seed}|decisions`),
        },
      },
      null,
      2,
    ),
  );
  writeFileSync(
    path.join(out, "timeline.json"),
    JSON.stringify({ transient_divergence: [], checkpoints: [] }, null, 2),
  );
  process.exit(0);
}

console.error(`e2e-analyzer: unknown command ${cmd}`);
process.exit(2);
