import { randomBytes } from "node:crypto";

import type { PoolClient } from "pg";

import { consentActive, setConsent } from "./consent.server";
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
import { clearGateMemo } from "./gate-memo.server";
import type { RoleName, RosterRow } from "./types";

// The one permission ledger. A role is held when foundry.role_grant carries an
// un-revoked row for (sid, role); revocation is a recorded second row-state, not
// a delete. `admin` and `host` are privileged and can never be self-granted
// (the DB CHECK enforces it too); `start`/`create` are self-chosen door records
// that gate nothing. requireRole/requireHost run INSIDE the mutation's own
// transaction, next to the write they guard, so a role lost mid-request cannot
// slip a write through.

function isRole(value: string): value is RoleName {
  return (
    value === "admin" ||
    value === "host" ||
    value === "create" ||
    value === "start"
  );
}

/** The roles a session actively holds — across its whole principal set, so a
 *  grant that landed under a now-aliased sid still counts. Newest grant first,
 *  each role once. */
export async function activeRoles(sid: string): Promise<RoleName[]> {
  const res = await getPool().query<{ role: string }>(
    `SELECT role FROM foundry.role_grant
      WHERE sid IN (${PRINCIPAL_SET_SQL}) AND revoked_at IS NULL
      GROUP BY role
      ORDER BY max(created_at) DESC`,
    [sid],
  );
  return res.rows.map((r) => r.role).filter(isRole);
}

export async function hasRole(
  client: PoolClient,
  sid: string,
  role: RoleName,
): Promise<boolean> {
  const res = await client.query<{ one: number }>(
    `SELECT 1 AS one FROM foundry.role_grant
      WHERE sid IN (${PRINCIPAL_SET_SQL}) AND role = $2 AND revoked_at IS NULL
      LIMIT 1`,
    [sid, role],
  );
  return res.rows.length > 0;
}

export async function requireRole(
  client: PoolClient,
  sid: string,
  role: RoleName,
): Promise<void> {
  if (!(await hasRole(client, sid, role))) {
    const shown = role === "admin" ? "operator" : role;
    throw new FoundryStateError(`This needs the ${shown} role.`);
  }
}

/**
 * A host may only act while the steward-code consent is currently granted:
 * withdrawing it pauses host powers immediately, without touching the grant. So
 * the gate is the AND of the role and the live consent, checked in-tx.
 */
export async function requireHost(
  client: PoolClient,
  sid: string,
): Promise<void> {
  const host = await hasRole(client, sid, "host");
  const consent = await consentActive(client, sid, "steward-code");
  if (!host || !consent) {
    throw new FoundryStateError(
      "This needs an active host role with the steward-code consent granted.",
    );
  }
}

/**
 * Redeems a one-time invite into a role_grant. The invite is locked FOR UPDATE
 * so two redemptions of the same code serialise; the second sees it already
 * redeemed and is refused. A host code additionally requires the steward-code
 * consent, granted in the SAME transaction so the role and its precondition land
 * together; any other invite records the consent too when it is offered. The
 * grant lands on the session's canonical persona sid, and a role already held
 * there keeps its standing grant — the invite is still burned.
 */
export async function redeemInvite({
  code,
  sid,
  consentSteward,
  ip,
}: {
  code: string;
  sid: string;
  consentSteward: boolean;
  ip?: string | null;
}): Promise<{ role: RoleName; personaName: string | null }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    const res = await client.query<{
      role: string;
      created_by_sid: string | null;
      expires_at: Date | null;
      redeemed_by_sid: string | null;
    }>(
      `SELECT role, created_by_sid, expires_at, redeemed_by_sid
         FROM foundry.role_invite WHERE code = $1 FOR UPDATE`,
      [code],
    );
    const invite = res.rows[0];
    if (!invite || !isRole(invite.role)) {
      throw new FoundryStateError("That invite code is not valid.");
    }
    if (invite.redeemed_by_sid !== null) {
      throw new FoundryStateError("That invite code was already redeemed.");
    }
    if (invite.expires_at !== null && invite.expires_at.getTime() < Date.now()) {
      throw new FoundryStateError("That invite code has expired.");
    }
    const role = invite.role;
    if (role === "host" && !consentSteward) {
      throw new FoundryStateError(
        "A host invite needs the steward-code consent to be accepted.",
      );
    }

    const canonSid = await canonicalSidTx(client, sid);
    if (!(await hasRole(client, canonSid, role))) {
      await client.query(
        `INSERT INTO foundry.role_grant (sid, role, granted_by_sid, invite_code)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (sid, role) WHERE revoked_at IS NULL DO NOTHING`,
        [canonSid, role, invite.created_by_sid, code],
      );
    }
    await client.query(
      `UPDATE foundry.role_invite
          SET redeemed_by_sid = $1, redeemed_at = now()
        WHERE code = $2`,
      [sid, code],
    );
    if (consentSteward) {
      await setConsent(client, {
        sid: canonSid,
        topic: "steward-code",
        state: "granted",
      });
    }
    await logAction(client, {
      sid,
      action: "redeem_invite",
      subject: code,
      detail: { role },
    });
    const persona = await client.query<{ display_name: string }>(
      `SELECT display_name FROM foundry.persona WHERE sid = $1`,
      [canonSid],
    );
    return { role, personaName: persona.rows[0]?.display_name ?? null };
  });
}

/**
 * The operator's fix for a role stranded on a lost sid: revoke the stranded
 * grant (recorded, never deleted), grant the same role — same invite lineage —
 * on the persona's own sid, and alias the lost sid onto the persona when it
 * owns no persona itself. One transaction; a second run finds nothing active
 * to move and changes nothing. role_grant.sid is never UPDATEd.
 */
export async function rebindGrant({
  fromSid,
  personaSid,
  role,
  note,
}: {
  fromSid: string;
  personaSid: string;
  role: RoleName;
  note: string;
}): Promise<void> {
  await withTx(async (client) => {
    const persona = await client.query(
      `SELECT 1 FROM foundry.persona WHERE sid = $1`,
      [personaSid],
    );
    if (persona.rowCount === 0) {
      throw new FoundryStateError("No persona holds that sid.");
    }
    // revoked_by_sid carries the persona the grant moved to: the revoke-pair
    // CHECK requires a non-null revoker, and the beneficiary is the honest one.
    const revoked = await client.query<{ invite_code: string | null }>(
      `UPDATE foundry.role_grant
          SET revoked_at = now(), revoked_by_sid = $2, revoke_note = $3
        WHERE sid = $1 AND role = $4 AND revoked_at IS NULL
        RETURNING invite_code`,
      [fromSid, personaSid, note.trim(), role],
    );
    const granted = await client.query(
      `INSERT INTO foundry.role_grant (sid, role, granted_by_sid, invite_code, note)
       VALUES ($1, $2, NULL, $3, $4)
       ON CONFLICT (sid, role) WHERE revoked_at IS NULL DO NOTHING`,
      [personaSid, role, revoked.rows[0]?.invite_code ?? null, note.trim()],
    );
    if (fromSid !== personaSid) {
      const owns = await client.query(
        `SELECT 1 FROM foundry.persona WHERE sid = $1`,
        [fromSid],
      );
      if (owns.rowCount === 0) {
        await client.query(
          `INSERT INTO foundry.sid_alias (alias_sid, persona_sid, via)
           VALUES ($1, $2, 'operator-rebind')
           ON CONFLICT (alias_sid) DO NOTHING`,
          [fromSid, personaSid],
        );
      }
    }
    if ((revoked.rowCount ?? 0) > 0 || (granted.rowCount ?? 0) > 0) {
      await logAction(client, {
        sid: personaSid,
        action: "rebind_grant",
        subject: role,
        detail: { role },
      });
    }
  });
  clearGateMemo(fromSid, personaSid);
}

/** A host mints a one-time invite. The code is minted here (never in SQL) and
 *  is the only time it is returned; it is stored verbatim as the PK. */
export async function mintInvite({
  sid,
  role,
  note,
  expiresAt,
  ip,
}: {
  sid: string;
  role: RoleName;
  note: string;
  expiresAt: string | null;
  ip?: string | null;
}): Promise<{ code: string }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    await requireHost(client, sid);
    const code = randomBytes(9).toString("hex");
    await client.query(
      `INSERT INTO foundry.role_invite
         (code, role, note, created_via, created_by_sid, expires_at)
       VALUES ($1, $2, $3, 'host', $4, $5::timestamptz)`,
      [code, role, note.trim(), sid, expiresAt],
    );
    // The ledger records who minted what for whom — never the code itself:
    // action_log feeds the public timeline, and a logged code is a redeemable
    // secret in plain sight. The code travels only in this return value.
    await logAction(client, {
      sid,
      action: "mint_invite",
      subject: role,
      detail: { role, note: note.trim() },
    });
    return { code };
  });
}

/** Revokes an active grant. Host or admin only; the revocation is stamped, never
 *  deleted, so the ledger keeps the whole history. */
export async function revokeRole({
  sid,
  grantId,
  note,
  ip,
}: {
  sid: string;
  grantId: number;
  note: string;
  ip?: string | null;
}): Promise<void> {
  assertRate(sid, ip);
  await withTx(async (client) => {
    const admin = await hasRole(client, sid, "admin");
    const host = await hasRole(client, sid, "host");
    if (!admin && !host) {
      throw new FoundryStateError("This needs the host or operator role.");
    }
    const upd = await client.query<{ sid: string; role: string }>(
      `UPDATE foundry.role_grant
          SET revoked_at = now(), revoked_by_sid = $1, revoke_note = $2
        WHERE id = $3 AND revoked_at IS NULL
        RETURNING sid, role`,
      [sid, note.trim(), grantId],
    );
    if (upd.rowCount === 0) {
      throw new FoundryStateError("That grant is not active.");
    }
    await logAction(client, {
      sid,
      action: "revoke_role",
      subject: String(grantId),
      detail: { role: upd.rows[0].role },
    });
  });
}

type RosterDbRow = {
  sid: string;
  role: string;
  created_at: Date | string;
  display_name: string | null;
  consent_state: string;
};

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

/**
 * The privileged roster: admin/host holders who have consented to be listed.
 * Holders whose latest roster-listing consent is absent or withdrawn are NOT
 * named — they surface only as an aggregate count, so a badge on the public page
 * always reflects a live consent.
 */
export async function listRoster(): Promise<{
  rows: RosterRow[];
  notListed: number;
}> {
  // Grants and consents both resolve through the alias layer to the canonical
  // persona sid, so one holder is one roster line no matter which of their
  // sids a row landed on; `since` keeps the earliest grant.
  const res = await getPool().query<RosterDbRow>(
    `WITH grant_canon AS (
       SELECT COALESCE(al.persona_sid, g.sid) AS canon_sid, g.role, g.created_at
         FROM foundry.role_grant g
         LEFT JOIN foundry.sid_alias al ON al.alias_sid = g.sid
        WHERE g.revoked_at IS NULL AND g.role IN ('admin','host')
     ),
     latest_consent AS (
       SELECT DISTINCT ON (canon) canon, state FROM (
         SELECT COALESCE(al.persona_sid, ce.sid) AS canon, ce.state, ce.at
           FROM foundry.consent_event ce
           LEFT JOIN foundry.sid_alias al ON al.alias_sid = ce.sid
          WHERE ce.topic = 'roster-listing'
       ) widened ORDER BY canon, at DESC
     )
     SELECT sid, role, created_at, display_name, consent_state FROM (
       SELECT DISTINCT ON (gc.canon_sid, gc.role)
              gc.canon_sid AS sid, gc.role, gc.created_at, p.display_name,
              coalesce(lc.state, 'withdrawn') AS consent_state
         FROM grant_canon gc
         LEFT JOIN latest_consent lc ON lc.canon = gc.canon_sid
         LEFT JOIN foundry.persona p ON p.sid = gc.canon_sid
        ORDER BY gc.canon_sid, gc.role, gc.created_at
     ) roster
     ORDER BY role, created_at`,
  );
  const rows: RosterRow[] = [];
  let notListed = 0;
  for (const r of res.rows) {
    if (!isRole(r.role)) continue;
    if (r.consent_state !== "granted") {
      notListed += 1;
      continue;
    }
    rows.push({
      role: r.role,
      actor: r.display_name
        ? { name: r.display_name }
        : { badge: sidBadge(r.sid) },
      since: iso(r.created_at),
    });
  }
  return { rows, notListed };
}
