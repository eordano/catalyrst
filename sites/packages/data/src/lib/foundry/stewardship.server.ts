import {
  FoundryStateError,
  assertRate,
  getPool,
  logAction,
  sidBadge,
  withTx,
} from "./db.server";
import { setConsent } from "./consent.server";
import { moderateRequest } from "./exchange.server";
import { requireRole } from "./roles.server";
import type {
  AppealRow,
  ConsentState,
  ConsentTopic,
  DecisionRow,
} from "./types";

// The stewardship console. A visitor may only appeal a decision that REALLY
// touched them and REALLY happened: the appeal subject is re-derived from
// listMyDecisions server-side, so an appeal can never name an invented decision
// (the BOARD_SQL "drive from the fact" discipline). Resolution is admin-only and
// carries a required note. Consent lanes delegate to consent.server so the
// latest-row semantics live in one place. Request moderation delegates to
// exchange.moderateRequest — there is no second mutation path.

const DECISIONS_SQL = `
  SELECT 'request'::text AS kind, r.id AS id, r.title AS label,
         r.created_at AS at, r.status AS detail
    FROM foundry.request r
   WHERE r.sid = $1 AND r.status <> 'open'
  UNION ALL
  SELECT 'role_grant'::text, g.id::text, g.role,
         coalesce(g.revoked_at, g.created_at), coalesce(g.revoke_note, 'revoked')
    FROM foundry.role_grant g
   WHERE g.sid = $1 AND g.revoked_at IS NOT NULL
  UNION ALL
  SELECT 'session_series'::text, ss.id, ss.title, ss.retired_at, 'retired'::text
    FROM foundry.session_series ss
   WHERE ss.retired_at IS NOT NULL
     AND (ss.created_by_sid = $1
          OR EXISTS (SELECT 1 FROM foundry.session_rsvp v
                      WHERE v.series_id = ss.id AND v.sid = $1))
  ORDER BY at DESC`;

type DecisionDbRow = {
  kind: string;
  id: string;
  label: string;
  at: Date | string;
  detail: string;
};

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

function decisionKind(value: string): DecisionRow["kind"] {
  if (value === "role_grant") return "role_grant";
  if (value === "session_series") return "session_series";
  return "request";
}

/** The decisions that both happened and touch this session — the only subjects
 *  an appeal may name. */
export async function listMyDecisions(sid: string): Promise<DecisionRow[]> {
  const res = await getPool().query<DecisionDbRow>(DECISIONS_SQL, [sid]);
  return res.rows.map((r) => ({
    kind: decisionKind(r.kind),
    id: r.id,
    label: r.label,
    at: iso(r.at),
    detail: r.detail,
  }));
}

export async function changeConsent({
  sid,
  topic,
  state,
  ip,
}: {
  sid: string;
  topic: ConsentTopic;
  state: ConsentState;
  ip?: string | null;
}): Promise<void> {
  assertRate(sid, ip);
  await withTx(async (client) => {
    await setConsent(client, { sid, topic, state });
  });
}

export const APPEAL_LIMITS = { body: 400 } as const;

export async function fileAppeal({
  sid,
  subjectKind,
  subjectId,
  body,
  ip,
}: {
  sid: string;
  subjectKind: DecisionRow["kind"];
  subjectId: string;
  body: string;
  ip?: string | null;
}): Promise<{ id: string }> {
  const text = body.trim();
  if (text.length === 0) throw new FoundryStateError("Say what you are contesting.");
  if (text.length > APPEAL_LIMITS.body) {
    throw new FoundryStateError(`Appeals are ${APPEAL_LIMITS.body} characters or fewer.`);
  }
  assertRate(sid, ip);
  return withTx(async (client) => {
    // Re-derive the appellant's real decisions inside the tx: the subject must be
    // one of them, so a hand-crafted subject that never touched this session (or
    // never happened) is refused here, not just hidden by the UI.
    const decisions = await client.query<{ kind: string; id: string }>(
      DECISIONS_SQL,
      [sid],
    );
    const allowed = decisions.rows.some(
      (d) => decisionKind(d.kind) === subjectKind && d.id === subjectId,
    );
    if (!allowed) {
      throw new FoundryStateError("That decision is not one you can appeal.");
    }
    const ins = await client.query<{ id: string }>(
      `INSERT INTO foundry.appeal (sid, subject_kind, subject_id, body)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (sid, subject_kind, subject_id) WHERE status = 'open'
         DO NOTHING
       RETURNING id`,
      [sid, subjectKind, subjectId, text],
    );
    const id = ins.rows[0]?.id;
    if (!id) {
      throw new FoundryStateError("You already have an open appeal for that decision.");
    }
    await logAction(client, {
      sid,
      action: "file_appeal",
      subject: id,
      detail: { subject_kind: subjectKind, subject_id: subjectId },
    });
    return { id };
  });
}

export async function withdrawAppeal({
  sid,
  appealId,
  ip,
}: {
  sid: string;
  appealId: string;
  ip?: string | null;
}): Promise<void> {
  assertRate(sid, ip);
  await withTx(async (client) => {
    const upd = await client.query(
      `UPDATE foundry.appeal SET status = 'withdrawn'
        WHERE id = $1 AND sid = $2 AND status = 'open'`,
      [appealId, sid],
    );
    if (upd.rowCount === 0) {
      throw new FoundryStateError("That appeal is not open.");
    }
    await logAction(client, {
      sid,
      action: "withdraw_appeal",
      subject: appealId,
      detail: {},
    });
  });
}

type AppealDbRow = {
  id: string;
  sid: string;
  subject_kind: string;
  subject_id: string;
  subject_label: string | null;
  body: string;
  status: string;
  created_at: Date | string;
  resolved_by_sid: string | null;
  resolved_at: Date | string | null;
  resolution_note: string | null;
};

// A polymorphic subject with no FK: resolve its label against the table its kind
// names, and let an unresolved subject read "no longer resolves" rather than a
// broken row.
const APPEAL_COLUMNS = `a.id, a.sid, a.subject_kind, a.subject_id, a.body, a.status,
       a.created_at, a.resolved_by_sid, a.resolved_at, a.resolution_note,
       CASE a.subject_kind
         WHEN 'request' THEN (SELECT r.title FROM foundry.request r WHERE r.id = a.subject_id)
         WHEN 'session_series' THEN (SELECT ss.title FROM foundry.session_series ss WHERE ss.id = a.subject_id)
         WHEN 'role_grant' THEN (SELECT g.role FROM foundry.role_grant g WHERE g.id::text = a.subject_id)
       END AS subject_label`;

function subjectKindOf(value: string): AppealRow["subjectKind"] {
  if (value === "role_grant") return "role_grant";
  if (value === "session_series") return "session_series";
  return "request";
}

function statusOf(value: string): AppealRow["status"] {
  if (value === "withdrawn") return "withdrawn";
  if (value === "upheld") return "upheld";
  if (value === "declined") return "declined";
  return "open";
}

function toAppeal(r: AppealDbRow, withAppellant: boolean): AppealRow {
  return {
    id: r.id,
    subjectKind: subjectKindOf(r.subject_kind),
    subjectId: r.subject_id,
    subjectLabel: r.subject_label ?? "no longer resolves",
    body: r.body,
    status: statusOf(r.status),
    createdAt: iso(r.created_at),
    resolvedBy: r.resolved_by_sid ? { badge: sidBadge(r.resolved_by_sid) } : null,
    resolvedAt: r.resolved_at === null ? null : iso(r.resolved_at),
    resolutionNote: r.resolution_note,
    ...(withAppellant ? { appellant: { badge: sidBadge(r.sid) } } : {}),
  };
}

/** The appeals this session filed, with their outcomes. */
export async function listMyAppeals(sid: string): Promise<AppealRow[]> {
  const res = await getPool().query<AppealDbRow>(
    `SELECT ${APPEAL_COLUMNS} FROM foundry.appeal a
      WHERE a.sid = $1 ORDER BY a.created_at DESC`,
    [sid],
  );
  return res.rows.map((r) => toAppeal(r, false));
}

/** The admin queue: open appeals, appellant shown as a badge only. */
export async function listOpenAppeals(): Promise<AppealRow[]> {
  const res = await getPool().query<AppealDbRow>(
    `SELECT ${APPEAL_COLUMNS} FROM foundry.appeal a
      WHERE a.status = 'open' ORDER BY a.created_at`,
  );
  return res.rows.map((r) => toAppeal(r, true));
}

export async function resolveAppeal({
  appealId,
  sid,
  verdict,
  note,
  ip,
}: {
  appealId: string;
  sid: string;
  verdict: "upheld" | "declined";
  note: string;
  ip?: string | null;
}): Promise<void> {
  const text = note.trim();
  if (text.length === 0) {
    throw new FoundryStateError("A resolution needs a note explaining it.");
  }
  assertRate(sid, ip);
  await withTx(async (client) => {
    await requireRole(client, sid, "admin");
    const upd = await client.query(
      `UPDATE foundry.appeal
          SET status = $2, resolved_by_sid = $3, resolved_at = now(),
              resolution_note = $4
        WHERE id = $1 AND status = 'open'`,
      [appealId, verdict, sid, text],
    );
    if (upd.rowCount === 0) {
      throw new FoundryStateError("That appeal is not open.");
    }
    await logAction(client, {
      sid,
      action: "resolve_appeal",
      subject: appealId,
      detail: { verdict },
    });
  });
}

/** Request moderation has one owner — exchange.moderateRequest — so admins reach
 *  it through here without a duplicate mutation path. */
export async function setRequestStatus(input: {
  requestId: string;
  sid: string;
  verdict: "approved" | "closed";
  ip?: string | null;
}): Promise<void> {
  if (input.verdict !== "approved" && input.verdict !== "closed") {
    throw new FoundryStateError("Unknown verdict.");
  }
  await moderateRequest(input);
}
