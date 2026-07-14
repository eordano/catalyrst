import { createHash } from "node:crypto";

import type { Pool, PoolClient } from "pg";

import {
  FoundryStateError,
  PRINCIPAL_SET_SQL,
  assertRate,
  canonicalSidTx,
  getPool,
  logAction,
  sidBadge,
  withTx,
} from "./db.server";
import { requireRole } from "./roles.server";
import type { MarketCellSlug } from "./types";

export const REQUEST_LIMITS = { title: 80, body: 280, source: 60 } as const;

export type RequestOrigin = "imported" | "visitor";

// The true, three-state moderation status of a request. A request is 'approved'
// or 'closed' only by an admin through moderateRequest; the card-facing `status`
// below stays open|closed (whether it still takes pledges), but `moderation`
// carries the real state so an 'approved' row is never silently read as 'closed'.
export type RequestModerationState = "open" | "approved" | "closed";

// An actor on the wire is a persona name (when claimed) or the honest 4-hex
// badge — the raw sid of another visitor is never serialized.
export type RequestActorLabel = { name: string } | { badge: string };

export type RequestModeration = {
  state: RequestModerationState;
  actor: RequestActorLabel | null;
  at: string | null;
};

export type RequestBoardRow = {
  id: string;
  title: string;
  body: string;
  source: string;
  status: "open" | "closed";
  moderation: RequestModeration;
  pledges: number;
  pledgedByMe: boolean;
  origin: RequestOrigin;
  author: RequestActorLabel | null;
  /** Whether the viewer's persona (any of its sids) authored this ask —
   *  drives the edit affordance, never a display value. */
  authoredByMe: boolean;
  createdAt: string;
  /** When the author last revised the wording; null = never edited. */
  editedAt: string | null;
  sourceUrl: string | null;
  sourcedAt: string | null;
  /** This program's reading of the ask (cell null = read, fits no cell);
   *  null = no reading row exists — honestly not read. */
  reading: { cell: MarketCellSlug | null; readAt: string } | null;
};

type RequestDbRow = {
  id: string;
  title: string;
  body: string;
  source: string;
  status: string;
  sid: string | null;
  author_name: string | null;
  mod_action: string | null;
  mod_at: Date | null;
  mod_sid: string | null;
  mod_name: string | null;
  pledges: number;
  pledged_by_me: boolean;
  authored_by_me: boolean;
  origin: string;
  created_at: Date;
  edited_at: Date | null;
  source_url: string | null;
  sourced_at: Date | null;
  reading_cell: MarketCellSlug | null;
  reading_read_at: string | null;
};

// The board is the only surface that reads the exchange back out. It is driven
// FROM foundry.request, so a pledge (or any action_log row) whose request_id no
// longer resolves is simply absent — a dangling reference can never render a row
// or a broken link here. The author persona, the latest moderation event and
// this program's reading of the ask are LEFT-joined, so a request with no
// claimed author, no moderation or no reading just carries nulls. Every sid
// resolves through the alias layer to its persona, so an act from a since-lost
// session still names its owner; pledges count distinct principals, and
// authored_by_me compares the author's canonical sid against the viewer's
// whole principal set — a rebound session still owns its own asks. Only
// persona names and badges cross the wire, never a raw sid.
const BOARD_SQL = `
  SELECT r.id, r.title, r.body, r.source, r.status, r.origin, r.created_at, r.sid,
         r.edited_at, r.source_url, r.sourced_at,
         count(DISTINCT COALESCE(pal.persona_sid, p.sid))::int AS pledges,
         bool_or(COALESCE(pal.persona_sid, p.sid) = $1) IS TRUE AS pledged_by_me,
         (r.sid IS NOT NULL
           AND COALESCE(aal.persona_sid, r.sid) IN (${PRINCIPAL_SET_SQL})
         ) AS authored_by_me,
         ap.display_name AS author_name,
         mod.action AS mod_action, mod.at AS mod_at,
         mod.actor_sid AS mod_sid, mp.display_name AS mod_name,
         rr.cell AS reading_cell,
         to_char(rr.read_at, 'YYYY-MM-DD') AS reading_read_at
    FROM foundry.request r
    LEFT JOIN foundry.pledge p ON p.request_id = r.id
    LEFT JOIN foundry.sid_alias pal ON pal.alias_sid = p.sid
    LEFT JOIN foundry.sid_alias aal ON aal.alias_sid = r.sid
    LEFT JOIN foundry.persona ap ON ap.sid = COALESCE(aal.persona_sid, r.sid)
    LEFT JOIN LATERAL (
      SELECT a.action, a.at, a.sid AS actor_sid
        FROM foundry.action_log a
       WHERE a.subject = r.id
         AND a.action IN ('approve_request','close_request')
       ORDER BY a.at DESC
       LIMIT 1
    ) mod ON true
    LEFT JOIN foundry.sid_alias mal ON mal.alias_sid = mod.actor_sid
    LEFT JOIN foundry.persona mp ON mp.sid = COALESCE(mal.persona_sid, mod.actor_sid)
    LEFT JOIN foundry.request_reading rr ON rr.request_id = r.id
   GROUP BY r.id, r.sid, r.edited_at, r.source_url, r.sourced_at,
            aal.persona_sid, ap.display_name,
            mod.action, mod.at, mod.actor_sid, mp.display_name,
            rr.cell, rr.read_at
   ORDER BY (r.status = 'open') DESC, pledges DESC,
            COALESCE(r.sourced_at, r.created_at) DESC, r.id`;

// The same row, filtered to one ask — derived from BOARD_SQL so the two can
// never drift apart in what they join or expose.
const DETAIL_SQL = BOARD_SQL.replace(
  "   GROUP BY",
  "   WHERE r.id = $2\n   GROUP BY",
);

function actorLabel(
  name: string | null,
  sid: string | null,
): RequestActorLabel | null {
  if (name) return { name };
  if (sid) return { badge: sidBadge(sid) };
  return null;
}

function moderationState(status: string): RequestModerationState {
  if (status === "approved") return "approved";
  if (status === "closed") return "closed";
  return "open";
}

function toBoardRow(r: RequestDbRow): RequestBoardRow {
  return {
    id: r.id,
    title: r.title,
    body: r.body,
    source: r.source,
    status: r.status === "closed" ? "closed" : "open",
    moderation: {
      state: moderationState(r.status),
      actor: r.mod_action ? actorLabel(r.mod_name, r.mod_sid) : null,
      at: r.mod_at ? r.mod_at.toISOString() : null,
    },
    pledges: Number(r.pledges),
    pledgedByMe: r.pledged_by_me,
    authoredByMe: r.authored_by_me === true,
    origin: r.origin === "visitor" ? "visitor" : "imported",
    author: actorLabel(r.author_name, r.sid),
    createdAt: r.created_at.toISOString(),
    editedAt: r.edited_at ? r.edited_at.toISOString() : null,
    sourceUrl: r.source_url,
    sourcedAt: r.sourced_at ? r.sourced_at.toISOString() : null,
    // read_at is NOT NULL, so a non-null reading_read_at ⟺ a reading row
    // exists — a null-cell reading ("fits no cell") is still a reading.
    reading:
      r.reading_read_at !== null
        ? { cell: r.reading_cell, readAt: r.reading_read_at }
        : null,
  };
}

/** One ask by its public id — the same row shape the board renders. Null when
 *  no such ask exists; the route answers that with an honest 404. */
export async function getRequest(
  sid: string,
  id: string,
): Promise<RequestBoardRow | null> {
  const res = await getPool().query<RequestDbRow>(DETAIL_SQL, [sid, id]);
  const row = res.rows[0];
  return row ? toBoardRow(row) : null;
}

/** One entry per pledge on an ask, oldest first: the claimed persona name when
 *  one exists, the 4-hex badge otherwise — never a raw sid. */
export type PledgeListRow = { actor: RequestActorLabel; at: string };

export async function listPledges(requestId: string): Promise<PledgeListRow[]> {
  const res = await getPool().query<{
    sid: string;
    display_name: string | null;
    created_at: Date;
  }>(
    `SELECT p.sid, per.display_name, p.created_at
       FROM foundry.pledge p
       LEFT JOIN foundry.sid_alias al ON al.alias_sid = p.sid
       LEFT JOIN foundry.persona per ON per.sid = COALESCE(al.persona_sid, p.sid)
      WHERE p.request_id = $1
      ORDER BY p.created_at ASC, p.sid`,
    [requestId],
  );
  return res.rows.map((r) => ({
    actor: actorLabel(r.display_name, r.sid) ?? { badge: sidBadge(r.sid) },
    at: r.created_at.toISOString(),
  }));
}

export async function listRequests(sid: string): Promise<RequestBoardRow[]> {
  const res = await getPool().query<RequestDbRow>(BOARD_SQL, [sid]);
  return res.rows.map(toBoardRow);
}

/**
 * The one path that moves a request out of 'open'. Admin-gated INSIDE the same
 * transaction as the status write, so a viewer who lost the role mid-request
 * cannot slip a moderation through; the event is recorded in action_log, which
 * the board reads back as the moderation line.
 */
export async function moderateRequest({
  requestId,
  sid,
  verdict,
  ip,
}: {
  requestId: string;
  sid: string;
  verdict: "approved" | "closed";
  ip?: string | null;
}): Promise<void> {
  // The parameterized UPDATE below would accept any status value at runtime —
  // 'open' would silently REOPEN a request while the action ternary logs it as
  // close_request. The type says approved|closed; enforce it.
  if (verdict !== "approved" && verdict !== "closed") {
    throw new FoundryStateError("Unknown verdict.");
  }
  assertRate(sid, ip);
  await withTx(async (client) => {
    await requireRole(client, sid, "admin");
    const res = await client.query<{ title: string }>(
      `UPDATE foundry.request SET status = $2 WHERE id = $1 RETURNING title`,
      [requestId, verdict],
    );
    if (res.rowCount === 0) {
      throw new FoundryStateError("That request no longer exists.");
    }
    await logAction(client, {
      sid,
      action: verdict === "approved" ? "approve_request" : "close_request",
      subject: requestId,
      detail: { verdict, title: res.rows[0].title },
    });
  });
}

export type ImportedAsk = {
  title: string;
  body: string;
  author: string;
  url: string;
  date: string;
};

/** A fixture date with no clock component — the source recorded a day, not an
 *  instant, and the stored row says so instead of minting a midnight. */
export function isDateOnly(date: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(date.trim());
}

/**
 * The persona names an imported ask's author handle reserves: the handle
 * verbatim, plus the bare name behind a reddit-style "u/" prefix (the slash is
 * outside the persona charset, so only the bare form could ever be claimed).
 */
export function askHandleVariants(author: string): string[] {
  const handle = author.trim();
  if (handle === "") return [];
  const bare = handle.replace(/^u\//i, "");
  return bare === handle ? [handle] : [handle, bare];
}

function isHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

/** The one id an imported ask ever gets: derived from its permalink, so every
 *  table keyed on the ask (readings included) derives the same key. */
export function askIdForUrl(url: string): string {
  return "ask-" + createHash("sha256").update(url).digest("hex").slice(0, 16);
}

/**
 * The one path an imported ask lands through — used by both the import script
 * and the e2e suite; createRequest stays visitor-only (it requires a sid and
 * enforces visitor length limits that would truncate a verbatim quote).
 *
 * Each row is verbatim public speech: `body` the quote, `source` the author's
 * public handle, `source_url` the permalink, `sourced_at` the original post
 * date, `sid` NULL. A row missing its permalink is un-importable — the working
 * link IS the origin proof. Idempotency is keyed on the permalink; re-running
 * updates the content columns and never touches `status`, so a re-import can
 * never revert a moderation. No action_log row: an import has no session and
 * action_log.sid is NOT NULL, consistent with the scene import.
 */
export async function upsertImportedAsks(
  db: Pool | PoolClient,
  asks: ImportedAsk[],
): Promise<{ upserted: number }> {
  let upserted = 0;
  for (const ask of asks) {
    if (!ask.url || !isHttpUrl(ask.url)) {
      throw new FoundryStateError(
        `An imported ask needs a public http(s) permalink; got "${ask.url}".`,
      );
    }
    const id = askIdForUrl(ask.url);
    await db.query(
      `INSERT INTO foundry.request
         (id, title, body, source, source_url, sourced_at, sourced_date_only, origin, sid)
       VALUES ($1, $2, $3, $4, $5, $6::timestamptz, $7, 'import', NULL)
       ON CONFLICT (id) DO UPDATE SET
         title             = EXCLUDED.title,
         body              = EXCLUDED.body,
         source            = EXCLUDED.source,
         source_url        = EXCLUDED.source_url,
         sourced_at        = EXCLUDED.sourced_at,
         sourced_date_only = EXCLUDED.sourced_date_only`,
      [id, ask.title, ask.body, ask.author, ask.url, ask.date, isDateOnly(ask.date)],
    );
    // The author's handle becomes a reserved persona name, so nobody can claim
    // it and stand next to the quote. ON CONFLICT DO NOTHING keeps re-imports
    // from resurrecting a handle an operator released to its returning author.
    for (const handle of askHandleVariants(ask.author)) {
      await db.query(
        `INSERT INTO foundry.reserved_handle (handle, source_request_id)
         VALUES ($1, $2)
         ON CONFLICT DO NOTHING`,
        [handle, id],
      );
    }
    upserted += 1;
  }
  return { upserted };
}

async function pledgeCount(
  query: (sql: string, values: unknown[]) => Promise<{ rows: { n: number }[] }>,
  requestId: string,
): Promise<number> {
  const res = await query(
    `SELECT count(*)::int AS n FROM foundry.pledge WHERE request_id = $1`,
    [requestId],
  );
  return Number(res.rows[0]?.n ?? 0);
}

export async function pledgeRequest({
  requestId,
  sid,
  ip,
}: {
  requestId: string;
  sid: string;
  ip?: string | null;
}): Promise<{ pledges: number; alreadyPledged: boolean }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    const req = await client.query<{ status: string }>(
      `SELECT status FROM foundry.request WHERE id = $1 FOR UPDATE`,
      [requestId],
    );
    if (req.rows.length === 0) {
      throw new FoundryStateError("That request no longer exists");
    }
    // Only 'closed' stops the cohort — an approved request keeps taking pledges.
    if (req.rows[0].status === "closed") {
      throw new FoundryStateError("That request is closed to new pledges");
    }
    const ins = await client.query(
      `INSERT INTO foundry.pledge (request_id, sid) VALUES ($1, $2)
       ON CONFLICT DO NOTHING`,
      [requestId, sid],
    );
    const alreadyPledged = ins.rowCount === 0;
    if (!alreadyPledged) {
      await logAction(client, {
        sid,
        action: "pledge",
        subject: requestId,
        detail: { request_id: requestId },
      });
    }
    const pledges = await pledgeCount(
      (sql, values) => client.query(sql, values),
      requestId,
    );
    return { pledges, alreadyPledged };
  });
}

export async function withdrawPledge({
  requestId,
  sid,
  ip,
}: {
  requestId: string;
  sid: string;
  ip?: string | null;
}): Promise<{ pledges: number; deleted: boolean }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    const del = await client.query(
      `DELETE FROM foundry.pledge WHERE request_id = $1 AND sid = $2`,
      [requestId, sid],
    );
    // A second withdraw deletes nothing, so it is not a mutation: no action_log
    // row, and the caller has the flag it needs to skip the event too.
    const deleted = (del.rowCount ?? 0) > 0;
    if (deleted) {
      await logAction(client, {
        sid,
        action: "withdraw_pledge",
        subject: requestId,
        detail: { request_id: requestId },
      });
    }
    const pledges = await pledgeCount(
      (sql, values) => client.query(sql, values),
      requestId,
    );
    return { pledges, deleted };
  });
}

// What a visitor row's `source` says when the asker left the provenance field
// blank: the ask originated right here, first-hand. Forcing the field invited
// invented communities — fabricated provenance is worse than declared absence.
export const SELF_SOURCE = "my own ask, made here";

export function validateRequest(input: {
  title: string;
  body: string;
  source: string;
}): Record<string, string> {
  const errors: Record<string, string> = {};
  const title = input.title.trim();
  const body = input.body.trim();
  const source = input.source.trim();
  if (title.length === 0) errors.title = "Give the request a title";
  else if (title.length > REQUEST_LIMITS.title)
    errors.title = `Titles are ${REQUEST_LIMITS.title} characters or fewer`;
  if (body.length === 0) errors.body = "Say what you wish existed";
  else if (body.length > REQUEST_LIMITS.body)
    errors.body = `Requests are ${REQUEST_LIMITS.body} characters or fewer`;
  // An empty source is a first-person ask, not an error: createRequest stores
  // SELF_SOURCE for it rather than making the asker invent a community.
  if (source.length > REQUEST_LIMITS.source)
    errors.source = `Sources are ${REQUEST_LIMITS.source} characters or fewer`;
  return errors;
}

export async function createRequest({
  title,
  body,
  source,
  sid,
  ip,
}: {
  title: string;
  body: string;
  source: string;
  sid: string;
  ip?: string | null;
}): Promise<{ id: string }> {
  const errors = validateRequest({ title, body, source });
  const first = Object.values(errors)[0];
  if (first) throw new FoundryStateError(first);
  assertRate(sid, ip);
  return withTx(async (client) => {
    const res = await client.query<{ id: string }>(
      `INSERT INTO foundry.request (title, body, source, origin, sid)
       VALUES ($1, $2, $3, 'visitor', $4) RETURNING id`,
      [title.trim(), body.trim(), source.trim() || SELF_SOURCE, sid],
    );
    const id = res.rows[0].id;
    await logAction(client, {
      sid,
      action: "post_request",
      subject: id,
      detail: { title: title.trim() },
    });
    return { id };
  });
}

export const EDIT_NOT_AUTHOR =
  "Only this ask's author can edit it — pledge to it, or post your own ask.";
export const EDIT_IMPORTED =
  "This ask is quoted verbatim from a public post and keeps its author's wording — post your own ask instead.";

/**
 * The author's own revision of a visitor ask — the one exchange write that
 * changes an existing request row in place. Ownership compares the canonical
 * sids of author and viewer, so an author whose session was rebound onto a
 * persona still reaches their own ask from any of its sids. Imported asks are
 * refused: they are verbatim public speech and re-import must never collide
 * with a visitor's words. The prior wording is recorded in action_log before
 * the overwrite, and edited_at marks the row so every reader sees the ask
 * was revised.
 */
export async function editRequest({
  requestId,
  title,
  body,
  sid,
  ip,
}: {
  requestId: string;
  title: string;
  body: string;
  sid: string;
  ip?: string | null;
}): Promise<void> {
  // source is not editable: for a visitor row it is the asker's provenance
  // statement from posting time, which an edit cannot rewrite.
  const errors = validateRequest({ title, body, source: "" });
  const first = Object.values(errors)[0];
  if (first) throw new FoundryStateError(first);
  assertRate(sid, ip);
  await withTx(async (client) => {
    const res = await client.query<{
      title: string;
      body: string;
      origin: string;
      sid: string | null;
    }>(
      `SELECT title, body, origin, sid FROM foundry.request
        WHERE id = $1 FOR UPDATE`,
      [requestId],
    );
    const row = res.rows[0];
    if (!row) throw new FoundryStateError("That request no longer exists.");
    if (row.origin !== "visitor") throw new FoundryStateError(EDIT_IMPORTED);
    const owner =
      row.sid !== null &&
      (await canonicalSidTx(client, sid)) ===
        (await canonicalSidTx(client, row.sid));
    if (!owner) throw new FoundryStateError(EDIT_NOT_AUTHOR);
    if (title.trim() === row.title && body.trim() === row.body) {
      throw new FoundryStateError(
        "Nothing changed — the ask already reads exactly like this.",
      );
    }
    await client.query(
      `UPDATE foundry.request SET title = $2, body = $3, edited_at = now()
        WHERE id = $1`,
      [requestId, title.trim(), body.trim()],
    );
    await logAction(client, {
      sid,
      action: "edit_request",
      subject: requestId,
      detail: { prev_title: row.title, prev_body: row.body },
    });
  });
}
