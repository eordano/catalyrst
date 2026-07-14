import {
  FoundryStateError,
  assertRate,
  getPool,
  logAction,
  withTx,
} from "./db.server";

export const REQUEST_LIMITS = { title: 80, body: 280, source: 60 } as const;

export type RequestOrigin = "imported" | "visitor";

export type RequestBoardRow = {
  id: string;
  title: string;
  body: string;
  source: string;
  status: "open" | "closed";
  pledges: number;
  pledgedByMe: boolean;
  origin: RequestOrigin;
  createdAt: string;
};

type RequestDbRow = {
  id: string;
  title: string;
  body: string;
  source: string;
  status: string;
  pledges: number;
  pledged_by_me: boolean;
  origin: string;
  created_at: Date;
};

// The board is the only surface that reads the exchange back out. It is driven
// FROM foundry.request, so a pledge (or any action_log row) whose request_id no
// longer resolves is simply absent — a dangling reference can never render a row
// or a broken link here. action_log itself is a write-only audit trail with no
// reader and no FK, by design.
const BOARD_SQL = `
  SELECT r.id, r.title, r.body, r.source, r.status, r.origin, r.created_at,
         count(p.sid)::int AS pledges,
         bool_or(p.sid = $1) IS TRUE AS pledged_by_me
    FROM foundry.request r
    LEFT JOIN foundry.pledge p ON p.request_id = r.id
   GROUP BY r.id
   ORDER BY (r.status = 'open') DESC, pledges DESC, r.created_at DESC`;

export async function listRequests(sid: string): Promise<RequestBoardRow[]> {
  const res = await getPool().query<RequestDbRow>(BOARD_SQL, [sid]);
  return res.rows.map((r) => ({
    id: r.id,
    title: r.title,
    body: r.body,
    source: r.source,
    status: r.status === "open" ? "open" : "closed",
    pledges: Number(r.pledges),
    pledgedByMe: r.pledged_by_me,
    origin: r.origin === "visitor" ? "visitor" : "imported",
    createdAt: r.created_at.toISOString(),
  }));
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
    if (req.rows[0].status !== "open") {
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
  if (source.length === 0) errors.source = "Name the community you are posting from";
  else if (source.length > REQUEST_LIMITS.source)
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
      [title.trim(), body.trim(), source.trim(), sid],
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
