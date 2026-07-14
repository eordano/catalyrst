import type { Pool } from "pg";

import { getPool, sidBadge } from "./db.server";
import { PRIVATE_LOG_ACTIONS } from "./persona.server";
import type { TimelineLane, TimelineRow } from "./types";

// The Floor's flagship reader, and the FIRST consumer of action_log. It merges
// six real row-sources into one reverse-chron memory. Nothing is synthesised:
// each lane reads rows that already exist, a dangling action_log subject renders
// as absent (LEFT JOIN, label from the subject's own table when it resolves),
// and the actor
// is a claimed persona name or the honest visitor badge — a raw sid never
// reaches the wire. Demand-bearing lanes exclude arena sandbox simulations with
// `runner IS DISTINCT FROM 'arena'` (NULL-safe).

export const TIMELINE_LANES: readonly TimelineLane[] = [
  "community",
  "exchange",
  "worlds",
  "harness",
  "trajectory",
  "docs",
];

// The community lane's subject is whatever the recorded action acted on: the
// action name says which table the subject id lives in, and the resolved title
// becomes the linked label. An id that resolves nowhere renders as absent, and
// an action outside these lists (persona claims log the actor's own sid as
// subject) resolves to no kind at all — never a request-shaped fallback.
const SCENE_ACTIONS = `('claim_steward','release_steward','scene_note',
                        'offer_transfer','revoke_transfer','accept_transfer',
                        'register_scene')`;
const SESSION_ACTIONS = `('schedule_session','retire_session','rsvp_session',
                          'withdraw_rsvp')`;
const REQUEST_ACTIONS = `('post_request','edit_request','pledge','withdraw_pledge','approve_request','close_request')`;
const GDD_ACTIONS = `('approve_gdd','edit_gdd_doc','publish_gdd_draft')`;

// Each lane emits the same column shape so the union is uniform. sid is set only
// for visitor-written rows; provenance drives the pill; runner labels bot rows;
// subject_kind is resolved only where the lane itself cannot name the subject's
// table (the community lane's mixed action log).
const LANE_SQL: Record<TimelineLane, string> = {
  community: `
    SELECT 'community'::text AS lane, ('al-' || a.id::text) AS id, a.at AS at,
           a.sid AS sid, a.action AS action, a.subject AS subject,
           CASE
             WHEN a.action IN ${SCENE_ACTIONS} THEN sc.title
             WHEN a.action IN ${SESSION_ACTIONS} THEN ss.title
             WHEN a.action IN ${REQUEST_ACTIONS} THEN r.title
             WHEN a.action IN ${GDD_ACTIONS} THEN gd.title
             ELSE NULL
           END AS subject_label, ''::text AS body,
           'visitor'::text AS provenance, NULL::text AS runner,
           NULL::text AS source_url,
           CASE
             WHEN a.action IN ${SCENE_ACTIONS} THEN 'scene'
             WHEN a.action IN ${SESSION_ACTIONS} THEN 'session'
             WHEN a.action IN ${REQUEST_ACTIONS} THEN 'request'
             WHEN a.action IN ${GDD_ACTIONS} THEN 'doc'
             ELSE NULL
           END AS subject_kind, false AS date_only
      FROM foundry.action_log a
      LEFT JOIN foundry.request r ON r.id = a.subject
      LEFT JOIN foundry.scene sc ON sc.id = a.subject
      LEFT JOIN foundry.session_series ss ON ss.id = a.subject
      LEFT JOIN foundry.gdd_doc gd ON gd.id = a.subject
     WHERE a.action NOT IN ${PRIVATE_LOG_ACTIONS}
       AND ($1::timestamptz IS NULL OR a.at < $1::timestamptz)
     ORDER BY a.at DESC, a.id DESC
     LIMIT $2::int`,
  exchange: `
    SELECT 'exchange'::text AS lane, ('rq-' || r.id::text) AS id,
           COALESCE(r.sourced_at, r.created_at) AS at,
           NULL::text AS sid, 'ask_arrived'::text AS action, r.id AS subject,
           r.title AS subject_label, ''::text AS body,
           'import'::text AS provenance, NULL::text AS runner,
           r.source_url AS source_url, NULL::text AS subject_kind,
           (r.sourced_date_only AND r.sourced_at IS NOT NULL) AS date_only
      FROM foundry.request r
     WHERE r.origin = 'import'
       AND ($1::timestamptz IS NULL
            OR COALESCE(r.sourced_at, r.created_at) < $1::timestamptz)
     ORDER BY COALESCE(r.sourced_at, r.created_at) DESC, r.id DESC
     LIMIT $2::int`,
  worlds: `
    SELECT 'worlds'::text AS lane, ('cl-' || c.id::text) AS id, c.at AS at,
           c.sid AS sid, 'scene_changelog'::text AS action, c.scene_id AS subject,
           s.title AS subject_label, c.note AS body,
           c.origin AS provenance, NULL::text AS runner,
           NULL::text AS source_url, NULL::text AS subject_kind,
           false AS date_only
      FROM foundry.scene_changelog c
      JOIN foundry.scene s ON s.id = c.scene_id
     WHERE ($1::timestamptz IS NULL OR c.at < $1::timestamptz)
     ORDER BY c.at DESC, c.id DESC
     LIMIT $2::int`,
  harness: `
    SELECT 'harness'::text AS lane, ('br-' || b.id) AS id, b.ran_at AS at,
           NULL::text AS sid, 'bench_run'::text AS action, b.scene_id AS subject,
           s.title AS subject_label,
           coalesce(b.verdict, 'run') AS body,
           'bot'::text AS provenance, b.runner AS runner,
           NULL::text AS source_url, NULL::text AS subject_kind,
           false AS date_only
      FROM foundry.bot_report b
      LEFT JOIN foundry.scene s ON s.id = b.scene_id
     WHERE b.runner IS DISTINCT FROM 'arena'
       AND ($1::timestamptz IS NULL OR b.ran_at < $1::timestamptz)
     ORDER BY b.ran_at DESC, b.id DESC
     LIMIT $2::int`,
  trajectory: `
    SELECT 'trajectory'::text AS lane, ('tr-' || t.id) AS id, t.created_at AS at,
           NULL::text AS sid, 'episode'::text AS action, t.scene_id AS subject,
           s.title AS subject_label, t.provenance AS body,
           t.provenance AS provenance, t.runner AS runner,
           NULL::text AS source_url, NULL::text AS subject_kind,
           false AS date_only
      FROM foundry.trajectory t
      LEFT JOIN foundry.scene s ON s.id = t.scene_id
     WHERE ($1::timestamptz IS NULL OR t.created_at < $1::timestamptz)
     ORDER BY t.created_at DESC, t.id DESC
     LIMIT $2::int`,
  docs: `
    SELECT 'docs'::text AS lane, ('gd-' || d.id) AS id, d.created_at AS at,
           NULL::text AS sid, 'design_doc'::text AS action, d.scene_id AS subject,
           d.title AS subject_label, ''::text AS body,
           CASE WHEN d.source IN ('program','copilot') THEN 'recorded'
                WHEN d.source = 'session' THEN 'visitor'
                ELSE 'import' END AS provenance,
           d.source AS runner,
           NULL::text AS source_url, NULL::text AS subject_kind,
           false AS date_only
      FROM foundry.gdd_doc d
     WHERE ($1::timestamptz IS NULL OR d.created_at < $1::timestamptz)
     ORDER BY d.created_at DESC, d.id DESC
     LIMIT $2::int`,
};

type TimelineDbRow = {
  lane: string;
  id: string;
  at: Date | string;
  sid: string | null;
  action: string;
  subject: string | null;
  subject_label: string | null;
  body: string;
  provenance: string;
  runner: string | null;
  source_url: string | null;
  subject_kind: string | null;
  date_only: boolean;
};

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

/** Claimed persona display names for a set of sids — one query, one map. Shared
 *  by the timeline and the scene continuity readers so every surface resolves
 *  actors the same way: claimed name when one exists (each sid resolved through
 *  the alias layer to its persona), honest badge otherwise. */
export async function personaNames(
  sids: readonly (string | null)[],
): Promise<Map<string, string>> {
  const names = new Map<string, string>();
  const distinct = [...new Set(sids.filter((s): s is string => !!s))];
  if (distinct.length === 0) return names;
  const res = await getPool().query<{ sid: string; display_name: string }>(
    `SELECT q.sid, p.display_name
       FROM unnest($1::text[]) AS q(sid)
       LEFT JOIN foundry.sid_alias al ON al.alias_sid = q.sid
       JOIN foundry.persona p ON p.sid = COALESCE(al.persona_sid, q.sid)`,
    [distinct],
  );
  for (const p of res.rows) names.set(p.sid, p.display_name);
  return names;
}

function isLane(value: string): value is TimelineLane {
  return (TIMELINE_LANES as readonly string[]).includes(value);
}

function provenanceOf(value: string): TimelineRow["provenance"] {
  if (value === "bot") return "bot";
  if (value === "visitor") return "visitor";
  if (value === "recorded") return "recorded";
  return "import";
}

function subjectKindOf(value: string | null): TimelineRow["subjectKind"] {
  return value === "request" ||
    value === "scene" ||
    value === "session" ||
    value === "doc"
    ? value
    : null;
}

// The one-line verb for a row, chosen by its lane and (for the community lane)
// the recorded action. An unknown action gets an honest fallback rather than a
// guessed sentence — the same refusal the trajectory reader uses for unknown
// event types. Verbs are label-free: the resolved subject label renders beside
// the verb, as a link where the subject has a surface of its own.
function bodyFor(r: TimelineDbRow): string {
  if (r.lane === "community") {
    switch (r.action) {
      case "post_request":
        return "posted a request";
      case "edit_request":
        return "revised their own request";
      case "pledge":
        return "pledged to a request";
      case "withdraw_pledge":
        return "withdrew a pledge";
      case "claim_persona":
        return "claimed a persona";
      case "update_persona":
        return "updated their persona";
      case "select_role":
        return "chose a door";
      case "redeem_invite":
        return "redeemed an invite";
      case "mint_invite":
        return "minted an invite";
      case "revoke_role":
        return "revoked a role";
      case "grant_consent":
        return "granted a consent";
      case "withdraw_consent":
        return "withdrew a consent";
      case "schedule_session":
        return "scheduled a session";
      case "retire_session":
        return "retired a session";
      case "rsvp_session":
        return "said they'll come to a session";
      case "withdraw_rsvp":
        return "withdrew a session RSVP";
      case "file_appeal":
        return "filed an appeal";
      case "withdraw_appeal":
        return "withdrew an appeal";
      case "resolve_appeal":
        return "resolved an appeal";
      case "approve_request":
        return "approved a request";
      case "close_request":
        return "closed a request";
      case "claim_steward":
        return "claimed stewardship of a scene";
      case "release_steward":
        return "released stewardship of a scene";
      case "offer_transfer":
        return "offered a stewardship transfer";
      case "revoke_transfer":
        return "revoked a stewardship transfer";
      case "accept_transfer":
        return "accepted a stewardship transfer";
      case "scene_note":
        return "left a note on a scene";
      case "register_scene":
        return "registered a game on the shelf";
      case "approve_gdd":
        return "approved a design doc version";
      case "edit_gdd_doc":
        return "edited a design doc";
      case "publish_gdd_draft":
        return "published a design doc from a copilot session";
      default:
        return `${r.action} on ${r.subject ?? "—"} — detail not rendered`;
    }
  }
  if (r.lane === "exchange") return "asked in public — imported to the exchange";
  if (r.lane === "worlds") return r.body || "deployed to Worlds";
  if (r.lane === "harness") {
    if (r.body === "pass") return "playtest passed";
    if (r.body === "fail") return "playtest failed";
    return "playtest ran — no verdict recorded";
  }
  if (r.lane === "trajectory") return "a bot played an episode";
  if (r.lane === "docs")
    // The actor column already reads "this program" on a program-drafted row;
    // the verb stays short so the line never says it twice.
    return r.runner === "program"
      ? "drafted a design doc"
      : r.runner === "session"
        ? "design doc edited on this site"
        : r.runner === "copilot"
          ? "design doc published from a copilot session"
          : "design doc imported";
  return r.body;
}

// The source host of an imported ask's public permalink — never throws on
// stored data: a null or unparseable URL reads as null and the caller labels it.
function hostOf(url: string | null): string | null {
  if (!url) return null;
  try {
    return new URL(url).hostname;
  } catch {
    return null;
  }
}

export interface TimelinePage {
  rows: TimelineRow[];
  nextBefore: string | null;
}

/**
 * A reverse-chron page of the merged memory. `before` is a keyset cursor on the
 * event timestamp; `lanes` narrows the union. Actor labels are resolved from
 * claimed personas, falling back to the honest badge for visitor rows and a
 * source label ('worlds mirror', the bot runner) for imported/bot rows.
 */
export async function listTimeline({
  before,
  lanes,
  limit = 50,
}: {
  before?: string | null;
  lanes?: TimelineLane[];
  limit?: number;
} = {}): Promise<TimelinePage> {
  const active =
    lanes && lanes.length > 0 ? lanes.filter(isLane) : [...TIMELINE_LANES];
  if (active.length === 0) return { rows: [], nextBefore: null };

  const cap = Math.max(1, Math.min(200, limit));
  const union = active.map((l) => `(${LANE_SQL[l]})`).join("\nUNION ALL\n");
  const pool = getPool();
  const res = await pool.query<TimelineDbRow>(
    `SELECT * FROM (${union}) merged
      ORDER BY at DESC, id DESC
      LIMIT $2::int`,
    [before ?? null, cap],
  );

  const names = await personaNames(res.rows.map((r) => r.sid));

  const rows: TimelineRow[] = res.rows.map((r) => {
    let actor: TimelineRow["actor"];
    if (r.sid) {
      const name = names.get(r.sid);
      actor = name ? { name } : { badge: sidBadge(r.sid) };
    } else if (r.provenance === "bot") {
      actor = { source: r.runner ?? "harness" };
    } else if (r.lane === "exchange") {
      actor = { source: hostOf(r.source_url) ?? "import" };
    } else if (r.lane === "docs") {
      actor = {
        source: r.runner === "program" ? "this program" : "design workspace",
      };
    } else {
      actor = { source: "worlds mirror" };
    }
    return {
      lane: isLane(r.lane) ? r.lane : "community",
      id: r.id,
      at: iso(r.at),
      actor,
      action: r.action,
      subject: r.subject,
      subjectLabel: r.subject_label,
      subjectKind: subjectKindOf(r.subject_kind),
      body: bodyFor(r),
      provenance: provenanceOf(r.provenance),
      runner: r.runner,
      dateOnly: r.date_only === true,
    };
  });

  const nextBefore = rows.length === cap ? rows[rows.length - 1].at : null;
  return { rows, nextBefore };
}

export interface TimelineStats {
  events: number;
  actors: number;
  firstMemory: string | null;
}

// The stat counts exactly what the feed renders: bench RUNS exclude arena (the
// harness lane filters them) while EPISODES include arena (the trajectory lane
// renders them, labeled sandbox) — the arena-row rule in bench.server.ts.
const STATS_SQL = `
  SELECT ((SELECT count(*) FROM foundry.action_log)
        + (SELECT count(*) FROM foundry.request WHERE origin = 'import')
        + (SELECT count(*) FROM foundry.scene_changelog)
        + (SELECT count(*) FROM foundry.bot_report WHERE runner IS DISTINCT FROM 'arena')
        + (SELECT count(*) FROM foundry.trajectory)
        + (SELECT count(*) FROM foundry.gdd_doc))::int AS events,
         (SELECT count(DISTINCT sid)::int FROM foundry.action_log) AS actors,
         LEAST(
           (SELECT min(at) FROM foundry.action_log),
           -- The same instant the exchange lane renders for an imported ask:
           -- its original public date when one was recorded.
           (SELECT min(COALESCE(sourced_at, created_at)) FROM foundry.request
             WHERE origin = 'import'),
           (SELECT min(at) FROM foundry.scene_changelog),
           (SELECT min(ran_at) FROM foundry.bot_report WHERE runner IS DISTINCT FROM 'arena'),
           (SELECT min(created_at) FROM foundry.trajectory),
           (SELECT min(created_at) FROM foundry.gdd_doc)
         ) AS first_memory`;

/** Header tiles for the timeline. events/actors are measured zeros when empty;
 *  firstMemory is null only when nothing has ever been recorded (UI prints '—'). */
/** Records this sid's timeline visit and returns the PREVIOUS visit time —
 *  the honest anchor for "since your last visit". First visit returns null and
 *  the surface stays silent rather than inventing a baseline. */
export async function markTimelineVisit(
  sid: string,
  pool: Pool = getPool(),
): Promise<string | null> {
  const res = await pool.query<{ at: Date | string }>(
    `WITH prev AS (SELECT at FROM foundry.timeline_visit WHERE sid = $1),
          up AS (
            INSERT INTO foundry.timeline_visit (sid, at) VALUES ($1, now())
            ON CONFLICT (sid) DO UPDATE SET at = now()
          )
     SELECT at FROM prev`,
    [sid],
  );
  const r = res.rows[0];
  return r ? iso(r.at) : null;
}

/** How many timeline events landed after the given instant — same six-lane
 *  union the feed reads, so the count can never disagree with the rows. */
export async function countTimelineSince(
  atIso: string,
  pool: Pool = getPool(),
): Promise<number> {
  // Every lane fragment binds $1 (before-cursor, null = no cap) and $2 (limit)
  // — they must be bound here too or postgres refuses the statement. The
  // per-lane limit is effectively unbounded; $3 is the actual since-filter.
  const union = TIMELINE_LANES.map((l) => `(${LANE_SQL[l]})`).join("\nUNION ALL\n");
  const res = await pool.query<{ fresh: number }>(
    `SELECT count(*)::int AS fresh FROM (${union}) t WHERE t.at > $3`,
    [null, 2147483647, atIso],
  );
  return Number(res.rows[0]?.fresh ?? 0);
}

export async function timelineStats(pool: Pool = getPool()): Promise<TimelineStats> {
  const res = await pool.query<{
    events: number;
    actors: number;
    first_memory: Date | string | null;
  }>(STATS_SQL);
  const r = res.rows[0];
  return {
    events: Number(r?.events ?? 0),
    actors: Number(r?.actors ?? 0),
    firstMemory: r?.first_memory == null ? null : iso(r.first_memory),
  };
}
