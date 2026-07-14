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
}

export type SkillSource = "sdk-skills" | "pre-prod";

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

function healthOverSocket(path: string): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const req = request(
      {
        socketPath: path,
        path: "/global/health",
        method: "GET",
        headers: { accept: "application/json", ...authHeader() },
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

// null when no public URL is configured — the copilot backend can be up on its
// socket while its web interface is not yet published (its host needs an edge
// vhost). The page then reports "online — interface pending" rather than linking
// to a host that does not answer.
export function copilotPublicUrl(): string | null {
  return process.env.FOUNDRY_COPILOT_PUBLIC_URL?.trim() || null;
}

function authHeader(): Record<string, string> {
  const password = process.env.FOUNDRY_COPILOT_PASSWORD?.trim();
  if (!password) return {};
  const user = process.env.FOUNDRY_COPILOT_USERNAME?.trim() || "opencode";
  return {
    authorization: `Basic ${Buffer.from(`${user}:${password}`).toString("base64")}`,
  };
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
    const body = (socket
      ? await healthOverSocket(socket)
      : await fetch(`${internalUrl()}/global/health`, {
          headers: authHeader(),
          signal: controller.signal,
        }).then((res) => (res.ok ? res.json() : Promise.reject(new Error(`HTTP ${res.status}`))))) as
      | { version?: unknown }
      | null;
    const version = typeof body?.version === "string" ? body.version : undefined;
    status = version
      ? { online: true, deployed: true, probedAt, version }
      : { online: true, deployed: true, probedAt };
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
