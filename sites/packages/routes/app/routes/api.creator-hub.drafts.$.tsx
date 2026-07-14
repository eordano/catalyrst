import { isSafeDraftId, type PutDraftInput, type PutResult } from "@data/lib/catalyst/creator-hub/scene-drafts";
import {
  DraftStoreError,
  getDraft,
  listDrafts,
  putDraft,
  verifyAuthChainRequest,
} from "@data/lib/catalyst/creator-hub/scene-drafts.server";

type RouteArgs = {
  request: Request;
  params: Record<string, string | undefined>;
};

function json(
  status: number,
  body: unknown,
  extraHeaders: Record<string, string> = {},
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
      "cache-control": "no-store",
      ...extraHeaders,
    },
  });
}

function draftId(params: Record<string, string | undefined>): string {
  return (params["*"] ?? "").trim();
}

type ParsedPut =
  | { ok: true; value: PutDraftInput }
  | { ok: false; error: string };

function parsePutBody(body: unknown): ParsedPut {
  if (!body || typeof body !== "object") {
    return { ok: false, error: "body must be a JSON object" };
  }
  const b = body as Record<string, unknown>;
  const { baseVersion } = b;
  if (
    typeof baseVersion !== "number" ||
    !Number.isInteger(baseVersion) ||
    baseVersion < 0
  ) {
    return { ok: false, error: "baseVersion must be a non-negative integer" };
  }
  if (typeof b.hash !== "string" || b.hash.length === 0 || b.hash.length > 512) {
    return { ok: false, error: "hash must be a non-empty string" };
  }
  if (!("blob" in b)) {
    return { ok: false, error: "blob is required" };
  }
  let title: string | undefined;
  if (b.title !== undefined) {
    if (typeof b.title !== "string" || b.title.length > 512) {
      return { ok: false, error: "title must be a string" };
    }
    title = b.title;
  }
  return { ok: true, value: { baseVersion, hash: b.hash, blob: b.blob, title } };
}

export async function loader({ request, params }: RouteArgs) {
  const auth = await verifyAuthChainRequest(request);
  if (!auth.ok) return json(auth.status, { error: auth.error });

  const id = draftId(params);
  if (!id) {
    return json(200, { drafts: await listDrafts(auth.wallet) });
  }
  if (!isSafeDraftId(id)) return json(400, { error: "invalid draft id" });

  const draft = await getDraft(auth.wallet, id);
  if (!draft) return json(404, { error: "draft not found" });
  return json(200, draft);
}

export async function action({ request, params }: RouteArgs) {
  if (request.method.toUpperCase() !== "PUT") {
    return json(405, { error: "method not allowed" }, { Allow: "PUT" });
  }

  const auth = await verifyAuthChainRequest(request);
  if (!auth.ok) return json(auth.status, { error: auth.error });

  const id = draftId(params);
  if (!id) return json(400, { error: "draft id is required" });
  if (!isSafeDraftId(id)) return json(400, { error: "invalid draft id" });

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return json(400, { error: "invalid JSON body" });
  }
  const parsed = parsePutBody(body);
  if (!parsed.ok) return json(400, { error: parsed.error });

  let result: PutResult;
  try {
    result = await putDraft(auth.wallet, id, parsed.value);
  } catch (err) {
    if (err instanceof DraftStoreError) {
      return json(400, { error: err.message });
    }
    throw err;
  }

  return json(result.ok ? 200 : 409, result);
}
