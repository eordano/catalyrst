import type { Pool } from "pg";

import { probeCopilot } from "./copilot.server";
import { getPool } from "./db.server";
import { listScenes } from "./scenes.server";
import type { FoundryScene, LlmUsageSummary } from "./types";

export type HomeSnapshot = {
  scenes: FoundryScene[];
  gddCount: number;
  benchRuns: number;
  lastBenchAt: string | null;
  copilotOnline: boolean;
  llm: Pick<LlmUsageSummary, "inputTokens" | "outputTokens" | "costUsd">;
  requests: number;
  pledges: number;
};

export interface ProgramCheck {
  id: string;
  label: string;
  value: string;
  source: string;
}

// The standing-note provenance: the exact figures the site-wide "what is real
// here" note asserts, read from the tables that hold them. Every field is a row
// count or a value out of a deployment entity — the note renders these instead
// of a sentence claiming they exist.
export interface FoundryProvenance {
  worldGames: number; // scenes carrying a Worlds deployment entity
  deployFrom: string | null; // earliest deployment-entity date
  deployTo: string | null; // latest deployment-entity date
  botRuns: number;
  trajectories: number;
  gddDocs: number;
  tokens: number; // input + output tokens metered from real copilot messages
}

const PROVENANCE_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.scene WHERE entity_id IS NOT NULL) AS world_games,
         (SELECT min(deployed_at) FROM foundry.scene WHERE entity_id IS NOT NULL) AS deploy_from,
         (SELECT max(deployed_at) FROM foundry.scene WHERE entity_id IS NOT NULL) AS deploy_to,
         (SELECT count(*)::int FROM foundry.bot_report) AS bot_runs,
         (SELECT count(*)::int FROM foundry.trajectory) AS trajectories,
         (SELECT count(*)::int FROM foundry.gdd_doc) AS gdd_docs,
         (SELECT coalesce(sum(input_tokens + output_tokens), 0)::int
            FROM foundry.llm_usage) AS tokens`;

export async function foundryProvenance(
  db: Pool = getPool(),
): Promise<FoundryProvenance> {
  const { rows } = await db.query<{
    world_games: number;
    deploy_from: Date | null;
    deploy_to: Date | null;
    bot_runs: number;
    trajectories: number;
    gdd_docs: number;
    tokens: number;
  }>(PROVENANCE_SQL);
  const r = rows[0];
  return {
    worldGames: r.world_games,
    deployFrom: r.deploy_from ? r.deploy_from.toISOString() : null,
    deployTo: r.deploy_to ? r.deploy_to.toISOString() : null,
    botRuns: r.bot_runs,
    trajectories: r.trajectories,
    gddDocs: r.gdd_docs,
    tokens: r.tokens,
  };
}

const SNAPSHOT_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.gdd_doc) AS gdd_count,
         (SELECT count(*)::int FROM foundry.bot_report) AS bench_runs,
         (SELECT max(ran_at) FROM foundry.bot_report) AS last_bench_at,
         (SELECT count(*)::int FROM foundry.request WHERE status = 'open') AS requests,
         (SELECT count(*)::int FROM foundry.pledge) AS pledges`;

const LLM_SQL = `
  SELECT coalesce(sum(input_tokens), 0)::int AS input_tokens,
         coalesce(sum(output_tokens), 0)::int AS output_tokens,
         coalesce(sum(coalesce(cost_usd,
                               input_tokens / 1e6 * price_input_per_m
                             + output_tokens / 1e6 * price_output_per_m)), 0)::float8 AS cost_usd
    FROM foundry.llm_usage`;

type SnapshotDbRow = {
  gdd_count: number;
  bench_runs: number;
  last_bench_at: Date | null;
  requests: number;
  pledges: number;
};

type LlmDbRow = {
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
};

// The copilot lives in its own unit; an unreachable one is a fact about today,
// not an error the front door should refuse to render.
async function copilotOnline(): Promise<boolean> {
  try {
    return (await probeCopilot()).online;
  } catch {
    return false;
  }
}

export async function homeSnapshot(db: Pool = getPool()): Promise<HomeSnapshot> {
  const [scenes, snap, llm, online] = await Promise.all([
    listScenes(db),
    db.query<SnapshotDbRow>(SNAPSHOT_SQL),
    db.query<LlmDbRow>(LLM_SQL),
    copilotOnline(),
  ]);
  const row = snap.rows[0];
  const usage = llm.rows[0];
  return {
    scenes,
    gddCount: Number(row?.gdd_count ?? 0),
    benchRuns: Number(row?.bench_runs ?? 0),
    lastBenchAt: row?.last_bench_at ? row.last_bench_at.toISOString() : null,
    copilotOnline: online,
    llm: {
      inputTokens: Number(usage?.input_tokens ?? 0),
      outputTokens: Number(usage?.output_tokens ?? 0),
      costUsd: Number(usage?.cost_usd ?? 0),
    },
    requests: Number(row?.requests ?? 0),
    pledges: Number(row?.pledges ?? 0),
  };
}

const CHECKS_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.scene) AS games,
         (SELECT count(DISTINCT scene_id)::int FROM foundry.bot_report
           WHERE verdict = 'pass' AND runner IS DISTINCT FROM 'arena') AS games_passing,
         (SELECT count(*)::int FROM foundry.gdd_doc) AS docs,
         (SELECT count(*)::int FROM foundry.gdd_doc
           WHERE coalesce((honesty -> 'totals' ->> 'open')::int, 0) = 0) AS docs_no_open,
         (SELECT coalesce(sum(input_tokens + output_tokens), 0)::bigint
            FROM foundry.llm_usage) AS tokens,
         (SELECT count(*)::int FROM foundry.request WHERE status = 'open') AS open_requests,
         (SELECT count(*)::int FROM foundry.pledge) AS pledges`;

type ChecksDbRow = {
  games: number;
  games_passing: number;
  docs: number;
  docs_no_open: number;
  tokens: string;
  open_requests: number;
  pledges: number;
};

// Intl's grouping moves with the runtime's ICU build and these strings are
// rendered on both sides of hydration.
function group(n: number): string {
  const digits = String(Math.trunc(Math.abs(n)));
  let out = "";
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += ",";
    out += digits[i];
  }
  return n < 0 ? `-${out}` : out;
}

// A count of zero is a genuine measurement of an empty table, so it prints `0`.
// An em dash is reserved for a value that was never measured (a NULL), which the
// individual checks below handle explicitly (e.g. "N of M" when there are no docs).
function reading(n: number): string {
  return group(n);
}

export async function programChecks(
  db: Pool = getPool(),
): Promise<ProgramCheck[]> {
  const res = await db.query<ChecksDbRow>(CHECKS_SQL);
  const r = res.rows[0];
  const docs = Number(r?.docs ?? 0);
  return [
    {
      id: "games",
      label: "Games registered",
      value: reading(Number(r?.games ?? 0)),
      source:
        "Every scene in the registry: the games deployed to Decentraland Worlds (imported by foundry:import-real) plus the repo starter scene that carries no Worlds entity. The front-door lede counts only the deployed ones, which is why it reads one lower.",
    },
    {
      id: "games-benched",
      label: "Games with a passing bench run",
      value: reading(Number(r?.games_passing ?? 0)),
      source:
        "Verdicts parsed from dcl-scene-bots output, ingested with foundry:ingest-bench. Arena sandbox simulations are excluded — a passing verdict here means a real run against the scene.",
    },
    {
      id: "docs",
      label: "Design docs",
      value: reading(docs),
      source:
        "shortGDDs imported from their source threads or drafted in the copilot workspace.",
    },
    {
      id: "docs-no-open",
      label: "Docs with no [OPEN] section",
      value: docs === 0 ? "—" : `${group(Number(r?.docs_no_open ?? 0))} of ${group(docs)}`,
      source: "Counted from each document's own honesty markers.",
    },
    {
      id: "tokens",
      label: "Copilot tokens spent",
      value: reading(Number(r?.tokens ?? 0)),
      source: "Per-message usage accounting read from the copilot's own API.",
    },
    {
      id: "requests",
      label: "Open requests",
      value: reading(Number(r?.open_requests ?? 0)),
      source: "Posted by visitors on this site.",
    },
    {
      id: "pledges",
      label: "Pledges",
      value: reading(Number(r?.pledges ?? 0)),
      source: "One row per session per request, counted directly.",
    },
  ];
}
