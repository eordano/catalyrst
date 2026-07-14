import type { Pool, PoolClient } from "pg";
import { failedChecksPhrase } from "@ui/foundry/checks";

import type {
  BotReport,
  CheckVerdict,
  TrajectoryEvent,
  TurnEndReason,
} from "./types";

// The bench reads what the dcl-scene-bots harness actually wrote and nothing
// else. Its core rule is ours: a check that cannot be evaluated fails, and a run
// that never happened is shown as nothing.
//
// THE ARENA-ROW RULE, site-wide: bench/bot RUN counts exclude runner='arena'
// everywhere. EPISODE/trajectory counts include arena, always labeled. The
// visitor-facing word is "sandbox simulation"; the raw runner value 'arena'
// never surfaces as prose.
//
// The harness reports a FINAL SNAPSHOT plus a verdict per check — not a step
// trace. The event mapping below is therefore deliberately coarse: one turn, one
// tool call carrying the manifest verbatim, one observation carrying the
// snapshot, one event per parsed verdict line. No per-step result is ever
// synthesised, and every event carries the run's own timestamp rather than an
// invented one.

export type BenchRunner = "dclbots" | "arena";

/** The keys `dclbots/run.py:capture()` writes. Everything is optional: an old
 *  explorer omits what it cannot report, and that absence is itself evidence. */
export interface BenchSnapshot {
  scene?: unknown;
  player?: unknown;
  logs?: unknown;
  logWindow?: unknown;
  networkWrites?: unknown[];
  missingTools?: unknown;
  stubbedTools?: unknown;
  peer?: unknown;
  entityListingUnread?: unknown;
  entityPagesTruncated?: unknown;
  screenshots?: unknown;
  [key: string]: unknown;
}

export interface BenchEvidence {
  runner: BenchRunner;
  slug: string;
  sceneId: string | null;
  ranAt: string;
  evidencePath: string;
  /** The manifest JSON, verbatim, when one was named. */
  manifest: unknown;
  snapshot: BenchSnapshot | null;
  /** `run.log` for dclbots, the arena's stdout for arena. Verbatim lines. */
  logLines: string[];
  shots: string[];
  exitCode: number | null;
}

export interface BenchIngest {
  trajectory: {
    id: string;
    sceneId: string | null;
    runner: BenchRunner;
    finishReason: TurnEndReason;
    evidencePath: string;
    createdAt: string;
  };
  events: Omit<TrajectoryEvent, "trajectoryId">[];
  report: BotReport;
}

const NETWORK_WRITE_CAP = 500;

/** The realm a dclbots run actually targeted, read from the captured snapshot.
 *  Arena runs carry no snapshot and therefore no realm. */
function sceneRealm(snapshot: BenchSnapshot | null): string | null {
  const scene = snapshot?.scene;
  if (scene !== null && scene !== undefined && typeof scene === "object") {
    const realm = (scene as { realm?: unknown }).realm;
    if (typeof realm === "string" && realm.trim() !== "") return realm;
  }
  return null;
}

const CHECK_LINE = /^\s*\[(PASS|FAIL)\]\s+([^:]+):\s*(.*)$/;
const FINAL_LINE =
  /^\s*(\S[^:]*):\s+(PASS|FAIL)\s+\((?:(\d+)\s+of\s+)?(\d+)\s+checks?\)\s*$/;

/**
 * `[PASS|FAIL] <kind>: <detail>  (<why>)` — `checks.Result.line()`. The why is
 * appended after two spaces and only on failure, so a detail of its own that
 * ends in a parenthesis survives intact.
 */
export function parseCheckLines(lines: readonly string[]): CheckVerdict[] {
  const out: CheckVerdict[] = [];
  for (const line of lines) {
    const m = CHECK_LINE.exec(line);
    if (!m) continue;
    const pass = m[1] === "PASS";
    const kind = (m[2] ?? "").trim();
    let detail = (m[3] ?? "").trim();
    let why = "";
    if (!pass && detail.endsWith(")")) {
      const at = detail.lastIndexOf("  (");
      if (at > 0) {
        why = detail.slice(at + 3, -1);
        detail = detail.slice(0, at).trim();
      }
    }
    out.push({ kind, pass, detail, why });
  }
  return out;
}

export interface BenchFinalVerdict {
  slug: string;
  verdict: "pass" | "fail";
  checksTotal: number;
  checksFailed: number;
}

/** `<slug>: PASS (N checks)` / `<slug>: FAIL (k of N checks)` — `checks.verdict()`. */
export function parseFinalVerdict(
  lines: readonly string[],
): BenchFinalVerdict | null {
  for (let i = lines.length - 1; i >= 0; i--) {
    const m = FINAL_LINE.exec(lines[i] ?? "");
    if (!m) continue;
    const total = Number(m[4]);
    const failed = m[3] === undefined ? 0 : Number(m[3]);
    return {
      slug: (m[1] ?? "").trim(),
      verdict: m[2] === "PASS" ? "pass" : "fail",
      checksTotal: Number.isFinite(total) ? total : 0,
      checksFailed: Number.isFinite(failed) ? failed : 0,
    };
  }
  return null;
}

function toStringList(value: unknown): string[] {
  if (Array.isArray(value)) return value.map((v) => String(v));
  if (value && typeof value === "object") {
    // `stubbedTools` is written as {tool: what is wrong with it}. Flatten it
    // without dropping the half that says why.
    return Object.entries(value as Record<string, unknown>).map(
      ([name, detail]) => `${name}: ${String(detail)}`,
    );
  }
  if (typeof value === "string" && value.trim() !== "") return [value];
  return [];
}

/** The stored observation: the parts of the snapshot a reader can act on, with
 *  the network-write list capped and the cap itself reported. */
export function trimSnapshot(snapshot: BenchSnapshot): Record<string, unknown> {
  const writes = Array.isArray(snapshot.networkWrites) ? snapshot.networkWrites : null;
  const trimmed: Record<string, unknown> = {};
  // The realm and blocked-flag are the datum that proves what the run actually
  // touched. They were being dropped here; a run against a local scene copy read
  // as a run against the deployed World, so they are kept.
  const scene = snapshot.scene;
  if (scene !== null && typeof scene === "object") {
    const realm = (scene as { realm?: unknown }).realm;
    const isBlocked = (scene as { isBlocked?: unknown }).isBlocked;
    const sceneOut: Record<string, unknown> = {};
    if (typeof realm === "string" && realm !== "") sceneOut.realm = realm;
    if (typeof isBlocked === "boolean") sceneOut.isBlocked = isBlocked;
    if (Object.keys(sceneOut).length > 0) trimmed.scene = sceneOut;
  }
  if (snapshot.logWindow !== undefined) trimmed.logWindow = snapshot.logWindow;
  if (writes) {
    trimmed.networkWrites = writes.slice(0, NETWORK_WRITE_CAP);
    trimmed.networkWritesTotal = writes.length;
    if (writes.length > NETWORK_WRITE_CAP) trimmed.networkWritesCapped = NETWORK_WRITE_CAP;
  }
  if (snapshot.missingTools !== undefined) trimmed.missingTools = snapshot.missingTools;
  if (snapshot.stubbedTools !== undefined) trimmed.stubbedTools = snapshot.stubbedTools;
  if (snapshot.entityPagesTruncated !== undefined) {
    trimmed.entityPagesTruncated = snapshot.entityPagesTruncated;
  }
  if (snapshot.entityListingUnread !== undefined) {
    trimmed.entityListingUnread = snapshot.entityListingUnread;
  }
  return trimmed;
}

/** Stable ids from the evidence directory, so re-ingesting a run updates it
 *  instead of forking a second copy of the same episode. */
export function benchIdsFor(evidencePath: string, ranAt: string): {
  trajectoryId: string;
  reportId: string;
} {
  const base = evidencePath.replace(/\/+$/, "").split("/").pop() ?? "";
  const key =
    base
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "")
      .slice(0, 48) || String(Date.parse(ranAt) || 0);
  return { trajectoryId: `traj-${key}`, reportId: `bench-${key}` };
}

export function buildBenchIngest(ev: BenchEvidence): BenchIngest {
  const { trajectoryId, reportId } = benchIdsFor(ev.evidencePath, ev.ranAt);
  const at = ev.ranAt;
  const events: Omit<TrajectoryEvent, "trajectoryId">[] = [];
  const push = (type: TrajectoryEvent["type"], data: unknown) =>
    events.push({ seq: events.length, type, time: at, data });

  push("turn/start", { turn: 1 });

  let verdict: "pass" | "fail" | null = null;
  let checksTotal: number | null = null;
  let checksFailed: number | null = null;
  let checksUnevaluable = 0;
  let reason: TurnEndReason;

  if (ev.runner === "arena") {
    // The arena prints a deterministic scoreboard and exits. Every printed line
    // is stored verbatim as its own observation — nothing is parsed into
    // metrics, because the sandbox's own words are the evidence.
    for (const line of ev.logLines) {
      if (line.trim() === "") continue;
      push("obs/snapshot", { line });
    }
    if (ev.exitCode === null) {
      reason = { kind: "interrupted", detail: "exit code not recorded" };
    } else if (ev.exitCode === 0) {
      verdict = "pass";
      reason = { kind: "completed" };
    } else {
      verdict = "fail";
      reason = { kind: "error", detail: `exit code ${ev.exitCode}` };
    }
  } else {
    push("tool/call", {
      callId: "manifest",
      name: "dclbots.run",
      arguments: ev.manifest ?? null,
    });
    push("obs/snapshot", {
      snapshot: ev.snapshot ? trimSnapshot(ev.snapshot) : null,
      shots: ev.shots,
    });

    const checks = parseCheckLines(ev.logLines);
    for (const check of checks) push("check/verdict", check);
    checksUnevaluable = checks.filter(
      (c) => !c.pass && c.detail.toLowerCase().includes("cannot evaluate"),
    ).length;

    const final = parseFinalVerdict(ev.logLines);
    if (final) {
      verdict = final.verdict;
      checksTotal = final.checksTotal;
      checksFailed = final.checksFailed;
      reason =
        final.verdict === "pass"
          ? { kind: "completed" }
          : {
              kind: "error",
              detail: failedChecksPhrase({
                checksFailed: final.checksFailed,
                checksTotal: final.checksTotal,
                checksUnevaluable,
              }),
            };
    } else {
      // A snapshot without the runner's own closing line: the run is recorded,
      // the verdict is not, and the report says exactly that.
      if (checks.length > 0) {
        checksTotal = checks.length;
        checksFailed = checks.filter((c) => !c.pass).length;
      }
      reason = { kind: "interrupted", detail: "no final verdict line in the run log" };
    }
  }

  push("turn/end", { turn: 1, reason });

  const snapshot = ev.snapshot ?? {};
  const writes = Array.isArray(snapshot.networkWrites) ? snapshot.networkWrites.length : null;

  return {
    trajectory: {
      id: trajectoryId,
      sceneId: ev.sceneId,
      runner: ev.runner,
      finishReason: reason,
      evidencePath: ev.evidencePath,
      createdAt: at,
    },
    events,
    report: {
      id: reportId,
      sceneId: ev.sceneId,
      slug: ev.slug,
      runner: ev.runner,
      realm: sceneRealm(ev.snapshot),
      ranAt: at,
      verdict,
      checksTotal,
      checksFailed,
      checksUnevaluable,
      missingTools: toStringList(snapshot.missingTools),
      stubbedTools: toStringList(snapshot.stubbedTools),
      networkWrites: writes,
      shots: ev.shots,
      evidencePath: ev.evidencePath,
      trajectoryId,
    },
  };
}

type ReportDbRow = {
  id: string;
  scene_id: string | null;
  slug: string;
  runner: string | null;
  realm: string | null;
  ran_at: Date | string;
  verdict: string | null;
  checks_total: number | null;
  checks_failed: number | null;
  checks_unevaluable: number | null;
  missing_tools: unknown;
  stubbed_tools: unknown;
  network_writes: number | null;
  shots: unknown;
  evidence_path: string | null;
  trajectory_id: string | null;
};

// checks_unevaluable is derived from the report's own stored check/verdict
// events — the harness writes "cannot evaluate: <gap>" as the failing check's
// detail, and ingest persists each verdict line as an event — so the "counted
// as failed" policy can travel with the number instead of living in prose.
const REPORT_COLUMNS = `b.id, b.scene_id, b.slug, b.runner, b.realm, b.ran_at,
       b.verdict, b.checks_total, b.checks_failed, b.missing_tools, b.stubbed_tools,
       b.network_writes, b.shots, b.evidence_path, b.trajectory_id,
       (SELECT count(*)::int FROM foundry.trajectory_event e
         WHERE e.trajectory_id = b.trajectory_id
           AND e.type = 'check/verdict'
           AND e.data->>'pass' = 'false'
           AND e.data->>'detail' ILIKE '%cannot evaluate%') AS checks_unevaluable`;

function jsonStrings(value: unknown): string[] {
  return Array.isArray(value) ? value.map((v) => String(v)) : [];
}

function toReport(r: ReportDbRow): BotReport {
  return {
    id: r.id,
    sceneId: r.scene_id,
    slug: r.slug,
    runner: r.runner === "dclbots" || r.runner === "arena" ? r.runner : null,
    realm: r.realm,
    ranAt: r.ran_at instanceof Date ? r.ran_at.toISOString() : String(r.ran_at),
    verdict: r.verdict === "pass" || r.verdict === "fail" ? r.verdict : null,
    checksTotal: r.checks_total === null ? null : Number(r.checks_total),
    checksFailed: r.checks_failed === null ? null : Number(r.checks_failed),
    checksUnevaluable: r.checks_unevaluable === null ? 0 : Number(r.checks_unevaluable),
    missingTools: jsonStrings(r.missing_tools),
    stubbedTools: jsonStrings(r.stubbed_tools),
    networkWrites: r.network_writes === null ? null : Number(r.network_writes),
    shots: jsonStrings(r.shots),
    evidencePath: r.evidence_path,
    trajectoryId: r.trajectory_id,
  };
}

export async function listBenchReports(
  db: Pool,
  sceneId?: string | null,
  limit = 100,
): Promise<BotReport[]> {
  const res = sceneId
    ? await db.query<ReportDbRow>(
        `SELECT ${REPORT_COLUMNS} FROM foundry.bot_report b
          WHERE b.scene_id = $1
          ORDER BY b.ran_at DESC, b.id
          LIMIT $2`,
        [sceneId, limit],
      )
    : await db.query<ReportDbRow>(
        `SELECT ${REPORT_COLUMNS} FROM foundry.bot_report b
          ORDER BY b.ran_at DESC, b.id
          LIMIT $1`,
        [limit],
      );
  return res.rows.map(toReport);
}

/** Totals for a capped list's "showing N of M" — split by the arena-row rule:
 *  `runs` counts what every other surface calls a run (runner != 'arena');
 *  `sandboxRuns` counts the arena simulations the list shows labeled, so the
 *  two surfaces can never disagree about what a "run" is. */
export async function countBenchReports(
  db: Pool,
  sceneId?: string | null,
): Promise<{ runs: number; sandboxRuns: number }> {
  const split = `count(*) FILTER (WHERE runner IS DISTINCT FROM 'arena')::int AS runs,
         count(*) FILTER (WHERE runner = 'arena')::int AS sandbox_runs`;
  const res = sceneId
    ? await db.query<{ runs: number; sandbox_runs: number }>(
        `SELECT ${split} FROM foundry.bot_report WHERE scene_id = $1`,
        [sceneId],
      )
    : await db.query<{ runs: number; sandbox_runs: number }>(
        `SELECT ${split} FROM foundry.bot_report`,
      );
  const row = res.rows[0];
  return {
    runs: Number(row?.runs ?? 0),
    sandboxRuns: Number(row?.sandbox_runs ?? 0),
  };
}

export interface BenchTarget {
  sceneId: string | null;
  /** The realm the run actually touched; null when the snapshot recorded none. */
  realm: string | null;
}

/**
 * The distinct (scene, realm) pairs real runs have touched, per the arena-row
 * rule. This is the datum behind every "local copies vs deployed Worlds"
 * sentence: the UI classifies each realm's host and words the claim from the
 * rows, so the sentence moves the day a run targets a deployed World.
 */
export async function benchTargets(db: Pool): Promise<BenchTarget[]> {
  const res = await db.query<{ scene_id: string | null; realm: string | null }>(
    `SELECT DISTINCT scene_id, realm FROM foundry.bot_report
      WHERE runner IS DISTINCT FROM 'arena'`,
  );
  return res.rows.map((r) => ({ sceneId: r.scene_id, realm: r.realm }));
}

export interface BenchSceneSummary {
  sceneId: string;
  /** Real (non-arena) runs only, per the arena-row rule above. */
  runs: number;
  /** Arena sandbox simulations on this scene — listed, labeled, never counted as runs. */
  sandboxRuns: number;
  lastVerdict: "pass" | "fail" | null;
  lastRanAt: string | null;
  /** From the same run the verdict was read from. */
  lastChecksFailed: number | null;
  lastChecksTotal: number | null;
  /** From the same run: its failed checks whose stored verdict events say
   *  "cannot evaluate" — harness gaps, not the game failing. */
  lastChecksUnevaluable: number | null;
  lastRealRanAt: string | null;
}

/** One row per scene that has actually been run. Scenes with no run are absent —
 *  the caller renders nothing for them rather than a zero. */
export async function benchSummaryByScene(db: Pool): Promise<BenchSceneSummary[]> {
  const res = await db.query<{
    scene_id: string;
    runs: number;
    sandbox_runs: number;
    last_verdict: string | null;
    last_ran_at: Date | string | null;
    last_checks_failed: number | null;
    last_checks_total: number | null;
    last_checks_unevaluable: number | null;
    last_real_ran_at: Date | string | null;
  }>(
    // A card's verdict pill means "a real bot run against this game passed", so
    // arena sandbox simulations are excluded from last_verdict — an arena PASS is
    // an exit code, not a test of the game. Per the arena-row rule, runs excludes
    // arena too; sandbox_runs carries the arena rows separately so the card can
    // say so. checks_failed/checks_total/ran_at come from the same run as the
    // verdict.
    `SELECT b.scene_id,
            count(*) FILTER (WHERE b.runner IS DISTINCT FROM 'arena')::int AS runs,
            count(*) FILTER (WHERE b.runner = 'arena')::int AS sandbox_runs,
            (array_agg(b.verdict ORDER BY b.ran_at DESC, b.id)
               FILTER (WHERE b.runner IS DISTINCT FROM 'arena'))[1] AS last_verdict,
            max(b.ran_at) AS last_ran_at,
            (array_agg(b.checks_failed ORDER BY b.ran_at DESC, b.id)
               FILTER (WHERE b.runner IS DISTINCT FROM 'arena'))[1] AS last_checks_failed,
            (array_agg(b.checks_total ORDER BY b.ran_at DESC, b.id)
               FILTER (WHERE b.runner IS DISTINCT FROM 'arena'))[1] AS last_checks_total,
            (array_agg((SELECT count(*)::int FROM foundry.trajectory_event e
                 WHERE e.trajectory_id = b.trajectory_id
                   AND e.type = 'check/verdict'
                   AND e.data->>'pass' = 'false'
                   AND e.data->>'detail' ILIKE '%cannot evaluate%')
               ORDER BY b.ran_at DESC, b.id)
               FILTER (WHERE b.runner IS DISTINCT FROM 'arena'))[1] AS last_checks_unevaluable,
            (array_agg(b.ran_at ORDER BY b.ran_at DESC, b.id)
               FILTER (WHERE b.runner IS DISTINCT FROM 'arena'))[1] AS last_real_ran_at
       FROM foundry.bot_report b
      WHERE b.scene_id IS NOT NULL
      GROUP BY b.scene_id`,
  );
  const iso = (v: Date | string | null): string | null =>
    v === null ? null : v instanceof Date ? v.toISOString() : String(v);
  return res.rows.map((r) => ({
    sceneId: r.scene_id,
    runs: Number(r.runs),
    sandboxRuns: Number(r.sandbox_runs),
    lastVerdict:
      r.last_verdict === "pass" || r.last_verdict === "fail" ? r.last_verdict : null,
    lastRanAt: iso(r.last_ran_at),
    lastChecksFailed:
      r.last_checks_failed === null ? null : Number(r.last_checks_failed),
    lastChecksTotal:
      r.last_checks_total === null ? null : Number(r.last_checks_total),
    lastChecksUnevaluable:
      r.last_checks_unevaluable === null ? null : Number(r.last_checks_unevaluable),
    lastRealRanAt: iso(r.last_real_ran_at),
  }));
}

export interface BenchIngestResult {
  trajectoryId: string;
  reportId: string;
  events: number;
  sceneId: string | null;
}

async function writeIngest(
  client: PoolClient,
  ingest: BenchIngest,
): Promise<BenchIngestResult> {
  const t = ingest.trajectory;
  await client.query(
    `INSERT INTO foundry.trajectory
       (id, scene_id, provenance, runner, finish_reason, evidence_path, created_at)
     VALUES ($1, $2, 'bot', $3, $4::jsonb, $5, $6)
     ON CONFLICT (id) DO UPDATE
        SET scene_id = EXCLUDED.scene_id,
            runner = EXCLUDED.runner,
            finish_reason = EXCLUDED.finish_reason,
            evidence_path = EXCLUDED.evidence_path,
            created_at = EXCLUDED.created_at`,
    [t.id, t.sceneId, t.runner, JSON.stringify(t.finishReason), t.evidencePath, t.createdAt],
  );

  // Re-ingesting the same evidence replaces its log rather than interleaving a
  // second one: seq stays contiguous from 0, which is what replay depends on.
  await client.query(`DELETE FROM foundry.trajectory_event WHERE trajectory_id = $1`, [
    t.id,
  ]);
  for (const ev of ingest.events) {
    await client.query(
      `INSERT INTO foundry.trajectory_event (trajectory_id, seq, type, time, data)
       VALUES ($1, $2, $3, $4, $5::jsonb)`,
      [t.id, ev.seq, ev.type, ev.time, JSON.stringify(ev.data)],
    );
  }

  const r = ingest.report;
  await client.query(
    `INSERT INTO foundry.bot_report
       (id, scene_id, slug, runner, realm, ran_at, verdict, checks_total,
        checks_failed, missing_tools, stubbed_tools, network_writes, shots,
        evidence_path, trajectory_id)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11::jsonb,$12,$13::jsonb,$14,$15)
     ON CONFLICT (id) DO UPDATE
        SET scene_id = EXCLUDED.scene_id,
            slug = EXCLUDED.slug,
            runner = EXCLUDED.runner,
            realm = EXCLUDED.realm,
            ran_at = EXCLUDED.ran_at,
            verdict = EXCLUDED.verdict,
            checks_total = EXCLUDED.checks_total,
            checks_failed = EXCLUDED.checks_failed,
            missing_tools = EXCLUDED.missing_tools,
            stubbed_tools = EXCLUDED.stubbed_tools,
            network_writes = EXCLUDED.network_writes,
            shots = EXCLUDED.shots,
            evidence_path = EXCLUDED.evidence_path,
            trajectory_id = EXCLUDED.trajectory_id`,
    [
      r.id,
      r.sceneId,
      r.slug,
      r.runner,
      r.realm,
      r.ranAt,
      r.verdict,
      r.checksTotal,
      r.checksFailed,
      JSON.stringify(r.missingTools),
      JSON.stringify(r.stubbedTools),
      r.networkWrites,
      JSON.stringify(r.shots),
      r.evidencePath,
      r.trajectoryId,
    ],
  );

  return {
    trajectoryId: t.id,
    reportId: r.id,
    events: ingest.events.length,
    sceneId: r.sceneId,
  };
}

/** One transaction: the episode, its events and the report that links them land
 *  together or not at all. */
export async function ingestBenchRun(
  db: Pool,
  ingest: BenchIngest,
): Promise<BenchIngestResult> {
  const client = await db.connect();
  try {
    await client.query("BEGIN");
    const out = await writeIngest(client, ingest);
    await client.query("COMMIT");
    return out;
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
}

export type BenchCheckState = "passed" | "failed" | "blocked";

export interface BenchCheckRow {
  kind: string;
  /** The check's own stated purpose — the goal a creator can aim at. */
  why: string;
  state: BenchCheckState;
  detail: string;
}

/** The stored check/verdict events of one run, in sequence — the named
 *  checklist behind a "N of M" number. "blocked" is the harness's own
 *  cannot-evaluate wording, kept distinct from a genuine failure. */
export async function benchChecklist(
  db: Pool,
  trajectoryId: string,
): Promise<BenchCheckRow[]> {
  const res = await db.query<{ data: unknown; seq: number }>(
    `SELECT seq, data FROM foundry.trajectory_event
      WHERE trajectory_id = $1 AND type = 'check/verdict'
      ORDER BY seq`,
    [trajectoryId],
  );
  return res.rows.map((r) => {
    const d = (r.data ?? {}) as Record<string, unknown>;
    const detail = typeof d.detail === "string" ? d.detail : "";
    const pass = d.pass === true || d.pass === "true";
    return {
      kind: typeof d.kind === "string" ? d.kind : "check",
      why: typeof d.why === "string" ? d.why : "",
      // Same classifier as the header pill's SQL (ILIKE '%cannot evaluate%'):
      // case-insensitive substring, so the pill and the list can never
      // disagree about which failures are harness gaps.
      state: pass
        ? "passed"
        : detail.toLowerCase().includes("cannot evaluate")
          ? "blocked"
          : "failed",
      detail,
    };
  });
}
