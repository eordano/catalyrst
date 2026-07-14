import { createHash, randomBytes } from "node:crypto";

import type { PoolClient } from "pg";

import {
  FoundryStateError,
  assertRate,
  getPool,
  logAction,
  sidBadge,
  withTx,
} from "./db.server";
import { personaNames } from "./memory.server";
import type {
  PublicSceneSteward,
  SceneStewardRow,
  SceneTransferRow,
  TransferView,
} from "./types";

// Scene continuity: who tends a scene, the notes they leave, and the recorded
// handoffs of that seat. NOTHING here is verified — the worlds mirror carries no
// deployer, so a stewardship claim is exactly that, a claim, and the UI says so.
// Actors render as a claimed persona name or the session badge, same as the
// timeline: a persona name is a self-claimed label, not verified ownership, and
// the raw sid still never leaves this module. A transfer stores only
// sha256(code); the raw code is shown once and is then unrecoverable, and
// 'expired' is a derived reading of an offered row past its expiry, never a
// rewrite of the stored status.

export const SCENE_ACTIONS = [
  "claim_steward",
  "release_steward",
  "offer_transfer",
  "revoke_transfer",
  "accept_transfer",
  "scene_note",
] as const;

// scene_note excluded: listSceneMemory's changelog arm already carries the note verbatim.
const MEMORY_ACTIONS = SCENE_ACTIONS.filter((a) => a !== "scene_note");

const TRANSFER_TTL_MS = 7 * 24 * 60 * 60 * 1000;

function hashToken(code: string): string {
  return createHash("sha256").update(code).digest("hex");
}

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

export async function isActiveSteward(
  client: PoolClient,
  sceneId: string,
  sid: string,
): Promise<boolean> {
  const res = await client.query<{ one: number }>(
    `SELECT 1 AS one FROM foundry.scene_steward
      WHERE scene_id = $1 AND sid = $2 AND released_at IS NULL LIMIT 1`,
    [sceneId, sid],
  );
  return res.rows.length > 0;
}

export interface SceneMemoryRow {
  /** Public row address: 'c<id>' (changelog) or 'a<id>' (action_log) — the
   *  /foundry/timeline/<eventId> page renders the full record behind it. */
  eventId: string;
  at: string;
  actor: { name: string } | { badge: string } | { source: string };
  action: string;
  body: string;
  sourceNote: string;
}

type MemoryDbRow = {
  event_id: string;
  at: Date | string;
  sid: string | null;
  action: string;
  body: string;
  source_note: string;
  origin: string;
};

/** The per-scene memory: deployment/edit changelog merged with the recorded
 *  stewardship actions on this scene. Imported rows carry no actor. */
export async function listSceneMemory(sceneId: string): Promise<SceneMemoryRow[]> {
  const res = await getPool().query<MemoryDbRow>(
    `SELECT event_id, at, sid, action, body, source_note, origin FROM (
       SELECT 'c' || c.id AS event_id, c.at AS at, c.sid AS sid,
              'changelog'::text AS action,
              c.note AS body, c.source_note AS source_note, c.origin AS origin
         FROM foundry.scene_changelog c
        WHERE c.scene_id = $1
       UNION ALL
       SELECT 'a' || a.id AS event_id, a.at, a.sid, a.action,
              coalesce(a.detail ->> 'note', '') AS body, ''::text AS source_note,
              'visitor'::text AS origin
         FROM foundry.action_log a
        WHERE a.subject = $1 AND a.action = ANY($2::text[])
     ) m
     ORDER BY at DESC`,
    [sceneId, MEMORY_ACTIONS],
  );
  const names = await personaNames(res.rows.map((r) => r.sid));
  return res.rows.map((r) => {
    // The chip carries the source alone ("worlds mirror"); the entity hash
    // stays once, in the row's Source-note column.
    const label = (r.source_note || "").replace(/\s+entity\s+\S+$/, "").trim();
    return {
      eventId: r.event_id,
      at: iso(r.at),
      actor: r.sid ? actorFor(r.sid, names) : { source: label || "import" },
      action: r.action,
      body: r.body,
      sourceNote: r.source_note,
    };
  });
}

export interface MemoryEventRecord {
  eventId: string;
  kind: "changelog" | "action";
  at: string;
  /** Null when the source row records no author (an imported changelog row). */
  actor: { name: string } | { badge: string } | null;
  action: string;
  body: string;
  sourceNote: string;
  origin: "import" | "visitor";
  scene: { id: string; title: string | null; entityId: string | null };
}

/** The full record behind one scene-memory row, addressed by its public
 *  eventId. Only rows the memory table itself renders resolve here: a
 *  changelog row, or an action_log row whose action is a scene action. */
export async function getMemoryEvent(
  eventId: string,
): Promise<MemoryEventRecord | null> {
  const match = /^([ca])(\d+)$/.exec(eventId);
  if (!match) return null;
  const rowId = Number(match[2]);
  if (!Number.isSafeInteger(rowId)) return null;

  if (match[1] === "c") {
    const res = await getPool().query<{
      at: Date | string;
      sid: string | null;
      note: string;
      source_note: string;
      origin: string;
      scene_id: string;
      title: string | null;
      entity_id: string | null;
    }>(
      `SELECT c.at, c.sid, c.note, c.source_note, c.origin,
              c.scene_id, s.title, s.entity_id
         FROM foundry.scene_changelog c
         JOIN foundry.scene s ON s.id = c.scene_id
        WHERE c.id = $1`,
      [rowId],
    );
    const r = res.rows[0];
    if (!r) return null;
    const names = await personaNames([r.sid]);
    return {
      eventId,
      kind: "changelog",
      at: iso(r.at),
      actor: r.sid ? actorFor(r.sid, names) : null,
      action: "changelog",
      body: r.note,
      sourceNote: r.source_note,
      origin: r.origin === "visitor" ? "visitor" : "import",
      scene: { id: r.scene_id, title: r.title, entityId: r.entity_id },
    };
  }

  const res = await getPool().query<{
    at: Date | string;
    sid: string;
    action: string;
    subject: string;
    note: string;
    title: string | null;
    entity_id: string | null;
  }>(
    `SELECT a.at, a.sid, a.action, a.subject,
            coalesce(a.detail ->> 'note', '') AS note,
            s.title, s.entity_id
       FROM foundry.action_log a
       JOIN foundry.scene s ON s.id = a.subject
      WHERE a.id = $1 AND a.action = ANY($2::text[])`,
    [rowId, MEMORY_ACTIONS],
  );
  const r = res.rows[0];
  if (!r) return null;
  const names = await personaNames([r.sid]);
  return {
    eventId,
    kind: "action",
    at: iso(r.at),
    actor: actorFor(r.sid, names),
    action: r.action,
    body: r.note,
    sourceNote: "",
    origin: "visitor",
    scene: { id: r.subject, title: r.title, entityId: r.entity_id },
  };
}

function actorFor(
  sid: string,
  names: Map<string, string>,
): { name: string } | { badge: string } {
  const name = names.get(sid);
  return name ? { name } : { badge: sidBadge(sid) };
}

/** scene_id → the earliest ACTIVE steward's display label (persona name, or
 *  the honest visitor badge when the claimant never authored one). One query
 *  for the whole shelf; scenes with no active steward are simply absent. */
export async function activeStewardsByScene(): Promise<Map<string, string>> {
  const res = await getPool().query<{ scene_id: string; sid: string }>(
    `SELECT DISTINCT ON (scene_id) scene_id, sid
       FROM foundry.scene_steward
      WHERE released_at IS NULL
      ORDER BY scene_id, since`,
  );
  const names = await personaNames(res.rows.map((r) => r.sid));
  const out = new Map<string, string>();
  for (const r of res.rows) {
    out.set(r.scene_id, names.get(r.sid) ?? `visitor ${sidBadge(r.sid)}`);
  }
  return out;
}

export const CONTINUITY_LIMITS = { note: 280, basis: 280 } as const;

export async function addSceneNote({
  sceneId,
  sid,
  note,
  ip,
}: {
  sceneId: string;
  sid: string;
  note: string;
  ip?: string | null;
}): Promise<void> {
  const text = note.trim();
  if (text.length === 0) throw new FoundryStateError("Write the note first.");
  if (text.length > CONTINUITY_LIMITS.note) {
    throw new FoundryStateError(`Notes are ${CONTINUITY_LIMITS.note} characters or fewer.`);
  }
  assertRate(sid, ip);
  await withTx(async (client) => {
    if (!(await isActiveSteward(client, sceneId, sid))) {
      throw new FoundryStateError("Only an active steward can leave a note here.");
    }
    await client.query(
      `INSERT INTO foundry.scene_changelog (scene_id, at, note, source_note, origin, sid)
       VALUES ($1, now(), $2, '', 'visitor', $3)`,
      [sceneId, text, sid],
    );
    await logAction(client, {
      sid,
      action: "scene_note",
      subject: sceneId,
      detail: { note: text },
    });
  });
}

type StewardDbRow = {
  sid: string;
  basis: string;
  since: Date | string;
  released_at: Date | string | null;
  release_reason: string | null;
  via_transfer_id: string | null;
};

function toSteward(r: StewardDbRow, names: Map<string, string>): SceneStewardRow {
  return {
    sid: r.sid,
    actor: actorFor(r.sid, names),
    basis: r.basis,
    since: iso(r.since),
    releasedAt: r.released_at === null ? null : iso(r.released_at),
    releaseReason:
      r.release_reason === "self" || r.release_reason === "transfer"
        ? r.release_reason
        : null,
    viaTransfer: r.via_transfer_id !== null,
  };
}

/** Steward rows with the raw sid stripped before they leave this function, so
 *  no caller — loader, action payload or export bundle — can leak one. The
 *  viewer's own standing comes back as a boolean instead. */
export async function listStewards(
  sceneId: string,
  viewerSid?: string,
): Promise<{
  active: PublicSceneSteward[];
  past: PublicSceneSteward[];
  isViewerSteward: boolean;
}> {
  const res = await getPool().query<StewardDbRow>(
    `SELECT sid, basis, since, released_at, release_reason, via_transfer_id
       FROM foundry.scene_steward
      WHERE scene_id = $1
      ORDER BY (released_at IS NULL) DESC, since`,
    [sceneId],
  );
  const names = await personaNames(res.rows.map((r) => r.sid));
  const active: PublicSceneSteward[] = [];
  const past: PublicSceneSteward[] = [];
  let isViewerSteward = false;
  for (const r of res.rows) {
    if (r.released_at === null && viewerSid !== undefined && r.sid === viewerSid) {
      isViewerSteward = true;
    }
    const { sid: _sid, ...pub } = toSteward(r, names);
    (r.released_at === null ? active : past).push(pub);
  }
  return { active, past, isViewerSteward };
}

export async function claimSteward({
  sceneId,
  sid,
  basis,
  ip,
}: {
  sceneId: string;
  sid: string;
  basis: string;
  ip?: string | null;
}): Promise<void> {
  const basisText = basis.trim();
  if (basisText.length > CONTINUITY_LIMITS.basis) {
    throw new FoundryStateError(`The basis is ${CONTINUITY_LIMITS.basis} characters or fewer.`);
  }
  assertRate(sid, ip);
  await withTx(async (client) => {
    const ins = await client.query(
      `INSERT INTO foundry.scene_steward (scene_id, sid, basis)
       VALUES ($1, $2, $3)
       ON CONFLICT (scene_id, sid) WHERE released_at IS NULL DO NOTHING`,
      [sceneId, sid, basisText],
    );
    if ((ins.rowCount ?? 0) === 0) {
      throw new FoundryStateError("You already steward this scene.");
    }
    await logAction(client, {
      sid,
      action: "claim_steward",
      subject: sceneId,
      detail: { basis: basis.trim() },
    });
  });
}

export async function releaseSteward({
  sceneId,
  sid,
  ip,
}: {
  sceneId: string;
  sid: string;
  ip?: string | null;
}): Promise<void> {
  assertRate(sid, ip);
  await withTx(async (client) => {
    const upd = await client.query(
      `UPDATE foundry.scene_steward
          SET released_at = now(), release_reason = 'self'
        WHERE scene_id = $1 AND sid = $2 AND released_at IS NULL`,
      [sceneId, sid],
    );
    if (upd.rowCount === 0) {
      throw new FoundryStateError("You do not currently steward this scene.");
    }
    await logAction(client, {
      sid,
      action: "release_steward",
      subject: sceneId,
      detail: {},
    });
  });
}

function effectiveStatus(
  status: string,
  expiresAtMs: number,
): SceneTransferRow["effectiveStatus"] {
  if (status === "offered" && expiresAtMs < Date.now()) return "expired";
  if (status === "accepted") return "accepted";
  if (status === "revoked") return "revoked";
  return "offered";
}

export async function offerTransfer({
  sceneId,
  sid,
  note,
  ip,
}: {
  sceneId: string;
  sid: string;
  note: string;
  ip?: string | null;
}): Promise<{ code: string }> {
  const noteText = note.trim();
  if (noteText.length > CONTINUITY_LIMITS.note) {
    throw new FoundryStateError(`Notes are ${CONTINUITY_LIMITS.note} characters or fewer.`);
  }
  assertRate(sid, ip);
  return withTx(async (client) => {
    if (!(await isActiveSteward(client, sceneId, sid))) {
      throw new FoundryStateError("Only an active steward can offer a transfer.");
    }
    const code = randomBytes(18).toString("hex");
    const res = await client.query<{ id: string }>(
      `INSERT INTO foundry.scene_transfer
         (scene_id, from_sid, token_hash, note, expires_at)
       VALUES ($1, $2, $3, $4, now() + ($5 || ' milliseconds')::interval)
       RETURNING id`,
      [sceneId, sid, hashToken(code), noteText, String(TRANSFER_TTL_MS)],
    );
    await logAction(client, {
      sid,
      action: "offer_transfer",
      subject: sceneId,
      detail: { transfer_id: res.rows[0].id },
    });
    return { code };
  });
}

export async function revokeTransfer({
  transferId,
  sid,
  ip,
}: {
  transferId: string;
  sid: string;
  ip?: string | null;
}): Promise<void> {
  assertRate(sid, ip);
  await withTx(async (client) => {
    const upd = await client.query<{ scene_id: string }>(
      `UPDATE foundry.scene_transfer SET status = 'revoked'
        WHERE id = $1 AND from_sid = $2 AND status = 'offered'
        RETURNING scene_id`,
      [transferId, sid],
    );
    if (upd.rowCount === 0) {
      throw new FoundryStateError("That transfer offer is not open, or is not yours.");
    }
    await logAction(client, {
      sid,
      action: "revoke_transfer",
      subject: upd.rows[0].scene_id,
      detail: { transfer_id: transferId },
    });
  });
}

/** Resolves a raw token to its offer plus the scene title, with expiry read as a
 *  derived effective status. Null when the token matches nothing. */
export async function getTransferForToken(
  code: string,
): Promise<TransferView | null> {
  const res = await getPool().query<{
    scene_id: string;
    scene_title: string;
    from_sid: string;
    note: string;
    status: string;
    expires_at: Date | string;
  }>(
    `SELECT tr.scene_id, s.title AS scene_title, tr.from_sid, tr.note, tr.status,
            tr.expires_at
       FROM foundry.scene_transfer tr
       JOIN foundry.scene s ON s.id = tr.scene_id
      WHERE tr.token_hash = $1`,
    [hashToken(code)],
  );
  const r = res.rows[0];
  if (!r) return null;
  const expiresMs = r.expires_at instanceof Date ? r.expires_at.getTime() : new Date(r.expires_at).getTime();
  return {
    sceneId: r.scene_id,
    sceneTitle: r.scene_title,
    from: { badge: sidBadge(r.from_sid) },
    note: r.note,
    expiresAt: iso(r.expires_at),
    effectiveStatus: effectiveStatus(r.status, expiresMs),
  };
}

/**
 * Accepts a transfer in one locked transaction: the offer flips to accepted, the
 * offerer's active steward row closes with reason 'transfer', and a successor
 * steward row opens carrying via_transfer_id. Anything but an unexpired offered
 * row is refused with its own reason.
 */
export async function acceptTransfer({
  code,
  sid,
  ip,
}: {
  code: string;
  sid: string;
  ip?: string | null;
}): Promise<{ sceneId: string }> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    const res = await client.query<{
      id: string;
      scene_id: string;
      from_sid: string;
      status: string;
      expires_at: Date | string;
    }>(
      `SELECT id, scene_id, from_sid, status, expires_at
         FROM foundry.scene_transfer WHERE token_hash = $1 FOR UPDATE`,
      [hashToken(code)],
    );
    const tr = res.rows[0];
    if (!tr) throw new FoundryStateError("That transfer link is not valid.");
    if (tr.status === "accepted") {
      throw new FoundryStateError("That transfer was already accepted.");
    }
    if (tr.status === "revoked") {
      throw new FoundryStateError("That transfer offer was revoked.");
    }
    const expiresMs = tr.expires_at instanceof Date ? tr.expires_at.getTime() : new Date(tr.expires_at).getTime();
    if (expiresMs < Date.now()) {
      throw new FoundryStateError("That transfer offer has expired.");
    }

    await client.query(
      `UPDATE foundry.scene_transfer
          SET status = 'accepted', accepted_sid = $2, accepted_at = now()
        WHERE id = $1`,
      [tr.id, sid],
    );
    await client.query(
      `UPDATE foundry.scene_steward
          SET released_at = now(), release_reason = 'transfer'
        WHERE scene_id = $1 AND sid = $2 AND released_at IS NULL`,
      [tr.scene_id, tr.from_sid],
    );
    await client.query(
      `INSERT INTO foundry.scene_steward (scene_id, sid, basis, via_transfer_id)
       VALUES ($1, $2, $3, $4)
       ON CONFLICT (scene_id, sid) WHERE released_at IS NULL DO NOTHING`,
      [tr.scene_id, sid, "accepted a stewardship transfer", tr.id],
    );
    await logAction(client, {
      sid,
      action: "accept_transfer",
      subject: tr.scene_id,
      detail: { transfer_id: tr.id, from: sidBadge(tr.from_sid) },
    });
    return { sceneId: tr.scene_id };
  });
}

export async function listTransfers(
  sceneId: string,
): Promise<Omit<SceneTransferRow, "sceneId">[]> {
  const res = await getPool().query<{
    id: string;
    from_sid: string;
    note: string;
    status: string;
    created_at: Date | string;
    expires_at: Date | string;
    accepted_at: Date | string | null;
    accepted_sid: string | null;
  }>(
    `SELECT id, from_sid, note, status, created_at, expires_at, accepted_at,
            accepted_sid
       FROM foundry.scene_transfer
      WHERE scene_id = $1
      ORDER BY created_at DESC`,
    [sceneId],
  );
  const names = await personaNames(
    res.rows.flatMap((r) => [r.from_sid, r.accepted_sid]),
  );
  return res.rows.map((r) => {
    const expiresMs = r.expires_at instanceof Date ? r.expires_at.getTime() : new Date(r.expires_at).getTime();
    return {
      id: r.id,
      from: actorFor(r.from_sid, names),
      note: r.note,
      status: r.status === "accepted" || r.status === "revoked" ? r.status : "offered",
      effectiveStatus: effectiveStatus(r.status, expiresMs),
      createdAt: iso(r.created_at),
      expiresAt: iso(r.expires_at),
      acceptedAt: r.accepted_at === null ? null : iso(r.accepted_at),
      acceptedBy: r.accepted_sid ? actorFor(r.accepted_sid, names) : null,
    };
  });
}
