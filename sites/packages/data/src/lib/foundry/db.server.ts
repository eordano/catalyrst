import { createHash } from "node:crypto";

import { Pool } from "pg";
import type { PoolClient } from "pg";

// Foundry actions are open to every visitor and hold no privileged token: they
// touch nothing but the program-owned `foundry` schema, every write is attributed
// in action_log, and the blast radius of a hostile visitor is graffiti in a
// sandbox we own. That is why this is not the confused-deputy shape that got
// admin.places-decision.tsx disabled.

export class FoundryUnavailableError extends Error {
  constructor(
    message = "Foundry database not configured (FOUNDRY_DATABASE_URL unset)",
  ) {
    super(message);
    this.name = "FoundryUnavailableError";
  }
}

export class FoundryRateLimitError extends Error {
  readonly status = 429;
  constructor(message = "Too many writes from this session — try again shortly") {
    super(message);
    this.name = "FoundryRateLimitError";
  }
}

export class FoundryStateError extends Error {
  readonly status = 409;
  constructor(message: string) {
    super(message);
    this.name = "FoundryStateError";
  }
}

function connectionString(): string | undefined {
  if (typeof process === "undefined") return undefined;
  const cs =
    process.env.FOUNDRY_DATABASE_URL || process.env.CATALYST_DATABASE_URL;
  return cs && cs.trim() !== "" ? cs : undefined;
}

let pool: Pool | null = null;

export function getPool(): Pool {
  const cs = connectionString();
  if (!cs) throw new FoundryUnavailableError();
  if (!pool) {
    pool = new Pool({
      connectionString: cs,
      max: 5,
      statement_timeout: 60_000,
      idle_in_transaction_session_timeout: 30_000,
      idleTimeoutMillis: 30_000,
    });
  }
  return pool;
}

export function isFoundryConfigured(): boolean {
  return connectionString() !== undefined;
}

export async function withTx<T>(
  fn: (client: PoolClient) => Promise<T>,
): Promise<T> {
  const client = await getPool().connect();
  try {
    await client.query("BEGIN");
    const out = await fn(client);
    await client.query("COMMIT");
    return out;
  } catch (e) {
    try {
      await client.query("ROLLBACK");
    } catch {
      // the connection is already gone; the original error is the useful one
    }
    throw e;
  } finally {
    client.release();
  }
}

export interface FoundryMigration {
  name: string;
  sql: string;
}

// One advisory lock guards the whole migration run, so two boots (or a boot and
// an operator running foundry:import-real) racing on the same database serialise
// instead of both trying the same DDL.
const MIGRATION_LOCK_KEY = 8_675_309_003;

/**
 * Applies any migration in `migrations` whose name is not yet recorded in
 * foundry.foundry_migration, in the order given, one transaction each. Returns
 * the names actually applied.
 *
 * The migration files are read by seed.server.ts (loadMigrations) rather than
 * here: this module is reachable from the route graph, and the built server
 * bundle has no .sql files sitting next to it.
 */
export async function runMigrations(
  pool: Pool,
  migrations: FoundryMigration[],
): Promise<string[]> {
  await pool.query("CREATE SCHEMA IF NOT EXISTS foundry");
  await pool.query(
    `CREATE TABLE IF NOT EXISTS foundry.foundry_migration (
       name       text PRIMARY KEY,
       applied_at timestamptz NOT NULL DEFAULT now()
     )`,
  );

  const client = await pool.connect();
  const applied: string[] = [];
  try {
    await client.query("SELECT pg_advisory_lock($1::bigint)", [
      MIGRATION_LOCK_KEY,
    ]);
    const done = await client.query<{ name: string }>(
      "SELECT name FROM foundry.foundry_migration",
    );
    const seen = new Set(done.rows.map((r) => r.name));

    for (const m of migrations) {
      if (seen.has(m.name)) continue;
      await client.query("BEGIN");
      try {
        await client.query(m.sql);
        await client.query(
          "INSERT INTO foundry.foundry_migration (name) VALUES ($1)",
          [m.name],
        );
        await client.query("COMMIT");
      } catch (e) {
        try {
          await client.query("ROLLBACK");
        } catch {
          // the connection is already gone; the original error is the useful one
        }
        throw e;
      }
      applied.push(m.name);
    }
    return applied;
  } finally {
    try {
      await client.query("SELECT pg_advisory_unlock($1::bigint)", [
        MIGRATION_LOCK_KEY,
      ]);
    } catch {
      // lock dies with the session anyway
    }
    client.release();
  }
}

export type ActionLogEntry = {
  sid: string;
  action: string;
  subject: string;
  detail?: Record<string, unknown>;
};

export async function logAction(
  client: PoolClient,
  entry: ActionLogEntry,
): Promise<void> {
  await client.query(
    `INSERT INTO foundry.action_log (sid, action, subject, detail)
     VALUES ($1, $2, $3, $4::jsonb)`,
    [
      entry.sid,
      entry.action,
      entry.subject,
      JSON.stringify(entry.detail ?? {}),
    ],
  );
}

const WRITES_PER_MINUTE = 10;
const WINDOW_MS = 60_000;
const buckets = new Map<string, number[]>();

function overLimit(key: string, now: number): boolean {
  const recent = (buckets.get(key) ?? []).filter((t) => now - t < WINDOW_MS);
  buckets.set(key, recent);
  return recent.length >= WRITES_PER_MINUTE;
}

function record(key: string, now: number): void {
  const recent = buckets.get(key) ?? [];
  recent.push(now);
  buckets.set(key, recent);
}

/**
 * Caps writes per minute. Bucketing on the sid alone is trivially bypassed by a
 * cookieless client, which is handed a fresh sid on every request; so when a
 * client IP is known it is bucketed too, and a rotating-sid flood still hits the
 * IP ceiling. The caller separately refuses a write with no pre-existing sid.
 */
export function assertRate(sid: string, ip?: string | null): void {
  const now = Date.now();
  const keys = [`sid:${sid}`];
  if (ip && ip.trim() !== "") keys.push(`ip:${ip.trim()}`);
  if (keys.some((k) => overLimit(k, now))) {
    throw new FoundryRateLimitError();
  }
  for (const k of keys) record(k, now);
  if (buckets.size > 5000) {
    for (const [key, stamps] of buckets) {
      if (stamps.every((t) => now - t >= WINDOW_MS)) buckets.delete(key);
    }
  }
}

export function sidBadge(sid: string): string {
  return createHash("sha256").update(sid).digest("hex").slice(0, 4);
}

export function visitorLabel(sid: string): string {
  return `visitor ${sidBadge(sid)}`;
}
