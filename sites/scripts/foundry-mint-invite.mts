#!/usr/bin/env node
// foundry-mint-invite — the operator bootstrap for the first host invite.
//
//   npm run foundry:mint-invite -- --role host --note '...' [--expires <ISO>]
//
// Mints one role_invite (created_via='operator', created_by_sid NULL) and prints
// the code ONCE — it is stored verbatim as the primary key and is the only thing
// that grants the role, so it is not recoverable from any later read here. It is
// redeemed at /foundry/people. Roles: admin | host | create | start.
import { randomBytes } from "node:crypto";

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
    "foundry-mint-invite: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  process.exit(1);
}

const role = flag("role")?.trim();
const note = flag("note")?.trim() ?? "";
const expires = flag("expires")?.trim() || null;
const ROLES = ["admin", "host", "create", "start"];

if (!role || !ROLES.includes(role)) {
  console.error(`foundry-mint-invite: --role must be one of ${ROLES.join(", ")}`);
  process.exit(1);
}
if (expires !== null && !Number.isFinite(Date.parse(expires))) {
  console.error("foundry-mint-invite: --expires must be an ISO timestamp");
  process.exit(1);
}

const pool = new Pool({ connectionString: url, max: 1 });

try {
  const code = randomBytes(9).toString("hex");
  await pool.query(
    `INSERT INTO foundry.role_invite
       (code, role, note, created_via, created_by_sid, expires_at)
     VALUES ($1, $2, $3, 'operator', NULL, $4::timestamptz)`,
    [code, role, note, expires],
  );
  console.log(`foundry-mint-invite: minted a ${role} invite`);
  console.log(`  code: ${code}`);
  console.log("  redeem it at /foundry/people — it is shown only this once.");
} catch (e) {
  console.error("foundry-mint-invite: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
