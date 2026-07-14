import { Pool } from "pg";
import { failedChecksPhrase } from "@ui/foundry/checks";

import type { BotReport } from "./types";

// The per-game response read: what this program has actually measured about one
// game, aggregated from the event store that the site's own usage events land
// in. This module opens its OWN read-only connection (the events live in a
// different database from the foundry schema) and hands back counts and
// distinct-badge counts only — a raw anonymousId never crosses this boundary,
// and the pre-badge-fix rows from 2026-08-14 that stored raw ids are excluded
// by the badge-length filter in every query.
//
// invalid_reason is deliberately ignored: the running collector loaded a stale
// contract and mis-flags valid fd_* rows, so selecting by event name is the
// honest read.

export class ResponseSignalsUnavailableError extends Error {
  constructor(
    message = "Response signals not configured (FOUNDRY_TELEMETRY_DATABASE_URL unset)",
  ) {
    super(message);
    this.name = "ResponseSignalsUnavailableError";
  }
}

function connectionString(): string | undefined {
  if (typeof process === "undefined") return undefined;
  const cs = process.env.FOUNDRY_TELEMETRY_DATABASE_URL;
  return cs && cs.trim() !== "" ? cs : undefined;
}

let pool: Pool | null = null;

export function getResponseSignalsPool(): Pool {
  const cs = connectionString();
  if (!cs) throw new ResponseSignalsUnavailableError();
  if (!pool) {
    pool = new Pool({
      connectionString: cs,
      max: 2,
      statement_timeout: 15_000,
      idleTimeoutMillis: 30_000,
    });
  }
  return pool;
}

export function isResponseSignalsConfigured(): boolean {
  return connectionString() !== undefined;
}

/** The narrow query surface the reads need, so tests can hand in a fake. */
export interface ResponseSignalsDb {
  query<T extends object>(
    text: string,
    values?: unknown[],
  ): Promise<{ rows: T[] }>;
}

// The game page (and its fd_game_viewed event) shipped on 2026-08-15; nothing
// was measured before that, and nothing is ever backfilled. The query floor
// sits one day earlier so the known pre-floor stragglers are in range of the
// badge-length exclusion instead of silently out of it.
export const MEASURED_FLOOR = "2026-08-14";
// Downloads count only from the day the loader-side track was removed: every
// earlier fd_bundle_downloaded row could have been minted by a bare GET (a
// sweep agent, a crawler) — observed 37 such rows, none of them a person.
const DOWNLOADS_FLOOR = "2026-08-20";
export const MEASURED_SINCE_LABEL = "15 Aug 2026";

// A visitor badge is 4 hex chars (sidBadge in db.server.ts). Anything longer
// is a pre-badge-fix raw id and is excluded, never aggregated.
const BADGE_ONLY = "char_length(body->>'anonymousId') <= 4";

export interface VisitDayRow {
  day: string;
  badge: string;
}

export interface VisitDay {
  day: string;
  visitors: number;
  returning: number;
}

/**
 * Distinct badges per day; a visitor counts as returning on a day when their
 * badge was already seen on an earlier day of the window. Pure math over
 * (day, badge) pairs, so the arithmetic is testable without a database.
 */
export function visitDays(rows: readonly VisitDayRow[]): VisitDay[] {
  const byDay = new Map<string, Set<string>>();
  for (const r of rows) {
    const set = byDay.get(r.day) ?? new Set<string>();
    set.add(r.badge);
    byDay.set(r.day, set);
  }
  const seen = new Set<string>();
  return [...byDay.keys()].sort().map((day) => {
    const badges = byDay.get(day) as Set<string>;
    let returning = 0;
    for (const b of badges) if (seen.has(b)) returning += 1;
    for (const b of badges) seen.add(b);
    return { day, visitors: badges.size, returning };
  });
}

/**
 * The before/after-revision split, only when visits were measured on BOTH
 * sides of the deploy day — otherwise null, and the caller says which absence
 * it is (no revision since measurement began, or too thin to read).
 */
export function revisionSplit(
  deployedAt: string | null,
  days: readonly VisitDay[],
): { deployedDay: string; before: number; after: number } | null {
  if (!deployedAt) return null;
  const deployedDay = deployedAt.slice(0, 10);
  let before = 0;
  let after = 0;
  for (const d of days) {
    if (d.day < deployedDay) before += d.visitors;
    else after += d.visitors;
  }
  return before > 0 && after > 0 ? { deployedDay, before, after } : null;
}

export interface ReplayLine {
  trajectoryId: string;
  opens: number;
  /** Steps + scrubs inside the replay — someone studying it, not just opening it. */
  interactions: number;
}

export interface ResponseSignals {
  /** Distinct visitor badges per day on this game's page, measured, never backfilled. */
  visitDays: VisitDay[];
  /** Raw view/embed event count behind those days — the volume honesty check. */
  visitEvents: number;
  distinctVisitors: number;
  replays: ReplayLine[];
  downloads: number;
}

type DayBadgeRow = { day: string; badge: string; n: number };
type ReplayRow = { trajectory_id: string; event: string; n: number };
type CountRow = { n: number };

/**
 * One game's measured signals since the floor. Counts and distinct-badge
 * counts only — the badge strings themselves stay in this function.
 */
export async function readResponseSignals(
  db: ResponseSignalsDb,
  opts: { slug: string; trajectoryIds: readonly string[] },
): Promise<ResponseSignals> {
  try {
    return await readResponseSignalsInner(db, opts);
  } catch (err) {
    if (err instanceof ResponseSignalsUnavailableError) throw err;
    // A configured-but-unreadable pool (missing table, bad search_path, a
    // refused connection) must degrade to the honest could-not-be-read state,
    // never an error page over the rest of the response reading.
    console.error("response signals unreadable", err);
    throw new ResponseSignalsUnavailableError();
  }
}

async function readResponseSignalsInner(
  db: ResponseSignalsDb,
  opts: { slug: string; trajectoryIds: readonly string[] },
): Promise<ResponseSignals> {
  const [visits, replays, downloads] = await Promise.all([
    db.query<DayBadgeRow>(
      `SELECT (received_at AT TIME ZONE 'UTC')::date::text AS day,
              body->>'anonymousId' AS badge,
              count(*)::int AS n
         FROM telemetry_events
        WHERE event_kind = 'track'
          AND received_at >= $2::date
          AND body->>'event' IN ('fd_game_viewed', 'fd_embed_started')
          AND body->'properties'->>'slug' = $1
          AND ${BADGE_ONLY}
        GROUP BY 1, 2`,
      [opts.slug, MEASURED_FLOOR],
    ),
    opts.trajectoryIds.length === 0
      ? Promise.resolve({ rows: [] as ReplayRow[] })
      : db.query<ReplayRow>(
          `SELECT body->'properties'->>'trajectory_id' AS trajectory_id,
                  body->>'event' AS event,
                  count(*)::int AS n
             FROM telemetry_events
            WHERE event_kind = 'track'
              AND received_at >= $2::date
              AND body->>'event' IN
                    ('fd_replay_opened', 'fd_replay_stepped', 'fd_replay_scrubbed')
              AND body->'properties'->>'trajectory_id' = ANY($1::text[])
              AND ${BADGE_ONLY}
            GROUP BY 1, 2`,
          [[...opts.trajectoryIds], MEASURED_FLOOR],
        ),
    db.query<CountRow>(
      `SELECT count(*)::int AS n
         FROM telemetry_events
        WHERE event_kind = 'track'
          AND received_at >= $2::date
          AND body->>'event' = 'fd_bundle_downloaded'
          AND body->'properties'->>'scene_id' = $1
          AND ${BADGE_ONLY}`,
      [opts.slug, DOWNLOADS_FLOOR],
    ),
  ]);

  const days = visitDays(visits.rows);
  const distinct = new Set(visits.rows.map((r) => r.badge));

  const byTrajectory = new Map<string, ReplayLine>();
  for (const r of replays.rows) {
    const line = byTrajectory.get(r.trajectory_id) ?? {
      trajectoryId: r.trajectory_id,
      opens: 0,
      interactions: 0,
    };
    if (r.event === "fd_replay_opened") line.opens += Number(r.n);
    else line.interactions += Number(r.n);
    byTrajectory.set(r.trajectory_id, line);
  }

  return {
    visitDays: days,
    visitEvents: visits.rows.reduce((acc, r) => acc + Number(r.n), 0),
    distinctVisitors: distinct.size,
    replays: [...byTrajectory.values()].sort((a, b) =>
      a.trajectoryId.localeCompare(b.trajectoryId),
    ),
    downloads: Number(downloads.rows[0]?.n ?? 0),
  };
}

export interface ResponseReportLine {
  id: string;
  ranAt: string;
  /** Plain words, arena rule applied: an arena run "completed a sandbox
   *  simulation", never "pass". */
  text: string;
  evidenceHref: string | null;
  replayHref: string | null;
}

/**
 * One bot run as one plain sentence fragment, with the arena rule applied
 * server-side: an arena verdict is a process exit code, not a test of the game
 * (the arena-row rule in bench.server.ts), so it reads as a completed run and
 * never as a pass.
 */
export function reportLine(report: BotReport): ResponseReportLine {
  let text: string;
  if (report.runner === "arena") {
    text = "completed a sandbox simulation";
  } else if (report.verdict === "pass") {
    text =
      report.checksTotal === null
        ? "passed its checks"
        : `passed all ${report.checksTotal} check${report.checksTotal === 1 ? "" : "s"}`;
  } else if (report.verdict === "fail") {
    if (report.checksFailed !== null && report.checksTotal !== null) {
      text = failedChecksPhrase({
        checksFailed: report.checksFailed,
        checksTotal: report.checksTotal,
        checksUnevaluable: report.checksUnevaluable ?? 0,
      });
    } else {
      text = "failed its checks";
    }
  } else {
    text = "ran without a recorded verdict";
  }
  return {
    id: report.id,
    ranAt: report.ranAt,
    text,
    evidenceHref: report.evidencePath
      ? `/foundry/console/evidence/${report.id}`
      : null,
    replayHref: report.trajectoryId
      ? `/foundry/console/trajectories/${report.trajectoryId}`
      : null,
  };
}
