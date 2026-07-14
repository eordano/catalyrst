import { createHash, randomBytes } from "node:crypto";

import type { PoolClient } from "pg";

import {
  FoundryStateError,
  assertRate,
  getPool,
  logAction,
  recordRate,
  withTx,
} from "./db.server";
import { clearGateMemo } from "./gate-memo.server";
import type { Persona, PersonaLabel, RoleName } from "./types";

// A persona is the one editable identity a visitor owns: a chosen name and an
// avatar spec over the real DCL base-avatar catalog, keyed to the durable sid.
// A session may only ever write its OWN sid row — every function here takes the
// caller's sid and writes exactly that. There is no seeded persona; an
// unclaimed session is `null`, and the UI shows the honest visitor badge.

function isRole(value: string): value is RoleName {
  return (
    value === "admin" ||
    value === "host" ||
    value === "create" ||
    value === "start"
  );
}

function iso(value: Date | string): string {
  return value instanceof Date ? value.toISOString() : String(value);
}

function toAvatar(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

type PersonaDbRow = {
  sid: string;
  display_name: string;
  avatar_body_urn: string | null;
  avatar: unknown;
  words: string | null;
  claimed_at: Date | string;
  updated_at: Date | string;
};

function toPersona(r: PersonaDbRow): Persona {
  return {
    sid: r.sid,
    displayName: r.display_name,
    avatarBodyUrn: r.avatar_body_urn,
    avatar: toAvatar(r.avatar),
    words: r.words,
    claimedAt: iso(r.claimed_at),
    updatedAt: iso(r.updated_at),
  };
}

export async function getPersona(sid: string): Promise<Persona | null> {
  const res = await getPool().query<PersonaDbRow>(
    `SELECT sid, display_name, avatar_body_urn, avatar, words, claimed_at, updated_at
       FROM foundry.persona WHERE sid = $1`,
    [sid],
  );
  const row = res.rows[0];
  return row ? toPersona(row) : null;
}

export interface PersonaInput {
  sid: string;
  displayName: string;
  avatarBodyUrn: string | null;
  avatar: Record<string, unknown>;
  /** Optional self-description; empty string clears it to NULL. */
  words?: string | null;
  ip?: string | null;
}

const NAME_TAKEN = "That name is taken.";

export const HANDLE_RESERVED =
  "That name belongs to the author of an ask imported from a public thread. If it is yours, an operator can release it to you.";

/** The case-insensitive collision rule for import-reserved author handles —
 *  the same comparison the reserved_handle_ci index enforces, exposed pure so
 *  the fixture's handles can be tested against it. */
export function isHandleReserved(
  name: string,
  handles: readonly string[],
): boolean {
  const wanted = name.trim().toLowerCase();
  return handles.some((h) => h.trim().toLowerCase() === wanted);
}

// Import-reserved author handles live in their own table (they are not personas
// and must not become rows in one). Checked inside the same transaction as the
// write, per the moderation pattern; released rows stop reserving.
async function assertHandleNotReserved(
  client: PoolClient,
  name: string,
): Promise<void> {
  const res = await client.query<{ handle: string }>(
    `SELECT handle FROM foundry.reserved_handle
      WHERE released_at IS NULL AND lower(handle) = lower($1)`,
    [name.trim()],
  );
  if (isHandleReserved(name, res.rows.map((r) => r.handle))) {
    throw new FoundryStateError(HANDLE_RESERVED);
  }
}

// Mirrors the persona_name_* CHECKs in schema.sql so the form's own sentence
// comes back inline instead of a constraint violation turned into generic copy.
// The DB CHECKs stay authoritative; this is the readable front for them.
export const PERSONA_RESERVED_NAMES: readonly string[] = [
  "admin",
  "anonymous",
  "visitor",
  "operator",
  "foundry",
  "system",
  "host",
];

const PERSONA_NAME_RE = /^[A-Za-z0-9 ._-]{2,32}$/;

/** Null when the name passes the DB's own rules; otherwise the sentence to show. */
export function validateDisplayName(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed.length < 2 || trimmed.length > 32) {
    return "2–32 characters. Letters, numbers, spaces and . _ - only.";
  }
  if (!PERSONA_NAME_RE.test(trimmed)) {
    return "Letters, numbers, spaces and . _ - only.";
  }
  if (PERSONA_RESERVED_NAMES.includes(trimmed.toLowerCase())) {
    return "That name is reserved.";
  }
  return null;
}

// The DB owns the real name rules (length, charset, reserved words) as CHECKs;
// a violation surfaces as a Postgres error the route turns into generic copy.
// The one collision worth a specific sentence is the case-insensitive unique
// name, mapped from 23505 here so the poster is told plainly.
function mapNameCollision(e: unknown): never {
  if ((e as { code?: string }).code === "23505") {
    throw new FoundryStateError(NAME_TAKEN);
  }
  throw e;
}

export interface PersonaClaimResult {
  persona: Persona;
  /** The one-time return code, minted with the FIRST claim only — the update
   *  path carries null. Never stored, never re-readable. */
  returnCode: string | null;
}

export async function claimPersona(
  input: PersonaInput,
): Promise<PersonaClaimResult> {
  const { sid, displayName, avatarBodyUrn, avatar, words, ip } = input;
  // The FIRST claim is the session's one essential write: refusing it can
  // strand a just-redeemed single-use invite behind the budget the journey's
  // own writes spent. It is still counted; updates stay limited.
  const owned = await getPool().query(
    `SELECT 1 FROM foundry.persona WHERE sid = $1`,
    [sid],
  );
  if ((owned.rowCount ?? 0) > 0) assertRate(sid, ip);
  else recordRate(sid, ip);
  return withTx(async (client) => {
    await assertHandleNotReserved(client, displayName);
    try {
      const res = await client.query<PersonaDbRow & { inserted: boolean }>(
        `INSERT INTO foundry.persona (sid, display_name, avatar_body_urn, avatar, words)
         VALUES ($1, $2, $3, $4::jsonb, $5)
         ON CONFLICT (sid) DO UPDATE
            SET display_name = EXCLUDED.display_name,
                avatar_body_urn = EXCLUDED.avatar_body_urn,
                avatar = EXCLUDED.avatar,
                words = EXCLUDED.words,
                updated_at = now()
         RETURNING sid, display_name, avatar_body_urn, avatar, words, claimed_at, updated_at,
                   (xmax = 0) AS inserted`,
        [sid, displayName.trim(), avatarBodyUrn, JSON.stringify(avatar),
         words?.trim() ? words.trim() : null],
      );
      const row = res.rows[0];
      await logAction(client, {
        sid,
        action: row.inserted ? "claim_persona" : "update_persona",
        subject: sid,
        detail: { display_name: row.display_name },
      });
      // A brand-new persona gets its return code in the same transaction, so
      // no persona ever exists without a recovery path.
      const minted = row.inserted ? await mintCarryCodeTx(client, sid) : null;
      return { persona: toPersona(row), returnCode: minted?.code ?? null };
    } catch (e) {
      mapNameCollision(e);
    }
  });
}

export async function updatePersona(input: PersonaInput): Promise<Persona> {
  const { sid, displayName, avatarBodyUrn, avatar, words, ip } = input;
  assertRate(sid, ip);
  return withTx(async (client) => {
    await assertHandleNotReserved(client, displayName);
    try {
      const res = await client.query<PersonaDbRow>(
        `UPDATE foundry.persona
            SET display_name = $2, avatar_body_urn = $3, avatar = $4::jsonb,
                words = $5, updated_at = now()
          WHERE sid = $1
          RETURNING sid, display_name, avatar_body_urn, avatar, words, claimed_at, updated_at`,
        [sid, displayName.trim(), avatarBodyUrn, JSON.stringify(avatar),
         words?.trim() ? words.trim() : null],
      );
      const row = res.rows[0];
      if (!row) {
        throw new FoundryStateError("No persona claimed for this session yet.");
      }
      await logAction(client, {
        sid,
        action: "update_persona",
        subject: sid,
        detail: { display_name: row.display_name },
      });
      return toPersona(row);
    } catch (e) {
      mapNameCollision(e);
    }
  });
}

// A carry code moves the persona to another browser by re-issuing its own sid
// cookie there. The code is a bearer secret: 20 chars over a 31-char alphabet
// (~99 bits), shown once at mint, stored only as sha256. Lookalike letters
// (i l o 0 1) are excluded so a hand-copied code survives the trip.
const CARRY_ALPHABET = "abcdefghjkmnpqrstuvwxyz23456789";
const CARRY_CODE_CHARS = 20;

function generateCarryCode(): string {
  const bytes = randomBytes(CARRY_CODE_CHARS);
  let out = "";
  for (let i = 0; i < CARRY_CODE_CHARS; i++) {
    out += CARRY_ALPHABET[bytes[i] % CARRY_ALPHABET.length];
    if ((i + 1) % 5 === 0 && i + 1 < CARRY_CODE_CHARS) out += "-";
  }
  return out;
}

/** Dashes/case are presentation; the hash is over the bare characters. */
export function normalizeCarryCode(code: string): string {
  return code.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function hashCarryCode(code: string): string {
  return createHash("sha256").update(normalizeCarryCode(code)).digest("hex");
}

/** Session-plumbing actions that never surface on public feeds: carrying a
 *  persona between browsers is the holder's business, not community news. The
 *  rows stay in the log — the holder's own page and the audit trail read them. */
export const PRIVATE_LOG_ACTIONS =
  "('mint_carry_code','redeem_carry_code','rebind_grant')";

const CARRY_NO_PERSONA = "Claim a persona first — a carry code moves one that exists.";
const CARRY_CODE_DEAD =
  "That code doesn't open anything. Codes are single-use — mint a fresh one in the browser that holds the persona.";
const CARRY_SAME_SESSION = "This browser already holds that persona.";

export interface CarryMintResult {
  /** The one-time plaintext, returned to the holder and never stored. */
  code: string;
  /** True when an earlier active code was superseded by this mint. */
  replaced: boolean;
}

// The mint itself, shared by the standalone mint and the claim transaction.
// Assumes the persona row exists (the FK would refuse otherwise).
async function mintCarryCodeTx(
  client: PoolClient,
  personaSid: string,
): Promise<CarryMintResult> {
  const code = generateCarryCode();
  const superseded = await client.query(
    `UPDATE foundry.persona_carry_code SET revoked_at = now()
      WHERE sid = $1 AND revoked_at IS NULL AND redeemed_at IS NULL`,
    [personaSid],
  );
  await client.query(
    `INSERT INTO foundry.persona_carry_code (sid, code_hash) VALUES ($1, $2)`,
    [personaSid, hashCarryCode(code)],
  );
  // The detail never carries the code or its hash — the mint is the fact.
  await logAction(client, {
    sid: personaSid,
    action: "mint_carry_code",
    subject: personaSid,
    detail: {},
  });
  return { code, replaced: (superseded.rowCount ?? 0) > 0 };
}

export async function mintCarryCode(
  sid: string,
  ip?: string | null,
): Promise<CarryMintResult> {
  assertRate(sid, ip);
  return withTx(async (client) => {
    const holder = await client.query(
      `SELECT 1 FROM foundry.persona WHERE sid = $1`,
      [sid],
    );
    if (holder.rowCount === 0) throw new FoundryStateError(CARRY_NO_PERSONA);
    return mintCarryCodeTx(client, sid);
  });
}

/** Whether an unredeemed, unrevoked return code stands for this persona —
 *  false drives the persona page's mint banner, so a pre-code persona can arm
 *  its own recovery without an operator. */
export async function hasActiveCarryCode(sid: string): Promise<boolean> {
  const res = await getPool().query(
    `SELECT 1 FROM foundry.persona_carry_code
      WHERE sid = $1 AND revoked_at IS NULL AND redeemed_at IS NULL`,
    [sid],
  );
  return (res.rowCount ?? 0) > 0;
}

export interface CarryStatus {
  /** Mint time of the active (unredeemed, unrevoked) code, or null. */
  activeSince: string | null;
  /** Redeem time of the last redeemed code, or null — the "it worked" fact. */
  lastRedeemedAt: string | null;
}

/** What the holder can honestly be shown about their codes without ever
 *  re-showing one: whether an active code exists and when it was minted. */
export async function carryStatus(sid: string): Promise<CarryStatus> {
  const res = await getPool().query<{
    active_since: Date | string | null;
    last_redeemed: Date | string | null;
  }>(
    `SELECT (SELECT max(created_at) FROM foundry.persona_carry_code
              WHERE sid = $1 AND revoked_at IS NULL AND redeemed_at IS NULL) AS active_since,
            (SELECT max(redeemed_at) FROM foundry.persona_carry_code
              WHERE sid = $1) AS last_redeemed`,
    [sid],
  );
  const r = res.rows[0];
  return {
    activeSince: r?.active_since == null ? null : iso(r.active_since),
    lastRedeemedAt: r?.last_redeemed == null ? null : iso(r.last_redeemed),
  };
}

export interface CarryRedeemResult {
  /** The persona's own sid — the caller re-issues it as the session cookie. */
  sid: string;
  displayName: string;
}

/**
 * Redeems a carry code in a NEW browser: burns the code (single-use, recorded)
 * and returns the persona's sid for the caller to set as the session cookie.
 * The abandoned sid becomes an alias of the persona — its pre-redeem acts stay
 * where they were written and resolve to the persona at read time — unless it
 * owns a persona of its own, which is never aliased away (that persona stays
 * reachable through its own return code). Refused when the code was already
 * spent, superseded, or never real.
 */
export async function redeemCarryCode(
  currentSid: string,
  code: string,
  ip?: string | null,
): Promise<CarryRedeemResult> {
  // Counted but never refused: this is the persona's one recovery write, gated
  // by a single-use ~99-bit bearer code — a budget refusal here can strand the
  // very recovery the code exists for.
  recordRate(currentSid, ip);
  if (normalizeCarryCode(code).length !== CARRY_CODE_CHARS) {
    throw new FoundryStateError(CARRY_CODE_DEAD);
  }
  const redeemed = await withTx(async (client) => {
    const found = await client.query<{
      id: string;
      sid: string;
      revoked_at: Date | string | null;
      redeemed_at: Date | string | null;
    }>(
      `SELECT id, sid, revoked_at, redeemed_at FROM foundry.persona_carry_code
        WHERE code_hash = $1 FOR UPDATE`,
      [hashCarryCode(code)],
    );
    const row = found.rows[0];
    if (!row || row.revoked_at !== null || row.redeemed_at !== null) {
      throw new FoundryStateError(CARRY_CODE_DEAD);
    }
    if (row.sid === currentSid) throw new FoundryStateError(CARRY_SAME_SESSION);
    const target = await client.query<{ display_name: string }>(
      `SELECT display_name FROM foundry.persona WHERE sid = $1`,
      [row.sid],
    );
    const persona = target.rows[0];
    if (!persona) throw new FoundryStateError(CARRY_CODE_DEAD);
    await client.query(
      `UPDATE foundry.persona_carry_code
          SET redeemed_at = now(), redeemed_from = $2
        WHERE id = $1`,
      [row.id, currentSid],
    );
    const mine = await client.query(
      `SELECT 1 FROM foundry.persona WHERE sid = $1`,
      [currentSid],
    );
    if (mine.rowCount === 0) {
      await client.query(
        `INSERT INTO foundry.sid_alias (alias_sid, persona_sid, via)
         VALUES ($1, $2, 'return-code')
         ON CONFLICT (alias_sid) DO NOTHING`,
        [currentSid, row.sid],
      );
    }
    await logAction(client, {
      sid: row.sid,
      action: "redeem_carry_code",
      subject: row.sid,
      detail: {},
    });
    return { sid: row.sid, displayName: persona.display_name };
  });
  // The fresh sid may hold a cached 401 verdict from before it was anyone.
  clearGateMemo(redeemed.sid, currentSid);
  return redeemed;
}

export interface PersonaHistoryRow {
  at: string;
  detail: Record<string, unknown>;
}

/** The session's own rename/claim trail, read from its action_log rows. */
export async function personaHistory(sid: string): Promise<PersonaHistoryRow[]> {
  const res = await getPool().query<{ at: Date | string; detail: unknown }>(
    `SELECT at, detail FROM foundry.action_log
      WHERE sid = $1 AND action IN ('claim_persona','update_persona')
      ORDER BY at DESC`,
    [sid],
  );
  return res.rows.map((r) => ({
    at: iso(r.at),
    detail: toAvatar(r.detail),
  }));
}

/**
 * Names + avatars for a set of sids in one round trip. A sid with no claimed
 * persona maps to `null`; the caller renders the honest badge for those, never
 * an invented name. A raw sid is never returned as a label — only the persona
 * the holder authored.
 */
export async function personaLabels(
  client: PoolClient,
  sids: readonly string[],
): Promise<Map<string, PersonaLabel | null>> {
  const map = new Map<string, PersonaLabel | null>();
  for (const s of sids) map.set(s, null);
  if (sids.length === 0) return map;
  const res = await client.query<{
    sid: string;
    display_name: string;
    avatar_body_urn: string | null;
    avatar: unknown;
  }>(
    `SELECT q.sid, p.display_name, p.avatar_body_urn, p.avatar
       FROM unnest($1::text[]) AS q(sid)
       LEFT JOIN foundry.sid_alias al ON al.alias_sid = q.sid
       JOIN foundry.persona p ON p.sid = COALESCE(al.persona_sid, q.sid)`,
    [[...new Set(sids)]],
  );
  for (const r of res.rows) {
    map.set(r.sid, {
      name: r.display_name,
      avatarBodyUrn: r.avatar_body_urn,
      avatar: toAvatar(r.avatar),
    });
  }
  return map;
}

export interface PersonaDirectoryRow {
  sid: string;
  displayName: string;
  avatarBodyUrn: string | null;
  avatar: Record<string, unknown>;
  /** The persona's own self-description, verbatim; null = never written. */
  words: string | null;
  claimedAt: string;
  roles: RoleName[];
  requests: number;
  /** Ids of the asks behind the requests count, in posting order. */
  requestIds: string[];
  pledges: number;
  /** Ask ids the pledges were made on, in pledge order. */
  pledgeRequestIds: string[];
  lastSeen: string | null;
}

// A persona owns rows written under its own sid and under any alias of it —
// the read-side half of the alias layer. Pledge counts dedupe by request so a
// pledge made twice across the set is one voice, not two.
const OWNED = (col: string, owner = "p.sid") =>
  `(${col} = ${owner} OR ${col} IN
     (SELECT alias_sid FROM foundry.sid_alias WHERE persona_sid = ${owner}))`;

/**
 * The People directory: one row per CLAIMED persona (bare sids are never
 * enumerated). Each carries measured counts — requests posted, pledges made,
 * last action — and its active roles. Empty counts are real zeros; lastSeen is
 * null for a persona that has no action_log row, which the UI prints as '—'.
 */
export async function listPersonas(): Promise<PersonaDirectoryRow[]> {
  const res = await getPool().query<{
    sid: string;
    display_name: string;
    avatar_body_urn: string | null;
    avatar: unknown;
    words: string | null;
    claimed_at: Date | string;
    roles: string[] | null;
    requests: number;
    request_ids: string[] | null;
    pledges: number;
    pledge_request_ids: string[] | null;
    last_seen: Date | string | null;
  }>(
    `SELECT p.sid, p.display_name, p.avatar_body_urn, p.avatar, p.words, p.claimed_at,
            (SELECT count(*)::int FROM foundry.request r
              WHERE ${OWNED("r.sid")} AND r.origin = 'visitor') AS requests,
            (SELECT array_agg(r.id ORDER BY r.created_at) FROM foundry.request r
              WHERE ${OWNED("r.sid")} AND r.origin = 'visitor') AS request_ids,
            (SELECT count(DISTINCT pl.request_id)::int FROM foundry.pledge pl
              WHERE ${OWNED("pl.sid")}) AS pledges,
            (SELECT array_agg(request_id ORDER BY first_at) FROM
              (SELECT pl.request_id, min(pl.created_at) AS first_at
                 FROM foundry.pledge pl WHERE ${OWNED("pl.sid")}
                GROUP BY pl.request_id) dedup) AS pledge_request_ids,
            (SELECT max(a.at) FROM foundry.action_log a
              WHERE ${OWNED("a.sid")}) AS last_seen,
            (SELECT array_agg(DISTINCT g.role ORDER BY g.role) FROM foundry.role_grant g
              WHERE ${OWNED("g.sid")} AND g.revoked_at IS NULL) AS roles
       FROM foundry.persona p
      ORDER BY p.claimed_at ASC`,
  );
  return res.rows.map((r) => ({
    sid: r.sid,
    displayName: r.display_name,
    avatarBodyUrn: r.avatar_body_urn,
    avatar: toAvatar(r.avatar),
    words: r.words,
    claimedAt: iso(r.claimed_at),
    roles: (r.roles ?? []).filter(isRole),
    requests: Number(r.requests),
    requestIds: r.request_ids ?? [],
    pledges: Number(r.pledges),
    pledgeRequestIds: r.pledge_request_ids ?? [],
    lastSeen: r.last_seen === null ? null : iso(r.last_seen),
  }));
}

export interface PeopleSnapshot {
  personas: number;
  rolesHeld: number;
  admins: number;
}

const PEOPLE_SNAPSHOT_SQL = `
  SELECT (SELECT count(*)::int FROM foundry.persona) AS personas,
         (SELECT count(DISTINCT role)::int FROM foundry.role_grant
           WHERE revoked_at IS NULL) AS roles_held,
         (SELECT count(*)::int FROM foundry.role_grant
           WHERE role = 'admin' AND revoked_at IS NULL) AS admins`;

/** Header tiles for the People page — all measured counts, zero when empty. */
export async function peopleSnapshot(): Promise<PeopleSnapshot> {
  const res = await getPool().query<{
    personas: number;
    roles_held: number;
    admins: number;
  }>(PEOPLE_SNAPSHOT_SQL);
  const r = res.rows[0];
  return {
    personas: Number(r?.personas ?? 0),
    rolesHeld: Number(r?.roles_held ?? 0),
    admins: Number(r?.admins ?? 0),
  };
}

export interface PersonStewarding {
  sceneId: string;
  sceneTitle: string;
  since: string;
  basis: string;
}

export interface PersonAsk {
  id: string;
  title: string;
  at: string;
}

export interface PersonAct {
  at: string;
  action: string;
  subjectLabel: string | null;
  subjectKind: "scene" | "session" | "request" | "doc" | null;
  subjectId: string | null;
}

/** Everything the site honestly knows about one claimed persona, looked up by
 *  its case-insensitively-unique display name. Every list is real rows keyed
 *  to the persona's sid; the sid itself never leaves this function. Null =
 *  no persona carries that name. */
export interface PersonProfile {
  displayName: string;
  avatarBodyUrn: string | null;
  avatar: Record<string, unknown>;
  words: string | null;
  claimedAt: string;
  roles: RoleName[];
  lastSeen: string | null;
  stewarding: PersonStewarding[];
  asks: PersonAsk[];
  pledges: PersonAsk[];
  acts: PersonAct[];
}

const PROFILE_SCENE_ACTIONS =
  "('claim_steward','release_steward','scene_note','offer_transfer','revoke_transfer','accept_transfer')";
const PROFILE_SESSION_ACTIONS =
  "('schedule_session','retire_session','rsvp_session','withdraw_rsvp')";
const PROFILE_REQUEST_ACTIONS =
  "('post_request','pledge','withdraw_pledge','approve_request','close_request')";
const PROFILE_GDD_ACTIONS =
  "('approve_gdd','edit_gdd_doc','publish_gdd_draft')";

export async function personProfile(
  displayName: string,
): Promise<PersonProfile | null> {
  const db = getPool();
  const head = await db.query<PersonaDbRow>(
    `SELECT sid, display_name, avatar_body_urn, avatar, words, claimed_at, updated_at
       FROM foundry.persona WHERE lower(display_name) = lower($1)`,
    [displayName],
  );
  const row = head.rows[0];
  if (!row) return null;
  const sid = row.sid;

  const [roles, lastSeen, stewarding, asks, pledges, acts] = await Promise.all([
    db.query<{ role: string }>(
      `SELECT role FROM foundry.role_grant
        WHERE ${OWNED("sid", "$1")} AND revoked_at IS NULL
        GROUP BY role ORDER BY min(created_at)`,
      [sid],
    ),
    db.query<{ at: Date | string | null }>(
      `SELECT max(at) AS at FROM foundry.action_log WHERE ${OWNED("sid", "$1")}`,
      [sid],
    ),
    db.query<{ scene_id: string; title: string; since: Date | string; basis: string }>(
      `SELECT st.scene_id, sc.title, st.since, st.basis
         FROM foundry.scene_steward st
         JOIN foundry.scene sc ON sc.id = st.scene_id
        WHERE ${OWNED("st.sid", "$1")} AND st.released_at IS NULL
        ORDER BY st.since`,
      [sid],
    ),
    db.query<{ id: string; title: string; at: Date | string }>(
      `SELECT id, title, created_at AS at FROM foundry.request
        WHERE ${OWNED("sid", "$1")} AND origin = 'visitor' ORDER BY created_at`,
      [sid],
    ),
    db.query<{ id: string; title: string; at: Date | string }>(
      `SELECT r.id, r.title, min(pl.created_at) AS at
         FROM foundry.pledge pl JOIN foundry.request r ON r.id = pl.request_id
        WHERE ${OWNED("pl.sid", "$1")}
        GROUP BY r.id, r.title ORDER BY min(pl.created_at)`,
      [sid],
    ),
    db.query<{
      at: Date | string;
      action: string;
      subject: string | null;
      subject_label: string | null;
      subject_kind: string | null;
    }>(
      `SELECT a.at, a.action, a.subject,
              CASE
                WHEN a.action IN ${PROFILE_SCENE_ACTIONS} THEN sc.title
                WHEN a.action IN ${PROFILE_SESSION_ACTIONS} THEN ss.title
                WHEN a.action IN ${PROFILE_REQUEST_ACTIONS} THEN r.title
                WHEN a.action IN ${PROFILE_GDD_ACTIONS} THEN gd.title
                ELSE NULL
              END AS subject_label,
              CASE
                WHEN a.action IN ${PROFILE_SCENE_ACTIONS} THEN 'scene'
                WHEN a.action IN ${PROFILE_SESSION_ACTIONS} THEN 'session'
                WHEN a.action IN ${PROFILE_REQUEST_ACTIONS} THEN 'request'
                WHEN a.action IN ${PROFILE_GDD_ACTIONS} THEN 'doc'
                ELSE NULL
              END AS subject_kind
         FROM foundry.action_log a
         LEFT JOIN foundry.request r ON r.id = a.subject
         LEFT JOIN foundry.scene sc ON sc.id = a.subject
         LEFT JOIN foundry.session_series ss ON ss.id = a.subject
         LEFT JOIN foundry.gdd_doc gd ON gd.id = a.subject
        WHERE ${OWNED("a.sid", "$1")} AND a.action NOT IN ${PRIVATE_LOG_ACTIONS}
        ORDER BY a.at DESC
        LIMIT 30`,
      [sid],
    ),
  ]);

  const toAsk = (r: { id: string; title: string; at: Date | string }): PersonAsk => ({
    id: r.id,
    title: r.title,
    at: iso(r.at),
  });

  return {
    displayName: row.display_name,
    avatarBodyUrn: row.avatar_body_urn,
    avatar: toAvatar(row.avatar),
    words: row.words,
    claimedAt: iso(row.claimed_at),
    roles: roles.rows.map((r) => r.role).filter(isRole),
    lastSeen: lastSeen.rows[0]?.at == null ? null : iso(lastSeen.rows[0].at),
    stewarding: stewarding.rows.map((r) => ({
      sceneId: r.scene_id,
      sceneTitle: r.title,
      since: iso(r.since),
      basis: r.basis,
    })),
    asks: asks.rows.map(toAsk),
    pledges: pledges.rows.map(toAsk),
    acts: acts.rows.map((a) => ({
      at: iso(a.at),
      action: a.action,
      subjectLabel: a.subject_label,
      subjectKind:
        a.subject_kind === "scene" ||
        a.subject_kind === "session" ||
        a.subject_kind === "request" ||
        a.subject_kind === "doc"
          ? a.subject_kind
          : null,
      subjectId: a.subject_label !== null ? a.subject : null,
    })),
  };
}
