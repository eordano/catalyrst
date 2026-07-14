import { existsSync } from "node:fs";
import { request } from "node:http";

import catalog from "../../fixtures/sdk-skills-catalog.json";

// The copilot is a service of its own — an `opencode serve` instance inside a
// network-isolated sandbox, reached through the unix socket its wrapper bridges
// out and, publicly, through its own password-gated vhost. This module is the
// honest seam between the two: the site probes the service and reports what it
// found, and never claims a capability it did not just observe.
//
// There is no chat UI here and there will not be one. Conversation happens in
// opencode's own interface; this page links to it, reports whether it answered,
// and shows what it has spent.

export interface CopilotStatus {
  online: boolean;
  // Whether the service looks provisioned at all, so an offline page can tell
  // "the copilot is not reachable right now" (deployed) apart from "the copilot
  // was never deployed here" (no socket on disk, no loopback URL configured).
  deployed: boolean;
  probedAt: string;
  version?: string;
  // A reachable service can still be mute: opencode answers its socket while
  // every conversation sits in connect-retry against the model gateway. This is
  // that observation, read from /session/status. null = status read, no session
  // stuck; undefined = the status route itself did not answer.
  gatewayStuck?: { attempts: number; message: string } | null;
}

export type SkillSource = "sdk-skills" | "pre-prod" | "command";

export interface SkillCatalogEntry {
  name: string;
  description: string;
  dir: string;
  firstCommit: string | null;
  lastCommit: string | null;
  source: SkillSource;
}

const PROBE_TIMEOUT_MS = 1_500;
const MEMO_MS = 30_000;

const DEFAULT_SOCKET = "/srv/foundry-copilot/run/opencode.sock";

function internalUrl(): string {
  return (
    process.env.FOUNDRY_COPILOT_INTERNAL_URL?.trim() || "http://127.0.0.1:14096"
  );
}

/** Empty when the deployment exposes a loopback port instead of a socket. */
function socketPath(): string {
  const explicit = process.env.FOUNDRY_COPILOT_SOCKET?.trim();
  if (explicit) return explicit;
  return process.env.FOUNDRY_COPILOT_INTERNAL_URL?.trim() ? "" : DEFAULT_SOCKET;
}

function getOverSocket(
  path: string,
  route: string,
  headers: Record<string, string> = {},
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const req = request(
      {
        socketPath: path,
        path: route,
        method: "GET",
        headers: { accept: "application/json", ...authHeader(), ...headers },
        timeout: PROBE_TIMEOUT_MS,
      },
      (res) => {
        let body = "";
        res.setEncoding("utf8");
        res.on("data", (c) => (body += c));
        res.on("end", () => {
          if (res.statusCode !== 200) {
            reject(new Error(`HTTP ${res.statusCode}`));
            return;
          }
          try {
            resolve(JSON.parse(body));
          } catch (e) {
            reject(e);
          }
        });
      },
    );
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
    req.end();
  });
}

function postOverSocket(
  path: string,
  route: string,
  body: unknown,
  headers: Record<string, string> = {},
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const payload = JSON.stringify(body);
    const req = request(
      {
        socketPath: path,
        path: route,
        method: "POST",
        headers: {
          accept: "application/json",
          "content-type": "application/json",
          "content-length": Buffer.byteLength(payload),
          ...authHeader(),
          ...headers,
        },
        timeout: PROBE_TIMEOUT_MS,
      },
      (res) => {
        let text = "";
        res.setEncoding("utf8");
        res.on("data", (c) => (text += c));
        res.on("end", () => {
          if (res.statusCode !== 200) {
            reject(new Error(`HTTP ${res.statusCode}`));
            return;
          }
          try {
            resolve(JSON.parse(text));
          } catch (e) {
            reject(e);
          }
        });
      },
    );
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", reject);
    req.end(payload);
  });
}

/** Mint one workspace session for the door; returns its id. */
export async function createCopilotDoorSession(title: string): Promise<string> {
  const headers = { "x-opencode-directory": copilotRoot() };
  const socket = socketPath();
  let body: unknown;
  if (socket) {
    body = await postOverSocket(socket, "/session", { title }, headers);
  } else {
    const res = await fetch(`${internalUrl()}/session`, {
      method: "POST",
      headers: { ...authHeader(), ...headers, "content-type": "application/json" },
      body: JSON.stringify({ title }),
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    body = await res.json();
  }
  const id = (body as { id?: unknown })?.id;
  if (typeof id !== "string" || !isCopilotSessionId(id)) {
    throw new Error("session create returned no usable id");
  }
  return id;
}

// null when no public URL is configured — the copilot backend can be up on its
// socket while its web interface is not yet published (its host needs an edge
// vhost). The page then reports "online — interface pending" rather than linking
// to a host that does not answer.
export function copilotPublicUrl(): string | null {
  return process.env.FOUNDRY_COPILOT_PUBLIC_URL?.trim() || null;
}

/**
 * The web UI's address for one session: /server/<base64 of the server URL,
 * unpadded>/session/<id>. A bare visit to the UI root lands on a per-browser
 * recents list and a folder picker that cannot see the workspace, so the door
 * mints a workspace session server-side and links straight here instead.
 */
export function copilotSessionUrl(sessionId: string): string | null {
  const base = copilotPublicUrl();
  if (!base) return null;
  const b64 = Buffer.from(base.replace(/\/$/, "")).toString("base64").replace(/=+$/, "");
  return `${base.replace(/\/$/, "")}/server/${b64}/session/${sessionId}`;
}

/** The copilot workspace directory — the one place this env var is read. */
export function copilotRoot(): string {
  return process.env.FOUNDRY_COPILOT_DIRECTORY?.trim() || "/srv/foundry-copilot/workspace";
}

export const DOOR_TITLE_SUFFIX = " — from the site";

export interface DoorFunnel {
  minted: number;
  replied: number;
  lastMintedAt: string | null;
}

interface SessionRow {
  id?: unknown;
  title?: unknown;
  time?: { created?: unknown };
}

/**
 * The join that makes the funnel: door sessions come from opencode's own
 * session list (a stranded session never reaches llm_usage, which is exactly
 * the population worth counting), replies from the ingested usage rows.
 * Pure so the arithmetic is testable without either backend.
 */
export function doorFunnelFrom(
  sessions: SessionRow[],
  repliedIds: Set<string>,
  sinceMs: number,
): DoorFunnel {
  const door = sessions.filter(
    (s) =>
      typeof s.id === "string" &&
      typeof s.title === "string" &&
      s.title.endsWith(DOOR_TITLE_SUFFIX) &&
      typeof s.time?.created === "number" &&
      s.time.created >= sinceMs,
  );
  const replied = door.filter((s) => repliedIds.has(s.id as string)).length;
  const newest = door.reduce<number>(
    (max, s) => Math.max(max, s.time?.created as number),
    0,
  );
  return {
    minted: door.length,
    replied,
    lastMintedAt: newest > 0 ? new Date(newest).toISOString() : null,
  };
}

const DOOR_FUNNEL_DAYS = 7;

/** Door sessions minted vs answered over the last week; null when unreadable. */
export async function doorFunnel(db: {
  query: (sql: string, params: unknown[]) => Promise<{ rows: { session_id: string }[] }>;
}): Promise<DoorFunnel | null> {
  try {
    const route = `/session?directory=${encodeURIComponent(copilotRoot())}`;
    const headers = { "x-opencode-directory": copilotRoot() };
    const socket = socketPath();
    let body: unknown;
    if (socket) {
      body = await getOverSocket(socket, route, headers);
    } else {
      const res = await fetch(`${internalUrl()}${route}`, { headers: { ...authHeader(), ...headers } });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      body = await res.json();
    }
    if (!Array.isArray(body)) return null;
    const sinceMs = Date.now() - DOOR_FUNNEL_DAYS * 24 * 3600 * 1000;
    const ids = body
      .map((s: SessionRow) => (typeof s.id === "string" ? s.id : null))
      .filter((v): v is string => v !== null);
    const replied = ids.length
      ? await db.query(
          "SELECT DISTINCT session_id FROM foundry.llm_usage WHERE session_id = ANY($1)",
          [ids],
        )
      : { rows: [] };
    return doorFunnelFrom(body as SessionRow[], new Set(replied.rows.map((r) => r.session_id)), sinceMs);
  } catch {
    return null;
  }
}

export interface DoorProbeStep {
  name: string;
  ok: boolean;
  detail: string;
}

export interface DoorProbeStatus {
  ok: boolean;
  at: string;
  steps: DoorProbeStep[];
}

/** Parse the probe's verdict file; null keeps "not configured" honest. */
export function parseDoorProbeStatus(raw: string): DoorProbeStatus | null {
  try {
    const d = JSON.parse(raw) as DoorProbeStatus;
    if (typeof d.ok !== "boolean" || typeof d.at !== "string" || !Array.isArray(d.steps)) return null;
    return {
      ok: d.ok,
      at: d.at,
      steps: d.steps
        .filter((s) => typeof s?.name === "string")
        .map((s) => ({ name: s.name, ok: Boolean(s.ok), detail: String(s.detail ?? "") })),
    };
  } catch {
    return null;
  }
}

/** The hourly cold walk's last verdict, from FOUNDRY_DOOR_PROBE_STATUS. */
export async function readDoorProbeStatus(): Promise<DoorProbeStatus | null> {
  const path = process.env.FOUNDRY_DOOR_PROBE_STATUS?.trim();
  if (!path) return null;
  try {
    const { readFile } = await import("node:fs/promises");
    return parseDoorProbeStatus(await readFile(path, "utf8"));
  } catch {
    return null;
  }
}

function authHeader(): Record<string, string> {
  const password = process.env.FOUNDRY_COPILOT_PASSWORD?.trim();
  if (!password) return {};
  const user = process.env.FOUNDRY_COPILOT_USERNAME?.trim() || "opencode";
  return {
    authorization: `Basic ${Buffer.from(`${user}:${password}`).toString("base64")}`,
  };
}

const SESSION_ID_RE = /^ses_[A-Za-z0-9]{6,64}$/;

/** The shape opencode session ids actually take — the publish form's gate. */
export function isCopilotSessionId(value: string): boolean {
  return SESSION_ID_RE.test(value);
}

/**
 * One session's full message list, over the same socket the status probe uses.
 * opencode scopes every request to the directory named in this header; without
 * it the call lands on an empty "global" project and sees nothing.
 */
export async function copilotSessionMessages(sessionId: string): Promise<unknown> {
  if (!isCopilotSessionId(sessionId)) {
    throw new Error(`not a copilot session id: ${sessionId}`);
  }
  const directory = copilotRoot();
  const route = `/session/${sessionId}/message`;
  const headers = { "x-opencode-directory": directory };
  const socket = socketPath();
  if (socket) return getOverSocket(socket, route, headers);
  const res = await fetch(`${internalUrl()}${route}`, {
    headers: { ...authHeader(), ...headers },
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/**
 * /session/status maps session id -> state; a state of `{type: "retry"}`
 * carries the attempt count and the provider's own error sentence. Two attempts
 * is the line between a transient blip and a gateway that is not answering; the
 * worst stuck session is the one reported. Exposed pure for tests.
 */
export function stuckFromSessionStatus(
  body: unknown,
): { attempts: number; message: string } | null {
  if (!body || typeof body !== "object") return null;
  let worst: { attempts: number; message: string } | null = null;
  for (const state of Object.values(body as Record<string, unknown>)) {
    if (!state || typeof state !== "object") continue;
    const s = state as { type?: unknown; attempt?: unknown; message?: unknown };
    if (s.type !== "retry" || typeof s.attempt !== "number") continue;
    if (s.attempt < 2) continue;
    if (!worst || s.attempt > worst.attempts) {
      worst = {
        attempts: s.attempt,
        message: typeof s.message === "string" ? s.message : "",
      };
    }
  }
  return worst;
}

let memo: { at: number; status: CopilotStatus } | null = null;

/**
 * One `GET /global/health`, over the sandbox's unix socket (or a loopback URL
 * where one is configured).
 *
 * Every failure mode — unreachable, unauthorised, slow, malformed — is the same
 * answer to the only question the page asks: is it there right now. The result
 * is memoised briefly so a page with several loaders probes once, and never
 * cached long enough to keep claiming a service that has since died.
 */
export async function probeCopilot(): Promise<CopilotStatus> {
  const now = Date.now();
  if (memo && now - memo.at < MEMO_MS) return memo.status;

  const probedAt = new Date(now).toISOString();
  const socket = socketPath();
  // A socket that does not exist on disk means the service was never provisioned
  // here; a loopback-URL deployment has no such file, so it is assumed deployed.
  const deployed = socket ? existsSync(socket) : true;
  let status: CopilotStatus = { online: false, deployed, probedAt };

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
  try {
    const get = (route: string): Promise<unknown> =>
      socket
        ? getOverSocket(socket, route)
        : fetch(`${internalUrl()}${route}`, {
            headers: authHeader(),
            signal: controller.signal,
          }).then((res) =>
            res.ok ? res.json() : Promise.reject(new Error(`HTTP ${res.status}`)),
          );
    const body = (await get("/global/health")) as { version?: unknown } | null;
    const version = typeof body?.version === "string" ? body.version : undefined;
    status = version
      ? { online: true, deployed: true, probedAt, version }
      : { online: true, deployed: true, probedAt };
    try {
      status.gatewayStuck = stuckFromSessionStatus(await get("/session/status"));
    } catch {
      // Health answered but status did not: online stands, the gateway
      // observation stays undefined rather than pretending either way.
    }
  } catch {
    // Offline is a fact about this server right now, not an error to propagate.
  } finally {
    clearTimeout(timer);
  }

  memo = { at: now, status };
  return status;
}

/** Test seam: the probe memo outlives a single request by design. */
export function resetCopilotProbe(): void {
  memo = null;
}

const SKILLS: SkillCatalogEntry[] = (catalog.skills as SkillCatalogEntry[]).map((s) => ({
  ...s,
}));

/** The generated snapshot, committed so the page does not need the mirrors. */
export function loadSkillsCatalog(): SkillCatalogEntry[] {
  return SKILLS;
}

export function skillsCatalogSource(): { readAt: string; note: string } {
  const from = catalog.generatedFrom as { readAt: string; note: string };
  return { readAt: from.readAt, note: from.note };
}
