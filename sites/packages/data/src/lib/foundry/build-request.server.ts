import type { Pool } from "pg";

import {
  FoundryStateError,
  PRINCIPAL_SET_SQL,
  assertRate,
  canonicalSidTx,
  logAction,
  sidBadge,
  withTx,
} from "./db.server";
import { consentActive } from "./consent.server";
import { hasRole } from "./roles.server";
import { SCENE_REGISTER_LIMITS, SLUG_RE } from "./scene-register.server";

// A build request is the doc page's "Build this" made recordable: a queued ask
// that the Forge broker (deploy/forge/DESIGN.md) drains, plus the broker's own
// status ledger for it. The row never touches foundry.scene -- on 'landed' the
// broker registers the scene through registerScene, so the shelf row and the
// doc linkage arrive through the existing host-gated writer.

export type BuildStatus = "queued" | "building" | "verifying" | "landed" | "failed";

export interface BuildRequestRow {
  id: number;
  docId: string;
  sceneSlug: string;
  /** Persona name resolved on read via the sid_alias join; a session that
   *  never claimed one shows its visitor badge -- a raw sid never leaves. */
  requestedBy: { name: string } | { badge: string };
  status: BuildStatus;
  detail: string;
  createdAt: string;
  updatedAt: string;
}

// action_log.sid is NOT NULL and the broker holds no browser session: its
// status writes land on this fixed sid, a stable attribution the operator can
// claim a persona for.
const BROKER_SID = "forge-broker";

const NO_DOC = "No such design document.";
const NOT_MAKER =
  "A build is requested by this doc's maker or a host \u{2014} edit the doc (any save makes you its maker) or redeem a host invite.";
const ALREADY_UNDERWAY =
  "A build of this design is already underway \u{2014} watch its status above.";

// The legal edges of the status machine. Everything else -- including standing
// still -- is refused before any row is touched.
const LEGAL_EDGES: Record<BuildStatus, readonly BuildStatus[]> = {
  queued: ["building", "failed"],
  building: ["verifying", "failed"],
  verifying: ["landed", "failed"],
  landed: [],
  failed: [],
};

// Every version in $1's supersedes chain, walked in both directions -- the same
// walk getGddChain does, trimmed to ids. Requests and refusals are chain-wide:
// an edit mints a new doc id, and a build asked on v2 must block a second ask
// landing on v3.
const CHAIN_SQL = `
  WITH RECURSIVE older AS (
    SELECT d.id, d.supersedes FROM foundry.gdd_doc d WHERE d.id = $1
    UNION
    SELECT d.id, d.supersedes
      FROM foundry.gdd_doc d JOIN older o ON o.supersedes = d.id
  ), newer AS (
    SELECT d.id, d.supersedes FROM foundry.gdd_doc d WHERE d.id = $1
    UNION
    SELECT d.id, d.supersedes
      FROM foundry.gdd_doc d JOIN newer n ON d.supersedes = n.id
  )
  SELECT id FROM older UNION SELECT id FROM newer`;

// The maker is whoever is behind the chain's latest edit/publish act; their
// person may answer from any sid of their principal set. $1 = live sid,
// $2 = chain ids.
const MAKER_SQL = `
  SELECT 1 FROM (
    SELECT a.sid FROM foundry.action_log a
     WHERE a.action IN ('edit_gdd_doc','publish_gdd_draft')
       AND a.subject = ANY($2)
     ORDER BY a.at DESC, a.id DESC
     LIMIT 1
  ) maker WHERE maker.sid IN (${PRINCIPAL_SET_SQL})`;

const ROW_COLUMNS = `
  b.id, b.doc_id, b.scene_slug, b.requested_by_sid, b.status, b.detail,
  b.created_at, b.updated_at, p.display_name`;

const ROW_JOINS = `
  LEFT JOIN foundry.sid_alias al ON al.alias_sid = b.requested_by_sid
  LEFT JOIN foundry.persona p ON p.sid = COALESCE(al.persona_sid, b.requested_by_sid)`;

interface DbRow {
  id: string | number;
  doc_id: string;
  scene_slug: string;
  requested_by_sid: string;
  status: string;
  detail: string;
  created_at: Date | string;
  updated_at: Date | string;
  display_name: string | null;
}

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

function toRow(r: DbRow): BuildRequestRow {
  return {
    id: Number(r.id),
    docId: r.doc_id,
    sceneSlug: r.scene_slug,
    requestedBy: r.display_name
      ? { name: r.display_name }
      : { badge: sidBadge(r.requested_by_sid) },
    status: r.status as BuildStatus,
    detail: r.detail,
    createdAt: iso(r.created_at),
    updatedAt: iso(r.updated_at),
  };
}

function validateSlug(sceneSlug: string): string {
  const slug = sceneSlug.trim();
  if (slug.length === 0) {
    throw new FoundryStateError(
      "Give the build a scene id \u{2014} it becomes /foundry/play/<id> when it lands.",
    );
  }
  if (slug.length > SCENE_REGISTER_LIMITS.id) {
    throw new FoundryStateError(
      `Scene ids are ${SCENE_REGISTER_LIMITS.id} characters or fewer.`,
    );
  }
  if (!SLUG_RE.test(slug)) {
    throw new FoundryStateError(
      "Scene ids are lowercase letters, digits and dashes, starting with a letter or digit.",
    );
  }
  // registerScene would refuse this slug when the build lands -- refuse it now,
  // not after a broker run.
  if (slug === "register") {
    throw new FoundryStateError(
      'The id "register" is taken by the register form \u{2014} pick another.',
    );
  }
  return slug;
}

/** Doc-page writer. assertRate; withTx: doc must exist; sid must be maker-or-host (in-tx re-check);
 *  slug validated (SLUG_RE, <=40) and free in foundry.scene AND live build_requests; one active request
 *  per doc chain enforced by build_request_active_once (23505 -> FoundryStateError). Inserts 'queued'
 *  on the canonical sid; logAction {action:"request_build", subject: docId, detail:{scene_slug}}. */
export async function requestBuild({
  docId,
  sceneSlug,
  sid,
  ip,
}: {
  docId: string;
  sceneSlug: string;
  sid: string;
  ip?: string | null;
}): Promise<{ id: number }> {
  const slug = validateSlug(sceneSlug);
  assertRate(sid, ip);
  return withTx(async (client) => {
    const chain = await client.query<{ id: string }>(CHAIN_SQL, [docId]);
    if (chain.rowCount === 0) throw new FoundryStateError(NO_DOC);
    const chainIds = chain.rows.map((r) => r.id);

    const maker = await client.query(MAKER_SQL, [sid, chainIds]);
    if ((maker.rowCount ?? 0) === 0) {
      const host =
        (await hasRole(client, sid, "host")) &&
        (await consentActive(client, sid, "steward-code"));
      if (!host) throw new FoundryStateError(NOT_MAKER);
    }

    const shelved = await client.query(
      `SELECT 1 FROM foundry.scene WHERE id = $1`,
      [slug],
    );
    if ((shelved.rowCount ?? 0) > 0) {
      throw new FoundryStateError(
        `"${slug}" is already on the shelf \u{2014} pick another id, or open /foundry/play/${slug}.`,
      );
    }
    const busy = await client.query(
      `SELECT 1 FROM foundry.build_request
        WHERE scene_slug = $1 AND status IN ('queued','building','verifying')
        LIMIT 1`,
      [slug],
    );
    if ((busy.rowCount ?? 0) > 0) {
      throw new FoundryStateError(
        `"${slug}" is already being built \u{2014} pick another id.`,
      );
    }
    const active = await client.query(
      `SELECT 1 FROM foundry.build_request
        WHERE doc_id = ANY($1) AND status IN ('queued','building','verifying')
        LIMIT 1`,
      [chainIds],
    );
    if ((active.rowCount ?? 0) > 0) throw new FoundryStateError(ALREADY_UNDERWAY);

    const canon = await canonicalSidTx(client, sid);
    let inserted;
    try {
      inserted = await client.query<{ id: string | number }>(
        `INSERT INTO foundry.build_request (doc_id, scene_slug, requested_by_sid)
         VALUES ($1, $2, $3)
         RETURNING id`,
        [docId, slug, canon],
      );
    } catch (err) {
      // Two same-doc requests racing past the pre-check: the partial unique
      // index build_request_active_once catches the loser.
      if ((err as { code?: string }).code === "23505") {
        throw new FoundryStateError(ALREADY_UNDERWAY);
      }
      throw err;
    }
    await logAction(client, {
      sid,
      action: "request_build",
      subject: docId,
      detail: { scene_slug: slug },
    });
    return { id: Number(inserted.rows[0].id) };
  });
}

/** Loader read: the doc chain's newest request, any status; null = none (render honest absence). */
export async function activeBuildForDoc(
  db: Pool,
  docId: string,
): Promise<BuildRequestRow | null> {
  const res = await db.query<DbRow>(
    `WITH RECURSIVE older AS (
       SELECT d.id, d.supersedes FROM foundry.gdd_doc d WHERE d.id = $1
       UNION
       SELECT d.id, d.supersedes
         FROM foundry.gdd_doc d JOIN older o ON o.supersedes = d.id
     ), newer AS (
       SELECT d.id, d.supersedes FROM foundry.gdd_doc d WHERE d.id = $1
       UNION
       SELECT d.id, d.supersedes
         FROM foundry.gdd_doc d JOIN newer n ON d.supersedes = n.id
     ), chain AS (
       SELECT id FROM older UNION SELECT id FROM newer
     )
     SELECT ${ROW_COLUMNS}
       FROM foundry.build_request b
       JOIN chain c ON c.id = b.doc_id
       ${ROW_JOINS}
      ORDER BY b.created_at DESC, b.id DESC
      LIMIT 1`,
    [docId],
  );
  const r = res.rows[0];
  return r ? toRow(r) : null;
}

/** Broker poll: queued rows, FIFO by created_at. */
export async function claimableBuilds(
  db: Pool,
  limit = 20,
): Promise<BuildRequestRow[]> {
  const res = await db.query<DbRow>(
    `SELECT ${ROW_COLUMNS}
       FROM foundry.build_request b
       ${ROW_JOINS}
      WHERE b.status = 'queued'
      ORDER BY b.created_at, b.id
      LIMIT $1`,
    [limit],
  );
  return res.rows.map(toRow);
}

/** Broker transitions -- the ONLY status writer. Legal edges: queued->building->verifying->{landed,failed},
 *  queued->failed, building->failed. Compare-and-set in-tx: UPDATE ... SET status=$to, detail=$detail,
 *  updated_at=now() WHERE id=$id AND status=$from RETURNING doc_id, scene_slug; zero rows =
 *  FoundryStateError (lost race or illegal edge). logAction {action:"build_status", subject: doc_id,
 *  detail:{scene_slug, status: to, note: detail}}. Never writes foundry.scene -- on 'landed' the broker
 *  separately calls registerScene({sid: brokerHostSid, id: sceneSlug, gddDocId: docId, ...}) so the
 *  shelf row and doc linkage arrive through the existing host-gated writer. */
export async function setBuildStatus({
  id,
  from,
  to,
  detail,
}: {
  id: number;
  from: BuildStatus;
  to: BuildStatus;
  detail: string;
}): Promise<void> {
  if (!LEGAL_EDGES[from]?.includes(to)) {
    throw new FoundryStateError(
      `A build never moves ${from}\u{2192}${to} \u{2014} the ledger runs queued\u{2192}building\u{2192}verifying\u{2192}landed, and any step before landed may fail.`,
    );
  }
  await withTx(async (client) => {
    const res = await client.query<{ doc_id: string; scene_slug: string }>(
      `UPDATE foundry.build_request
          SET status = $1, detail = $2, updated_at = now()
        WHERE id = $3 AND status = $4
        RETURNING doc_id, scene_slug`,
      [to, detail, id, from],
    );
    const row = res.rows[0];
    if (!row) {
      throw new FoundryStateError(
        `Build ${id} is not '${from}' any more \u{2014} re-read its row before moving it.`,
      );
    }
    await logAction(client, {
      sid: BROKER_SID,
      action: "build_status",
      subject: row.doc_id,
      detail: { scene_slug: row.scene_slug, status: to, note: detail },
    });
  });
}
