import type { Pool, PoolClient } from "pg";

import {
  FoundryStateError,
  PRINCIPAL_SET_SQL,
  assertRate,
  canonicalSidTx,
  logAction,
  withTx,
} from "./db.server";

// An approval is a person's signature on a specific doc version — the deck's
// "creator approves, no auto-publish" boundary made recordable. Versions are
// immutable (an edit mints v(n+1)), so a signature can never be hollowed out
// by later edits. The signer must hold a claimed persona: a signature needs a
// name, and the badge of an anonymous session is not one. Append-only, one
// row per (doc, signer); nothing here ever deletes or updates a signature.

const APPROVE_NO_DOC = "No such design document.";
export const APPROVE_NO_PERSONA =
  "An approval is signed with a name — claim a persona first.";
const APPROVE_TWICE = "You have already approved this version.";

export interface GddApprovalRecord {
  /** The signer's persona name at signing-read time — resolved on read, so a
   *  rename shows the current name, never a stale copy. */
  name: string;
  at: string;
}

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

export async function approveGddDoc({
  docId,
  sid,
  ip,
}: {
  docId: string;
  sid: string;
  ip?: string | null;
}): Promise<GddApprovalRecord> {
  assertRate(sid, ip);
  return withTx(async (client: PoolClient) => {
    const doc = await client.query(
      `SELECT 1 FROM foundry.gdd_doc WHERE id = $1`,
      [docId],
    );
    if (doc.rowCount === 0) throw new FoundryStateError(APPROVE_NO_DOC);
    // The live cookie sid may be an ALIAS of the persona's canonical sid
    // (operator-rebind leaves the browser on the alias). Resolve it before
    // the persona lookup and the insert, so a rebound session still signs
    // with its name and the signature lands canonically.
    const canon = await canonicalSidTx(client, sid);
    const persona = await client.query<{ display_name: string }>(
      `SELECT display_name FROM foundry.persona WHERE sid = $1`,
      [canon],
    );
    const name = persona.rows[0]?.display_name;
    if (!name) throw new FoundryStateError(APPROVE_NO_PERSONA);
    // One signature per PERSON, not per sid: rows written before the alias
    // fix may sit on an alias sid, where the (doc, sid) unique index cannot
    // see them from the canonical sid — so the whole principal set is checked.
    const dup = await client.query(
      `SELECT 1 FROM foundry.gdd_approval
        WHERE sid IN (${PRINCIPAL_SET_SQL}) AND doc_id = $2`,
      [sid, docId],
    );
    if ((dup.rowCount ?? 0) > 0) throw new FoundryStateError(APPROVE_TWICE);
    const ins = await client.query<{ at: Date | string }>(
      `INSERT INTO foundry.gdd_approval (doc_id, sid) VALUES ($1, $2)
       ON CONFLICT (doc_id, sid) DO NOTHING
       RETURNING at`,
      [docId, canon],
    );
    const row = ins.rows[0];
    if (!row) throw new FoundryStateError(APPROVE_TWICE);
    await logAction(client, {
      sid,
      action: "approve_gdd",
      subject: docId,
      detail: {},
    });
    return { name, at: iso(row.at) };
  });
}

/** Every signature on one version, signing order, names resolved on read —
 *  a raw sid never leaves this function. */
export async function approvalsForDoc(
  db: Pool,
  docId: string,
): Promise<GddApprovalRecord[]> {
  const res = await db.query<{ display_name: string; at: Date | string }>(
    `SELECT p.display_name, a.at
       FROM foundry.gdd_approval a
       LEFT JOIN foundry.sid_alias al ON al.alias_sid = a.sid
       JOIN foundry.persona p ON p.sid = COALESCE(al.persona_sid, a.sid)
      WHERE a.doc_id = $1
      ORDER BY a.at, a.id`,
    [docId],
  );
  return res.rows.map((r) => ({ name: r.display_name, at: iso(r.at) }));
}

/** Signature counts for a set of versions in one round trip (the version
 *  rail, the list cards). A doc with no row simply maps to nothing — the
 *  caller renders zero as the honest absence it is. */
export async function approvalCounts(
  db: Pool,
  docIds: readonly string[],
): Promise<Map<string, number>> {
  const map = new Map<string, number>();
  if (docIds.length === 0) return map;
  const res = await db.query<{ doc_id: string; n: number }>(
    `SELECT doc_id, count(*)::int AS n FROM foundry.gdd_approval
      WHERE doc_id = ANY($1) GROUP BY doc_id`,
    [[...docIds]],
  );
  for (const r of res.rows) map.set(r.doc_id, Number(r.n));
  return map;
}

/** Whether this session's PERSON already signed this version — resolved
 *  across the persona's whole sid set, so a signature survives a re-binding.
 *  Drives the affordance, never a display value. */
export async function hasApproved(
  db: Pool,
  docId: string,
  sid: string,
): Promise<boolean> {
  const res = await db.query(
    `SELECT 1 FROM foundry.gdd_approval
      WHERE sid IN (${PRINCIPAL_SET_SQL}) AND doc_id = $2`,
    [sid, docId],
  );
  return (res.rowCount ?? 0) > 0;
}
