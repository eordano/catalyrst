import { buildEphemeralMessage } from "@data/lib/auth/identity";
import {
  allowPairCreate,
  pairStore,
} from "@data/lib/auth/pair-store.server";

const ID_RE = /^[A-Za-z0-9_-]{20,64}$/;
const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;
const SIG_RE = /^0x[0-9a-fA-F]{130,260}$/;
const TOKEN_RE = /^[A-Za-z0-9_-]{20,128}$/;
const MAX_EXPIRATION_MS = 31 * 24 * 60 * 60 * 1000;

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

const PRIVATE_HOP =
  /^(127\.|10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.|::1$|f[cd])/i;

function clientIp(request: Request): string {
  const hops = (request.headers.get("x-forwarded-for") ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  for (let i = hops.length - 1; i >= 0; i--) {
    if (!PRIVATE_HOP.test(hops[i])) return hops[i];
  }
  return "local";
}

type PairBody = {
  kind?: string;
  ephemeral?: string;
  expiration?: string;
  id?: string;
  pollToken?: string;
  signer?: string;
  signature?: string;
};

export async function action({ request }: { request: Request }) {
  if (request.method !== "POST") return json(405, { error: "POST only" });
  let body: PairBody;
  try {
    body = (await request.json()) as PairBody;
  } catch {
    return json(400, { error: "invalid JSON" });
  }

  if (body.kind === "create") {
    const { ephemeral, expiration } = body;
    if (typeof ephemeral !== "string" || !ADDR_RE.test(ephemeral)) {
      return json(400, { error: "bad ephemeral" });
    }
    const exp = Date.parse(typeof expiration === "string" ? expiration : "");
    if (
      !Number.isFinite(exp) ||
      exp <= Date.now() ||
      exp > Date.now() + MAX_EXPIRATION_MS
    ) {
      return json(400, { error: "bad expiration" });
    }
    if (!allowPairCreate(clientIp(request))) {
      return json(429, { error: "too many sessions, try again later" });
    }
    const message = buildEphemeralMessage(
      ephemeral.toLowerCase(),
      new Date(exp),
    );
    const session = pairStore.create({
      ephemeral: ephemeral.toLowerCase(),
      expiration: new Date(exp).toISOString(),
      message,
    });
    if (!session) return json(429, { error: "too many active sign-ins" });
    return json(200, {
      id: session.id,
      pollToken: session.pollToken,
      expiresAt: new Date(session.expiresAt).toISOString(),
    });
  }

  if (body.kind === "complete") {
    const { id, signer, signature } = body;
    if (typeof id !== "string" || !ID_RE.test(id)) {
      return json(400, { error: "bad id" });
    }
    if (typeof signer !== "string" || !ADDR_RE.test(signer)) {
      return json(400, { error: "bad signer" });
    }
    if (typeof signature !== "string" || !SIG_RE.test(signature)) {
      return json(400, { error: "bad signature" });
    }
    const session = pairStore.get(id);
    if (!session) return json(404, { error: "unknown session" });
    let recovered: string;
    try {
      const { recoverMessageAddress } = await import("viem");
      recovered = await recoverMessageAddress({
        message: session.message,
        signature: signature as `0x${string}`,
      });
    } catch {
      return json(400, { error: "unrecoverable signature" });
    }
    if (recovered.toLowerCase() !== signer.toLowerCase()) {
      return json(400, { error: "signature does not match signer" });
    }
    const result = pairStore.complete(id, signer.toLowerCase(), signature);
    if (result === "missing") return json(404, { error: "unknown session" });
    if (result === "expired") return json(410, { error: "session expired" });
    if (result === "already") return json(409, { error: "already completed" });
    return json(200, { ok: true });
  }

  if (body.kind === "poll" || body.kind === "cancel") {
    const { id, pollToken } = body;
    if (typeof id !== "string" || !ID_RE.test(id)) {
      return json(400, { error: "bad id" });
    }
    if (typeof pollToken !== "string" || !TOKEN_RE.test(pollToken)) {
      return json(400, { error: "bad pollToken" });
    }
    if (body.kind === "cancel") {
      pairStore.cancel(id, pollToken);
      return new Response(null, {
        status: 204,
        headers: { "cache-control": "no-store" },
      });
    }
    const result = pairStore.poll(id, pollToken);
    if (result.state === "missing") return json(404, { error: "unknown session" });
    if (result.state === "forbidden") return json(403, { error: "bad token" });
    if (result.state === "expired") return json(410, { state: "expired" });
    return json(200, result);
  }

  return json(400, { error: "unknown kind" });
}
