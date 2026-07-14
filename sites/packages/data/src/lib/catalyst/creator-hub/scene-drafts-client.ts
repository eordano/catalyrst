// Client-side access to the server scene-draft store (api/creator-hub/drafts).
// Direct GET-then-PUT with the caller's signed identity: the shared sync
// engine's outbox is deliberately bypassed here because its conflict states
// have no resolve UI in the editor; last-write-wins against the server copy
// is the contract for editor saves. All functions are best-effort and return
// null / false instead of throwing when signed out or offline -- the local
// (FSA/IndexedDB) save path is the availability floor, the server copy is
// the durability floor.
import { getIdentity } from "../../auth/session";
import { signRequest } from "../../auth/signer";
import type { Draft } from "./scene-drafts";

export type ServerDraftBlob = {
  composite: string;
  title?: string;
  base?: string;
  template?: string;
  assets?: Record<string, string>;
};

export function parseServerDraftBlob(blob: unknown): ServerDraftBlob | null {
  if (!blob || typeof blob !== "object") return null;
  const b = blob as Record<string, unknown>;
  if (typeof b.composite !== "string" || !b.composite.trim()) return null;
  try {
    JSON.parse(b.composite);
  } catch {
    return null;
  }
  const out: ServerDraftBlob = { composite: b.composite };
  if (typeof b.title === "string") out.title = b.title;
  if (typeof b.base === "string") out.base = b.base;
  if (typeof b.template === "string") out.template = b.template;
  if (b.assets && typeof b.assets === "object" && !Array.isArray(b.assets)) {
    const assets: Record<string, string> = {};
    for (const [k, v] of Object.entries(b.assets as Record<string, unknown>)) {
      if (typeof v === "string") assets[k] = v;
    }
    if (Object.keys(assets).length > 0) out.assets = assets;
  }
  return out;
}

/** Newer-wins pick between a local and a server timestamp (0 = absent). */
export function serverCopyIsNewer(
  localUpdatedAt: number | null | undefined,
  serverUpdatedAt: number | null | undefined,
): boolean {
  return (serverUpdatedAt ?? 0) > (localUpdatedAt ?? 0);
}

export type ServerDraft = {
  id: string;
  version: number;
  updatedAt: number;
  title: string;
  blob: ServerDraftBlob;
};

export type ServerDraftMeta = {
  id: string;
  version: number;
  updatedAt: number;
  title: string;
};

/** djb2 over the composite text -- deterministic, so identical saves dedupe. */
export function hashComposite(text: string): string {
  let h = 5381;
  for (let i = 0; i < text.length; i++) {
    h = ((h << 5) + h + text.charCodeAt(i)) | 0;
  }
  return (h >>> 0).toString(16).padStart(8, "0") + "-" + (text.length >>> 0).toString(16);
}

function draftPath(id: string): string {
  return `/api/creator-hub/drafts/${encodeURIComponent(id)}`;
}

/** List the signed-in account's server drafts; null when signed out/offline. */
export async function listServerDrafts(): Promise<ServerDraftMeta[] | null> {
  const identity = getIdentity();
  if (!identity || typeof fetch === "undefined") return null;
  try {
    const path = "/api/creator-hub/drafts";
    const { headers } = await signRequest(identity, "GET", path);
    const res = await fetch(path, { headers });
    if (!res.ok) return null;
    const raw = (await res.json()) as { drafts?: unknown };
    if (!Array.isArray(raw?.drafts)) return null;
    const out: ServerDraftMeta[] = [];
    for (const d of raw.drafts as Array<Record<string, unknown>>) {
      if (!d || typeof d.id !== "string" || typeof d.version !== "number") continue;
      out.push({
        id: d.id,
        version: d.version,
        updatedAt: typeof d.updatedAt === "number" ? d.updatedAt : 0,
        title: typeof d.title === "string" ? d.title : "",
      });
    }
    return out;
  } catch {
    return null;
  }
}

export async function fetchServerDraft(id: string): Promise<ServerDraft | null> {
  const identity = getIdentity();
  if (!identity || typeof fetch === "undefined") return null;
  try {
    const path = draftPath(id);
    const { headers } = await signRequest(identity, "GET", path);
    const res = await fetch(path, { headers });
    if (!res.ok) return null;
    const raw = (await res.json()) as Draft;
    const blob = parseServerDraftBlob(raw?.blob);
    if (!blob || typeof raw.version !== "number") return null;
    return {
      id: raw.id,
      version: raw.version,
      updatedAt: typeof raw.updatedAt === "number" ? raw.updatedAt : 0,
      title: typeof raw.title === "string" ? raw.title : "",
      blob,
    };
  } catch {
    return null;
  }
}

/**
 * Push the current editor state as the server draft. Reads the server
 * version first so the PUT's baseVersion matches (the API is
 * compare-and-swap); one retry on 409 re-reads and overwrites -- for editor
 * saves the user's just-written copy wins.
 */
export async function pushServerDraft(
  id: string,
  blob: ServerDraftBlob,
): Promise<boolean> {
  const identity = getIdentity();
  if (!identity || typeof fetch === "undefined") return false;

  const putOnce = async (baseVersion: number): Promise<Response | null> => {
    try {
      const path = draftPath(id);
      const { headers } = await signRequest(identity, "PUT", path);
      return await fetch(path, {
        method: "PUT",
        headers: { ...headers, "content-type": "application/json" },
        body: JSON.stringify({
          baseVersion,
          hash: hashComposite(blob.composite),
          title: blob.title,
          blob,
        }),
      });
    } catch {
      return null;
    }
  };

  const current = await fetchServerDraft(id);
  let res = await putOnce(current?.version ?? 0);
  if (res?.status === 409) {
    const server = (await res.json().catch(() => null)) as
      | { server?: { version?: number } }
      | null;
    const v = server?.server?.version;
    if (typeof v === "number") res = await putOnce(v);
  }
  return res?.ok === true;
}
