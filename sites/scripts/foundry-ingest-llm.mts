#!/usr/bin/env node
// foundry-ingest-llm — records what the copilot has actually spent.
//
//   npm run foundry:ingest-llm -- [--db <url>] [--socket <path>] [--copilot <url>]
//                                 [--payload-out <path>]
//
// Reads opencode's own HTTP API: `GET /session`, then `GET /session/{id}/message`
// for each one, and writes one llm_usage row per assistant message. Token counts
// and the cost are taken verbatim from the fields the gateway reported — this
// script computes no usage of its own, and a message opencode never accounted
// for produces no row.
//
// The service listens inside a network-isolated sandbox and is reachable only
// through the unix socket its wrapper bridges out (FOUNDRY_COPILOT_SOCKET,
// default /srv/foundry-copilot/run/opencode.sock). --copilot <url> is the
// fallback for a deployment that exposes a loopback port instead.
//
// Basic auth comes from FOUNDRY_COPILOT_PASSWORD (user FOUNDRY_COPILOT_USERNAME,
// default `opencode`), matching the service's own OPENCODE_SERVER_PASSWORD.
// FOUNDRY_COPILOT_DIRECTORY names the workspace: opencode answers per-project
// and serves an empty "global" project to a request that does not name one.
//
// Intended cadence: the same main-loop cron that scans the workspace for GDDs.
// If the connection has no rights to write, the rows are saved as JSON and the
// script exits 3 rather than retrying.
import { mkdirSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { dirname, isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Pool } from "pg";

import { upsertLlmUsage, type LlmUsageInput } from "../packages/data/src/lib/foundry/costs.server";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const EXIT_PARKED = 3;
const EXIT_UNREACHABLE = 4;

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
    "foundry-ingest-llm: pass --db <url>, or set FOUNDRY_DATABASE_URL (or CATALYST_DATABASE_URL)",
  );
  process.exit(1);
}

const socketPath =
  flag("socket")?.trim() ||
  process.env.FOUNDRY_COPILOT_SOCKET?.trim() ||
  (flag("copilot") || process.env.FOUNDRY_COPILOT_INTERNAL_URL
    ? ""
    : "/srv/foundry-copilot/run/opencode.sock");

const copilot = (
  flag("copilot")?.trim() ||
  process.env.FOUNDRY_COPILOT_INTERNAL_URL?.trim() ||
  "http://127.0.0.1:14096"
).replace(/\/+$/, "");

const where = socketPath ? `unix:${socketPath}` : copilot;

function headers(): Record<string, string> {
  const h: Record<string, string> = {
    accept: "application/json",
    "x-opencode-directory":
      process.env.FOUNDRY_COPILOT_DIRECTORY?.trim() || "/srv/foundry-copilot/workspace",
  };
  const password = process.env.FOUNDRY_COPILOT_PASSWORD?.trim();
  if (password) {
    const user = process.env.FOUNDRY_COPILOT_USERNAME?.trim() || "opencode";
    h.authorization = `Basic ${Buffer.from(`${user}:${password}`).toString("base64")}`;
  }
  return h;
}

function overSocket<T>(path: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const req = request({ socketPath, path, method: "GET", headers: headers() }, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (c) => (body += c));
      res.on("end", () => {
        if ((res.statusCode ?? 0) < 200 || (res.statusCode ?? 0) >= 300) {
          reject(new Error(`${path} → HTTP ${res.statusCode}`));
          return;
        }
        try {
          resolve(JSON.parse(body) as T);
        } catch (e) {
          reject(new Error(`${path} → unparseable response: ${(e as Error).message}`));
        }
      });
    });
    req.on("error", reject);
    req.end();
  });
}

async function api<T>(path: string): Promise<T> {
  if (socketPath) return overSocket<T>(path);
  const res = await fetch(`${copilot}${path}`, { headers: headers() });
  if (!res.ok) throw new Error(`${path} → HTTP ${res.status}`);
  return (await res.json()) as T;
}

type SessionJson = { id?: string; title?: string };

type Tokens = {
  input?: number;
  output?: number;
  reasoning?: number;
  cache?: { read?: number; write?: number };
};

type MessageJson = {
  id?: string;
  info?: MessageJson;
  role?: string;
  sessionID?: string;
  modelID?: string;
  providerID?: string;
  tokens?: Tokens;
  cost?: number;
  time?: { created?: number; completed?: number };
};

function num(value: unknown): number {
  const n = Number(value);
  return Number.isFinite(n) ? n : 0;
}

/**
 * The list endpoint returns bare messages in some versions and `{info, parts}`
 * envelopes in others; both carry the same assistant fields, so unwrap and read
 * the one shape.
 */
function toRow(raw: MessageJson, session: SessionJson): LlmUsageInput | null {
  const m = raw.info ?? raw;
  if (m.role !== "assistant") return null;
  const id = m.id ?? raw.id;
  if (!id) return null;

  const tokens = m.tokens;
  // No usage accounting means the gateway reported none. A zero row would claim
  // a measurement that was never taken.
  if (!tokens || (tokens.input === undefined && tokens.output === undefined)) return null;

  const created = m.time?.completed ?? m.time?.created;
  const model = [m.providerID, m.modelID].filter(Boolean).join("/") || (m.modelID ?? "unknown");

  return {
    messageId: id,
    sessionId: m.sessionID ?? session.id ?? "unknown",
    sessionTitle: session.title ?? null,
    model,
    inputTokens: num(tokens.input),
    outputTokens: num(tokens.output),
    reasoningTokens: num(tokens.reasoning),
    cacheReadTokens: num(tokens.cache?.read),
    cacheWriteTokens: num(tokens.cache?.write),
    costUsd: typeof m.cost === "number" ? m.cost : null,
    at: new Date(created ? Number(created) : Date.now()).toISOString(),
  };
}

function payloadPath(): string {
  const explicit = flag("payload-out");
  if (explicit) return isAbsolute(explicit) ? explicit : join(SITES, explicit);
  const dir =
    process.env.FOUNDRY_PENDING_INGEST_DIR?.trim() ||
    join(SITES, ".foundry-pending-ingest");
  return join(dir, "foundry-ingest-llm.json");
}

function isPermissionDenied(e: unknown): boolean {
  const err = e as { code?: string; message?: string };
  if (err.code === "42501" || err.code === "42P01") return true;
  return /permission denied|must be owner/i.test(err.message ?? "");
}

let rows: LlmUsageInput[] = [];

try {
  const sessions = await api<SessionJson[]>("/session");
  for (const session of sessions) {
    if (!session.id) continue;
    const messages = await api<MessageJson[]>(
      `/session/${encodeURIComponent(session.id)}/message`,
    );
    for (const raw of messages) {
      const row = toRow(raw, session);
      if (row) rows.push(row);
    }
  }
  console.log(
    `foundry-ingest-llm: ${sessions.length} session(s), ${rows.length} accounted message(s) at ${where}`,
  );
} catch (e) {
  console.error(`foundry-ingest-llm: copilot not readable at ${where} — ${(e as Error).message}`);
  console.error(
    "foundry-ingest-llm: nothing was written. The costs page shows an empty ledger, which is the truth until the service answers.",
  );
  process.exit(EXIT_UNREACHABLE);
}

if (rows.length === 0) {
  console.log("foundry-ingest-llm: nothing to ingest");
  process.exit(0);
}

const pool = new Pool({ connectionString: url, max: 2 });

try {
  for (const row of rows) await upsertLlmUsage(pool, row);
  const totals = rows.reduce(
    (acc, r) => ({
      input: acc.input + r.inputTokens,
      output: acc.output + r.outputTokens,
    }),
    { input: 0, output: 0 },
  );
  console.log(
    `foundry-ingest-llm: ${rows.length} row(s) upserted — ${totals.input} in / ${totals.output} out`,
  );
} catch (e) {
  if (isPermissionDenied(e)) {
    const out = payloadPath();
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, `${JSON.stringify({ llm_usage: rows }, null, 2)}\n`);
    console.error(`foundry-ingest-llm: refused by the database — ${(e as Error).message}`);
    console.error(`foundry-ingest-llm: ${rows.length} row(s) parked at ${out}`);
    process.exit(EXIT_PARKED);
  }
  console.error("foundry-ingest-llm: failed —", (e as Error).message);
  process.exitCode = 1;
} finally {
  await pool.end();
}
