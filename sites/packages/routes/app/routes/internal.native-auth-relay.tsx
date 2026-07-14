
const TTL_MS = 5 * 60_000;
const MAX_ENTRIES = 500;

type Entry = {
  signer: string;
  signature: string;
  ephemeral: string;
  expiration: string;
  at: number;
};

const store = new Map<string, Entry>();

function sweep(): void {
  const now = Date.now();
  for (const [key, entry] of store) {
    if (now - entry.at > TTL_MS) store.delete(key);
  }
  while (store.size > MAX_ENTRIES) {
    const oldest = store.keys().next().value;
    if (oldest === undefined) break;
    store.delete(oldest);
  }
}

const ID_RE = /^[0-9a-fA-F-]{30,80}$/;
const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;
const SIG_RE = /^0x[0-9a-fA-F]{130,260}$/;
const MAX_EXPIRATION_MS = 31 * 24 * 60 * 60 * 1000;

function bad(status: number, error: string): Response {
  return new Response(JSON.stringify({ error }), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

export async function action({ request }: { request: Request }) {
  if (request.method !== "POST") return bad(405, "POST only");
  sweep();
  let body: Record<string, unknown>;
  try {
    body = (await request.json()) as Record<string, unknown>;
  } catch {
    return bad(400, "invalid JSON");
  }
  const { id, signer, signature, ephemeral, expiration } = body as Record<
    string,
    string
  >;
  if (typeof id !== "string" || !ID_RE.test(id)) return bad(400, "bad id");
  if (typeof signer !== "string" || !ADDR_RE.test(signer)) {
    return bad(400, "bad signer");
  }
  if (typeof ephemeral !== "string" || !ADDR_RE.test(ephemeral)) {
    return bad(400, "bad ephemeral");
  }
  if (typeof signature !== "string" || !SIG_RE.test(signature)) {
    return bad(400, "bad signature");
  }
  const exp = Date.parse(typeof expiration === "string" ? expiration : "");
  if (
    !Number.isFinite(exp) ||
    exp <= Date.now() ||
    exp > Date.now() + MAX_EXPIRATION_MS
  ) {
    return bad(400, "bad expiration");
  }
  store.set(id, { signer, signature, ephemeral, expiration, at: Date.now() });
  return new Response(null, {
    status: 204,
    headers: { "cache-control": "no-store" },
  });
}

export async function loader({ request }: { request: Request }) {
  sweep();
  const id = new URL(request.url).searchParams.get("id") ?? "";
  if (!ID_RE.test(id)) return bad(400, "bad id");
  const entry = store.get(id);
  if (!entry) {
    return new Response(null, {
      status: 204,
      headers: { "cache-control": "no-store" },
    });
  }
  store.delete(id);
  const { at: _at, ...payload } = entry;
  return new Response(JSON.stringify(payload), {
    status: 200,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}
