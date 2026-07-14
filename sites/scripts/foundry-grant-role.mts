#!/usr/bin/env node
// foundry-grant-role — the operator bootstrap for the FIRST privileged holder.
//
//   npm run foundry:grant-role -- --sid <sid> --role admin --note '...'
//
// admin and host can never be self-granted (a visitor cannot mint their own
// authority), so before any privileged holder exists there is no in-app path to
// the first one. This writes a role_grant with granted_by_sid NULL — the
// operator grant the DB CHECK allows — and nothing else. Roles: admin | host |
// create | start.
//
// --rebind moves a role stranded on a lost sid onto the persona that should
// hold it — revoke-and-regrant plus an alias from the lost sid, never an
// UPDATE of any existing row (rebindGrant, roles.server.ts):
//
//   npm run foundry:grant-role -- --rebind --from-sid <A> --sid <B> --role host --note '...'
import { Pool } from "pg";

function flag(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
}

const url =
  flag("db")?.trim() ||
  process.env.FOUNDRY_DATABASE_URL?.trim() ||
  process.env.CATALYST_DATABASE_URL?.trim();

if (!url) {
  console.error(
    "foundry-grant-role: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  process.exit(1);
}

const rebind = process.argv.includes("--rebind");
const sid = flag("sid")?.trim();
const fromSid = flag("from-sid")?.trim();
const role = flag("role")?.trim();
const note = flag("note")?.trim() ?? "";
const ROLES = ["admin", "host", "create", "start"] as const;
type Role = (typeof ROLES)[number];

function isRole(value: string | undefined): value is Role {
  return value !== undefined && (ROLES as readonly string[]).includes(value);
}

if (!sid) {
  console.error("foundry-grant-role: --sid <sid> is required");
  process.exit(1);
}
if (!isRole(role)) {
  console.error(`foundry-grant-role: --role must be one of ${ROLES.join(", ")}`);
  process.exit(1);
}

if (rebind) {
  if (!fromSid) {
    console.error("foundry-grant-role: --rebind needs --from-sid <sid>");
    process.exit(1);
  }
  // rebindGrant reads the connection lazily through the lib's own pool.
  process.env.FOUNDRY_DATABASE_URL = url;
  const { rebindGrant } = await import(
    "../packages/data/src/lib/foundry/roles.server"
  );
  try {
    await rebindGrant({ fromSid, personaSid: sid, role, note });
    console.log(
      `foundry-grant-role: rebound ${role} from ${fromSid} to ${sid}`,
    );
  } catch (e) {
    console.error("foundry-grant-role: rebind failed —", (e as Error).message);
    process.exitCode = 1;
  } finally {
    const { getPool } = await import(
      "../packages/data/src/lib/foundry/db.server"
    );
    await getPool().end();
  }
} else {
  const pool = new Pool({ connectionString: url, max: 1 });

  try {
    const res = await pool.query<{
      id: string;
      sid: string;
      role: string;
      created_at: Date;
    }>(
      `INSERT INTO foundry.role_grant (sid, role, granted_by_sid, note)
       VALUES ($1, $2, NULL, $3)
       RETURNING id, sid, role, created_at`,
      [sid, role, note],
    );
    const r = res.rows[0];
    console.log(
      `foundry-grant-role: granted ${r.role} to ${r.sid} (grant #${r.id}, ${r.created_at.toISOString()})`,
    );
  } catch (e) {
    console.error("foundry-grant-role: failed —", (e as Error).message);
    process.exitCode = 1;
  } finally {
    await pool.end();
  }
}
