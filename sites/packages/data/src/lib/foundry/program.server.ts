import type { Pool } from "pg";

import { benchTargets, type BenchTarget } from "./bench.server";
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
  /** When the probe actually ran — memoised up to 30s, so "when the page
   *  rendered" would be a lie; null only if the probe itself threw. */
  copilotProbedAt: string | null;
  llm: Pick<LlmUsageSummary, "inputTokens" | "outputTokens" | "costUsd">;
  requests: number;
  pledges: number;
  personasClaimed: number;
  /** Deployed scenes carrying a linked design doc — the lede's docs clause. */
  docsOnDeployed: number;
  /** Docs the copilot itself drafted — the lede's copilot clause. */
  copilotDrafted: number;
  /** Distinct (scene, realm) pairs real bench runs touched — the lede's bench
   *  clause classifies these instead of asserting "local copies" as prose. */
  benchTargets: BenchTarget[];
};

export interface ProgramCheck {
  id: string;
  label: string;
  value: string;
  source: string;
  href?: string;
}

const SNAPSHOT_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.gdd_doc) AS gdd_count,
         (SELECT count(*)::int FROM foundry.bot_report
           WHERE runner IS DISTINCT FROM 'arena') AS bench_runs,
         (SELECT max(ran_at) FROM foundry.bot_report
           WHERE runner IS DISTINCT FROM 'arena') AS last_bench_at,
         (SELECT count(*)::int FROM foundry.request WHERE status <> 'closed') AS requests,
         (SELECT count(*)::int FROM foundry.pledge) AS pledges,
         (SELECT count(*)::int FROM foundry.persona) AS personas,
         (SELECT count(DISTINCT s.id)::int FROM foundry.scene s
           WHERE s.entity_id IS NOT NULL
             AND (s.gdd_doc_id IS NOT NULL
                  OR EXISTS (SELECT 1 FROM foundry.gdd_doc d
                              WHERE d.scene_id = s.id))) AS docs_on_deployed,
         (SELECT count(*)::int FROM foundry.gdd_doc
           WHERE source = 'copilot') AS copilot_drafted`;

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
  personas: number;
  docs_on_deployed: number;
  copilot_drafted: number;
};

type LlmDbRow = {
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
};

export async function homeSnapshot(db: Pool = getPool()): Promise<HomeSnapshot> {
  // The copilot lives in its own unit; an unreachable one is a fact about
  // today, not an error the front door should refuse to render — probeCopilot
  // owns that refusal-to-throw and answers offline-with-a-probe-time itself.
  const [scenes, snap, llm, copilot, targets] = await Promise.all([
    listScenes(db),
    db.query<SnapshotDbRow>(SNAPSHOT_SQL),
    db.query<LlmDbRow>(LLM_SQL),
    probeCopilot(),
    benchTargets(db),
  ]);
  const row = snap.rows[0];
  const usage = llm.rows[0];
  return {
    scenes,
    gddCount: Number(row?.gdd_count ?? 0),
    benchRuns: Number(row?.bench_runs ?? 0),
    lastBenchAt: row?.last_bench_at ? row.last_bench_at.toISOString() : null,
    copilotOnline: copilot.online,
    copilotProbedAt: copilot.probedAt,
    llm: {
      inputTokens: Number(usage?.input_tokens ?? 0),
      outputTokens: Number(usage?.output_tokens ?? 0),
      costUsd: Number(usage?.cost_usd ?? 0),
    },
    requests: Number(row?.requests ?? 0),
    pledges: Number(row?.pledges ?? 0),
    personasClaimed: Number(row?.personas ?? 0),
    docsOnDeployed: Number(row?.docs_on_deployed ?? 0),
    copilotDrafted: Number(row?.copilot_drafted ?? 0),
    benchTargets: targets,
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
         (SELECT count(*)::int FROM foundry.request WHERE status <> 'closed') AS open_requests,
         (SELECT count(*)::int FROM foundry.pledge) AS pledges,
         (SELECT count(*)::int FROM foundry.request WHERE origin = 'visitor') AS loop_asks,
         (SELECT count(*)::int FROM foundry.session_series
           WHERE retired_at IS NULL) AS loop_series,
         (SELECT count(*)::int FROM (
            SELECT 1 FROM foundry.session_rsvp
             GROUP BY series_id, sid
            HAVING count(DISTINCT occurrence_at) >= 2) returners) AS loop_returns`;

type ChecksDbRow = {
  games: number;
  games_passing: number;
  docs: number;
  docs_no_open: number;
  tokens: string;
  open_requests: number;
  pledges: number;
  loop_asks: number;
  loop_series: number;
  loop_returns: number;
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
      href: "/foundry/console/bench",
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
      source: "Posted here or imported from a public source; one row each.",
      href: "/foundry/exchange",
    },
    {
      id: "pledges",
      label: "Pledges",
      value: reading(Number(r?.pledges ?? 0)),
      source: "One row per browser session per request, counted directly.",
      href: "/foundry/exchange",
    },
    // The four demand-loop stages. This is the hypothesis the program is
    // running to test — ask, pledge, session, return — so each stage is a
    // measured count with its table named, and a zero is the honest reading of
    // a stage nobody has reached yet, shown the way the console shows absence.
    {
      id: "loop-ask",
      label: "Demand loop: asks posted here",
      value: reading(Number(r?.loop_asks ?? 0)),
      source:
        "Visitor-posted rows in foundry.request (origin = 'visitor') — imported asks are counted under Open requests. Stage one: someone asks for what they want built.",
      href: "/foundry/exchange",
    },
    {
      id: "loop-pledge",
      label: "Demand loop: pledges recorded",
      value: reading(Number(r?.pledges ?? 0)),
      source:
        "Rows in foundry.pledge, one per browser session per request. Stage two: someone else says they'll show up for it.",
      href: "/foundry/exchange",
    },
    {
      id: "loop-session",
      label: "Demand loop: sessions on the calendar",
      value: reading(Number(r?.loop_series ?? 0)),
      source:
        "Unretired series in foundry.session_series. Stage three: a host puts a recurring gathering on the calendar.",
      href: "/foundry/sessions",
    },
    {
      id: "loop-return",
      label: "Demand loop: same-group returns",
      value: reading(Number(r?.loop_returns ?? 0)),
      source:
        "Visitors with RSVPs to two or more occurrences of the same series, counted from foundry.session_rsvp. Stage four: the same group comes back.",
      href: "/foundry/sessions",
    },
  ];
}

/** Docs the site's own program drafted — so the copilot page can say its
 *  counts are copilot-only without the shelf's program brief reading as one. */
export async function listProgramDraftedDocs(
  db: Pool = getPool(),
): Promise<{ id: string; title: string }[]> {
  const res = await db.query<{ id: string; title: string }>(
    `SELECT id, title FROM foundry.gdd_doc
      WHERE source = 'program'
      ORDER BY created_at DESC`,
  );
  return res.rows.map((r) => ({ id: r.id, title: r.title }));
}

export interface CopilotPipelineStage {
  id: string;
  label: string;
  /** Copilot-originated artifacts that actually traversed this stage. */
  count: number;
  /** The rows the count is read from. */
  source: string;
  /** The surface that lists those rows. */
  href: string | null;
}

// Per-stage receipts for the copilot pipeline diagram: how many artifacts of
// COPILOT origin actually traversed each stage, read from the same tables the
// rest of the console reads. All zeros today is the honest reading — the
// pipeline's claim becomes shown data instead of prose. Scene/deploy/bench
// stages count through the doc link, the only recorded tie between a scene and
// the copilot's work.
const PIPELINE_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.gdd_doc
           WHERE source = 'copilot' AND kind IN ('brief','proposal')) AS briefs,
         (SELECT count(*)::int FROM foundry.gdd_doc
           WHERE source = 'copilot' AND kind = 'shortgdd') AS shortgdds,
         (SELECT count(DISTINCT s.id)::int FROM foundry.scene s
           WHERE EXISTS (SELECT 1 FROM foundry.gdd_doc d
                          WHERE d.source = 'copilot'
                            AND (d.scene_id = s.id OR d.id = s.gdd_doc_id))) AS scenes,
         (SELECT count(DISTINCT s.id)::int FROM foundry.scene s
           WHERE s.entity_id IS NOT NULL
             AND EXISTS (SELECT 1 FROM foundry.gdd_doc d
                          WHERE d.source = 'copilot'
                            AND (d.scene_id = s.id OR d.id = s.gdd_doc_id))) AS deploys,
         (SELECT count(*)::int FROM foundry.bot_report b
           WHERE b.runner IS DISTINCT FROM 'arena'
             AND EXISTS (SELECT 1 FROM foundry.scene s
                          WHERE s.id = b.scene_id
                            AND EXISTS (SELECT 1 FROM foundry.gdd_doc d
                                         WHERE d.source = 'copilot'
                                           AND (d.scene_id = s.id OR d.id = s.gdd_doc_id)))) AS benches`;

type PipelineDbRow = {
  briefs: number;
  shortgdds: number;
  scenes: number;
  deploys: number;
  benches: number;
};

export async function copilotPipelineReceipts(
  db: Pool = getPool(),
): Promise<CopilotPipelineStage[]> {
  const res = await db.query<PipelineDbRow>(PIPELINE_SQL);
  const r = res.rows[0];
  // A count of exactly one names its artifact and links it directly — a
  // receipt a reader cannot follow to its row is a claim, not a receipt. The
  // supersession label keeps a superseded version from passing as current.
  const docs = await db.query<{
    id: string;
    title: string;
    kind: string;
    superseded: boolean;
  }>(
    `SELECT g.id, g.title, g.kind,
            EXISTS (SELECT 1 FROM foundry.gdd_doc s WHERE s.supersedes = g.id)
              AS superseded
       FROM foundry.gdd_doc g WHERE g.source = 'copilot'`,
  );
  const single = (kinds: string[]) => {
    const hits = docs.rows.filter((d) => kinds.includes(d.kind));
    if (hits.length !== 1) return null;
    const d = hits[0];
    return {
      href: `/foundry/gdd/${d.id}`,
      note: ` — ${d.title}${d.superseded ? ", superseded" : ""}`,
    };
  };
  const briefOne = single(["brief", "proposal"]);
  const shortOne = single(["shortgdd"]);
  return [
    {
      id: "brief",
      label: "brief",
      count: Number(r?.briefs ?? 0),
      source:
        "copilot-drafted briefs and proposals in foundry.gdd_doc" +
        (briefOne?.note ?? ""),
      href: briefOne?.href ?? "/foundry/gdd",
    },
    {
      id: "shortgdd",
      label: "shortGDD",
      count: Number(r?.shortgdds ?? 0),
      source:
        "copilot-drafted shortGDDs in foundry.gdd_doc" + (shortOne?.note ?? ""),
      href: shortOne?.href ?? "/foundry/gdd",
    },
    {
      id: "scene",
      label: "SDK7 scene",
      count: Number(r?.scenes ?? 0),
      source: "scenes linked to a copilot-drafted doc in foundry.scene",
      href: "/foundry/play",
    },
    {
      id: "deploy",
      label: "deploy to a World",
      count: Number(r?.deploys ?? 0),
      source: "those scenes carrying a Worlds deployment entity",
      href: "/foundry/play",
    },
    {
      id: "bench",
      label: "bot-bench test",
      count: Number(r?.benches ?? 0),
      source: "real bench runs on those scenes in foundry.bot_report",
      href: "/foundry/console/bench",
    },
  ];
}
