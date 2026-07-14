import type { PoolClient } from "pg";

import {
  PRINCIPAL_SET_SQL,
  canonicalSidTx,
  getPool,
  logAction,
} from "./db.server";
import type { ConsentSnapshot, ConsentState, ConsentTopic } from "./types";

// The consent ledger is append-only: a change of mind is a new row, never an
// edit or a delete. Current state is therefore the LATEST row per (sid, topic),
// not "has this sid ever granted" — a withdrawal after a grant must read as
// withdrawn. This module holds no FK to roles or appeals so both can import it
// without a cycle (roles.server and stewardship.server both do).

type ConsentDbRow = { topic: string; state: string; at: Date | string };

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

function isTopic(value: string): value is ConsentTopic {
  return value === "steward-code" || value === "roster-listing";
}

/** Latest row per topic for one persona — read across its whole principal set,
 *  so consent given under a since-aliased sid still reads; a topic never
 *  touched is simply absent. */
export async function consentState(sid: string): Promise<ConsentSnapshot> {
  const res = await getPool().query<ConsentDbRow>(
    `SELECT DISTINCT ON (topic) topic, state, at
       FROM foundry.consent_event
      WHERE sid IN (${PRINCIPAL_SET_SQL})
      ORDER BY topic, at DESC`,
    [sid],
  );
  const topics: ConsentSnapshot["topics"] = {};
  for (const r of res.rows) {
    if (!isTopic(r.topic)) continue;
    topics[r.topic] = {
      state: r.state === "granted" ? "granted" : "withdrawn",
      at: iso(r.at),
    };
  }
  return { topics };
}

/**
 * Whether the latest row for (sid, topic) is a grant. Latest-row semantics, so a
 * grant that was later withdrawn reads false — never EXISTS(granted) over the
 * whole history, which would keep a withdrawn consent alive.
 */
export async function consentActive(
  client: PoolClient,
  sid: string,
  topic: ConsentTopic,
): Promise<boolean> {
  const res = await client.query<{ state: string }>(
    `SELECT state FROM foundry.consent_event
      WHERE sid IN (${PRINCIPAL_SET_SQL}) AND topic = $2
      ORDER BY at DESC
      LIMIT 1`,
    [sid, topic],
  );
  return res.rows[0]?.state === "granted";
}

export async function setConsent(
  client: PoolClient,
  {
    sid,
    topic,
    state,
  }: { sid: string; topic: ConsentTopic; state: ConsentState },
): Promise<void> {
  // The consent row lands on the canonical persona sid; the act itself is
  // logged under the live cookie sid, like every other act.
  const canonSid = await canonicalSidTx(client, sid);
  await client.query(
    `INSERT INTO foundry.consent_event (sid, topic, state) VALUES ($1, $2, $3)`,
    [canonSid, topic, state],
  );
  await logAction(client, {
    sid,
    action: state === "granted" ? "grant_consent" : "withdraw_consent",
    subject: topic,
    detail: { topic, state },
  });
}
