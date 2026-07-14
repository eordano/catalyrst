import type { PoolClient } from "pg";

import {
  FoundryStateError,
  assertRate,
  getPool,
  logAction,
  sidBadge,
  withTx,
} from "./db.server";
import { requireHost } from "./roles.server";
import type { SessionOccurrence, SessionSeriesInput } from "./types";

// A session series is a schedule, not a list of events. Weekly occurrences are
// derived arithmetically from first_at at read time within a capped horizon and
// are NEVER materialized — a stored occurrence that never happened would be
// fiction. An RSVP is the pledge pattern keyed to a specific derived occurrence,
// server-validated against the same derivation before it is accepted. The count
// shown is count(*) over session_rsvp and nothing else, and the copy is always
// "said they'll come" — never "attended", which no row could support.

const HORIZON_DAYS = 28;
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;
const SCHEDULE_LABEL = "from the series schedule";

export const SESSION_LIMITS = { title: 80, body: 280 } as const;

type Cadence = "once" | "weekly";

/** The occurrences of a series that fall inside [now, now + horizon]. A `once`
 *  series yields at most one; a `weekly` one yields each 7-day step in range. */
export function deriveOccurrences(
  firstAtMs: number,
  cadence: Cadence,
  nowMs: number,
  horizonEndMs: number,
): string[] {
  const occs: string[] = [];
  if (cadence === "once") {
    if (firstAtMs >= nowMs && firstAtMs <= horizonEndMs) {
      occs.push(new Date(firstAtMs).toISOString());
    }
    return occs;
  }
  let k = Math.max(0, Math.floor((nowMs - firstAtMs) / WEEK_MS));
  const k0 = k;
  for (;;) {
    const occ = firstAtMs + k * WEEK_MS;
    if (occ > horizonEndMs) break;
    if (occ >= nowMs) occs.push(new Date(occ).toISOString());
    k += 1;
    if (k - k0 > Math.ceil(HORIZON_DAYS / 7) + 1) break;
  }
  return occs;
}

type SeriesDbRow = {
  id: string;
  title: string;
  body: string;
  scene_id: string | null;
  scene_title: string | null;
  cadence: string;
  first_at: Date | string;
  duration_minutes: number;
  created_by_sid: string;
  retired_at: Date | string | null;
};

function ms(value: Date | string): number {
  return value instanceof Date ? value.getTime() : new Date(value).getTime();
}

function cadenceOf(value: string): Cadence {
  return value === "weekly" ? "weekly" : "once";
}

/**
 * Upcoming occurrences across every active series, each carrying its measured
 * RSVP count and whether the viewer is among them. Occurrences are derived, not
 * stored; the label says so. `sceneId` narrows to the series that name that
 * scene — derivation, horizon and RSVP counting are identical either way.
 */
export async function listUpcoming(
  viewerSid: string,
  sceneId?: string | null,
): Promise<SessionOccurrence[]> {
  const pool = getPool();
  const seriesRes = sceneId
    ? await pool.query<SeriesDbRow>(
        `SELECT ss.id, ss.title, ss.body, ss.scene_id, s.title AS scene_title,
                ss.cadence, ss.first_at, ss.duration_minutes, ss.created_by_sid,
                ss.retired_at
           FROM foundry.session_series ss
           LEFT JOIN foundry.scene s ON s.id = ss.scene_id
          WHERE ss.retired_at IS NULL AND ss.scene_id = $1
          ORDER BY ss.first_at`,
        [sceneId],
      )
    : await pool.query<SeriesDbRow>(
        `SELECT ss.id, ss.title, ss.body, ss.scene_id, s.title AS scene_title,
                ss.cadence, ss.first_at, ss.duration_minutes, ss.created_by_sid,
                ss.retired_at
           FROM foundry.session_series ss
           LEFT JOIN foundry.scene s ON s.id = ss.scene_id
          WHERE ss.retired_at IS NULL
          ORDER BY ss.first_at`,
      );
  if (seriesRes.rows.length === 0) return [];

  const now = Date.now();
  const horizonEnd = now + HORIZON_DAYS * 24 * 60 * 60 * 1000;

  type Pending = { series: SeriesDbRow; occurrenceAt: string };
  const pending: Pending[] = [];
  for (const series of seriesRes.rows) {
    const occs = deriveOccurrences(
      ms(series.first_at),
      cadenceOf(series.cadence),
      now,
      horizonEnd,
    );
    for (const occurrenceAt of occs) pending.push({ series, occurrenceAt });
  }
  if (pending.length === 0) return [];

  const seriesIds = [...new Set(pending.map((p) => p.series.id))];
  const rsvpRes = await pool.query<{
    series_id: string;
    occurrence_at: Date | string;
    n: number;
    mine: boolean;
  }>(
    `SELECT series_id, occurrence_at, count(*)::int AS n,
            bool_or(sid = $2) IS TRUE AS mine
       FROM foundry.session_rsvp
      WHERE series_id = ANY($1)
      GROUP BY series_id, occurrence_at`,
    [seriesIds, viewerSid],
  );
  const rsvpKey = (seriesId: string, occ: string) => `${seriesId}#${occ}`;
  const counts = new Map<string, { n: number; mine: boolean }>();
  for (const r of rsvpRes.rows) {
    counts.set(rsvpKey(r.series_id, new Date(ms(r.occurrence_at)).toISOString()), {
      n: Number(r.n),
      mine: r.mine,
    });
  }

  const hostSids = [...new Set(pending.map((p) => p.series.created_by_sid))];
  const names = new Map<string, string>();
  const pres = await pool.query<{ sid: string; display_name: string }>(
    `SELECT q.sid, p.display_name
       FROM unnest($1::text[]) AS q(sid)
       LEFT JOIN foundry.sid_alias al ON al.alias_sid = q.sid
       JOIN foundry.persona p ON p.sid = COALESCE(al.persona_sid, q.sid)`,
    [hostSids],
  );
  for (const p of pres.rows) names.set(p.sid, p.display_name);

  const out: SessionOccurrence[] = pending.map(({ series, occurrenceAt }) => {
    const c = counts.get(rsvpKey(series.id, occurrenceAt));
    const hostName = names.get(series.created_by_sid);
    return {
      seriesId: series.id,
      title: series.title,
      body: series.body,
      sceneId: series.scene_id,
      sceneTitle: series.scene_title,
      cadence: cadenceOf(series.cadence),
      occurrenceAt,
      durationMinutes: Number(series.duration_minutes),
      host: hostName
        ? { name: hostName }
        : { badge: sidBadge(series.created_by_sid) },
      rsvpCount: c ? c.n : 0,
      viewerRsvped: c ? c.mine : false,
      label: SCHEDULE_LABEL,
    };
  });
  out.sort((a, b) => a.occurrenceAt.localeCompare(b.occurrenceAt));
  return out;
}

export function validateSeriesInput(input: SessionSeriesInput): string | null {
  const title = input.title.trim();
  if (title.length === 0) return "Give the session a title.";
  if (title.length > SESSION_LIMITS.title) {
    return `Titles are ${SESSION_LIMITS.title} characters or fewer.`;
  }
  if (input.body.length > SESSION_LIMITS.body) {
    return `Descriptions are ${SESSION_LIMITS.body} characters or fewer.`;
  }
  if (input.cadence !== "once" && input.cadence !== "weekly") {
    return "Pick a cadence.";
  }
  const firstAt = Date.parse(input.firstAt);
  if (!Number.isFinite(firstAt)) return "Give the session a start time.";
  if (input.durationMinutes < 15 || input.durationMinutes > 480) {
    return "Duration must be between 15 and 480 minutes.";
  }
  return null;
}

export async function createSeries({
  sid,
  input,
  ip,
}: {
  sid: string;
  input: SessionSeriesInput;
  ip?: string | null;
}): Promise<{ id: string }> {
  const invalid = validateSeriesInput(input);
  if (invalid) throw new FoundryStateError(invalid);
  assertRate(sid, ip);
  return withTx(async (client) => {
    await requireHost(client, sid);
    if (input.sceneId) {
      const scene = await client.query(
        `SELECT 1 FROM foundry.scene WHERE id = $1`,
        [input.sceneId],
      );
      if (scene.rows.length === 0) {
        throw new FoundryStateError("That scene is not in the games registry.");
      }
    }
    const res = await client.query<{ id: string }>(
      `INSERT INTO foundry.session_series
         (title, body, scene_id, cadence, first_at, duration_minutes, created_by_sid)
       VALUES ($1, $2, $3, $4, $5::timestamptz, $6, $7)
       RETURNING id`,
      [
        input.title.trim(),
        input.body.trim(),
        input.sceneId,
        input.cadence,
        input.firstAt,
        input.durationMinutes,
        sid,
      ],
    );
    const id = res.rows[0].id;
    await logAction(client, {
      sid,
      action: "schedule_session",
      subject: id,
      detail: { title: input.title.trim(), cadence: input.cadence },
    });
    return { id };
  });
}

export async function retireSeries({
  sid,
  seriesId,
  ip,
}: {
  sid: string;
  seriesId: string;
  ip?: string | null;
}): Promise<void> {
  assertRate(sid, ip);
  await withTx(async (client) => {
    await requireHost(client, sid);
    const upd = await client.query(
      `UPDATE foundry.session_series
          SET retired_at = now(), retired_by_sid = $1
        WHERE id = $2 AND retired_at IS NULL`,
      [sid, seriesId],
    );
    if (upd.rowCount === 0) {
      throw new FoundryStateError("That series is not active.");
    }
    await logAction(client, {
      sid,
      action: "retire_session",
      subject: seriesId,
      detail: {},
    });
  });
}

async function loadOccurrenceGuard(
  client: PoolClient,
  seriesId: string,
  occurrenceAt: string,
): Promise<void> {
  const res = await client.query<{
    cadence: string;
    first_at: Date | string;
    retired_at: Date | string | null;
  }>(
    `SELECT cadence, first_at, retired_at
       FROM foundry.session_series WHERE id = $1 FOR UPDATE`,
    [seriesId],
  );
  const series = res.rows[0];
  if (!series || series.retired_at !== null) {
    throw new FoundryStateError("That session is no longer on the calendar.");
  }
  const now = Date.now();
  const horizonEnd = now + HORIZON_DAYS * 24 * 60 * 60 * 1000;
  const parsed = Date.parse(occurrenceAt);
  if (!Number.isFinite(parsed)) {
    throw new FoundryStateError("That is not a valid session time.");
  }
  const canonical = new Date(parsed).toISOString();
  const valid = deriveOccurrences(
    ms(series.first_at),
    cadenceOf(series.cadence),
    now,
    horizonEnd,
  );
  if (!valid.includes(canonical)) {
    throw new FoundryStateError("That session time is not on the schedule.");
  }
}

export async function rsvp({
  seriesId,
  occurrenceAt,
  sid,
  ip,
}: {
  seriesId: string;
  occurrenceAt: string;
  sid: string;
  ip?: string | null;
}): Promise<{ added: boolean }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    await loadOccurrenceGuard(client, seriesId, occurrenceAt);
    const canonical = new Date(Date.parse(occurrenceAt)).toISOString();
    const ins = await client.query(
      `INSERT INTO foundry.session_rsvp (series_id, occurrence_at, sid)
       VALUES ($1, $2::timestamptz, $3)
       ON CONFLICT DO NOTHING`,
      [seriesId, canonical, sid],
    );
    const added = (ins.rowCount ?? 0) > 0;
    if (added) {
      await logAction(client, {
        sid,
        action: "rsvp_session",
        subject: seriesId,
        detail: { occurrence_at: canonical },
      });
    }
    return { added };
  });
}

export async function withdrawRsvp({
  seriesId,
  occurrenceAt,
  sid,
  ip,
}: {
  seriesId: string;
  occurrenceAt: string;
  sid: string;
  ip?: string | null;
}): Promise<{ deleted: boolean }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    const canonical = new Date(Date.parse(occurrenceAt)).toISOString();
    const del = await client.query(
      `DELETE FROM foundry.session_rsvp
        WHERE series_id = $1 AND occurrence_at = $2::timestamptz AND sid = $3`,
      [seriesId, canonical, sid],
    );
    const deleted = (del.rowCount ?? 0) > 0;
    if (deleted) {
      await logAction(client, {
        sid,
        action: "withdraw_rsvp",
        subject: seriesId,
        detail: { occurrence_at: canonical },
      });
    }
    return { deleted };
  });
}
