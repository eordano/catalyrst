import {
  AUTH_CHAIN_HEADER_PREFIX,
  AUTH_METADATA_HEADER,
  AUTH_TIMESTAMP_HEADER,
} from "@ui/data/auth/signerCore";

import {
  governanceSubmitPath,
  isGovernanceProposalKind,
} from "@data/lib/catalyst/governance/submit-client";

type RouteArgs = {
  request: Request;
  params: Record<string, string | undefined>;
};

const MAX_BODY_BYTES = 256 * 1024;
const UPSTREAM_DAO_HOST_RE = /(^|\.)decentraland\.org$/i;

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

function unavailable(message: string): Response {
  return json(503, { ok: false, error: "not configured", message });
}

function submitBase(): string | null {
  const raw =
    typeof process !== "undefined"
      ? process.env?.GOVERNANCE_SUBMIT_URL?.trim()
      : undefined;
  return raw ? raw.replace(/\/$/, "") : null;
}

function forwardedAuthHeaders(request: Request): Headers | null {
  const out = new Headers();
  const timestamp = request.headers.get(AUTH_TIMESTAMP_HEADER);
  const chain0 = request.headers.get(`${AUTH_CHAIN_HEADER_PREFIX}0`);
  if (!timestamp || !chain0) return null;

  out.set(AUTH_TIMESTAMP_HEADER, timestamp);
  out.set(AUTH_METADATA_HEADER, request.headers.get(AUTH_METADATA_HEADER) ?? "");
  for (const [name, value] of request.headers) {
    if (name.toLowerCase().startsWith(AUTH_CHAIN_HEADER_PREFIX)) {
      out.set(name, value);
    }
  }
  out.set("accept", "application/json");
  out.set("content-type", "application/json");
  return out;
}

export async function action({ request, params }: RouteArgs) {
  if (request.method.toUpperCase() !== "POST") {
    return json(405, { ok: false, error: "method not allowed", message: "POST only" });
  }

  const kind = (params.kind ?? "").trim();
  if (!isGovernanceProposalKind(kind)) {
    return json(404, {
      ok: false,
      error: "unknown proposal kind",
      message: `unknown proposal kind: ${kind}`,
    });
  }

  const base = submitBase();
  if (!base) {
    return unavailable(
      "governance submit endpoint not configured: set GOVERNANCE_SUBMIT_URL to a catalyrst-governance instance that holds a Snapshot signing key",
    );
  }

  let target: URL;
  try {
    target = new URL(`${base}${governanceSubmitPath(kind)}`);
  } catch {
    return unavailable("governance submit endpoint not configured: GOVERNANCE_SUBMIT_URL is not a valid URL");
  }
  if (UPSTREAM_DAO_HOST_RE.test(target.hostname)) {
    return unavailable(
      `governance submit endpoint not configured: refusing to forward proposal writes to the live Decentraland DAO API at ${target.hostname}`,
    );
  }

  const headers = forwardedAuthHeaders(request);
  if (!headers) {
    return json(401, {
      ok: false,
      error: "unauthorized",
      message: "missing auth-chain headers",
    });
  }

  const body = await request.text();
  if (body.length > MAX_BODY_BYTES) {
    return json(413, { ok: false, error: "payload too large", message: "proposal body too large" });
  }

  let upstream: Response;
  try {
    upstream = await fetch(target, {
      method: "POST",
      headers,
      body,
      signal: request.signal,
    });
  } catch (err) {
    return json(502, {
      ok: false,
      error: "upstream unreachable",
      message: `governance submit upstream unreachable: ${(err as Error)?.message ?? "network error"}`,
    });
  }

  const text = await upstream.text();
  return new Response(text, {
    status: upstream.status,
    headers: {
      "content-type": upstream.headers.get("content-type") ?? "application/json",
      "cache-control": "no-store",
    },
  });
}
