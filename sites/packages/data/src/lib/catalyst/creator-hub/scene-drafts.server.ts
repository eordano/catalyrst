import { randomUUID } from "node:crypto";
import { promises as fs } from "node:fs";
import path from "node:path";

import {
  AUTH_CHAIN_HEADER_PREFIX,
  AUTH_METADATA_HEADER,
  AUTH_TIMESTAMP_HEADER,
  buildRequestPayload,
} from "../../auth/signer";
import type { AuthLink } from "../../auth/types";
import {
  isSafeDraftId,
  isSafeWallet,
  normalizeWallet,
  type Draft,
  type DraftMeta,
  type PutDraftInput,
  type PutResult,
} from "./scene-drafts";
import { ETH_ADDRESS_RE } from "../format/address";

export class DraftStoreError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "DraftStoreError";
  }
}

type StoredFile = { meta: DraftMeta; blob: unknown };

export function draftsRoot(): string {
  return (
    process.env.CH_DRAFTS_DIR || path.join(process.cwd(), "data", "ch-drafts")
  );
}

function walletDir(wallet: string): string {
  const w = normalizeWallet(wallet);
  if (!isSafeWallet(w)) throw new DraftStoreError("invalid wallet");
  return path.join(draftsRoot(), w);
}

function draftPath(wallet: string, id: string): string {
  if (!isSafeDraftId(id)) throw new DraftStoreError("invalid draft id");
  return path.join(walletDir(wallet), `${id}.json`);
}

async function readStored(
  wallet: string,
  id: string,
): Promise<StoredFile | null> {
  let raw: string;
  try {
    raw = await fs.readFile(draftPath(wallet, id), "utf8");
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return null;
    throw err;
  }
  try {
    const parsed = JSON.parse(raw) as StoredFile;
    if (
      !parsed ||
      typeof parsed !== "object" ||
      !parsed.meta ||
      typeof parsed.meta.version !== "number"
    ) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

export async function listDrafts(wallet: string): Promise<DraftMeta[]> {
  if (!isSafeWallet(wallet)) return [];
  let names: string[];
  try {
    names = await fs.readdir(walletDir(wallet));
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") return [];
    throw err;
  }
  const metas: DraftMeta[] = [];
  for (const name of names) {
    if (!name.endsWith(".json")) continue;
    const id = name.slice(0, -5);
    if (!isSafeDraftId(id)) continue;
    const stored = await readStored(wallet, id);
    if (stored?.meta) metas.push(stored.meta);
  }
  metas.sort((a, b) => b.updatedAt - a.updatedAt);
  return metas;
}

export async function getDraft(
  wallet: string,
  id: string,
): Promise<Draft | null> {
  if (!isSafeWallet(wallet) || !isSafeDraftId(id)) return null;
  const stored = await readStored(wallet, id);
  if (!stored) return null;
  return { ...stored.meta, blob: stored.blob };
}

export async function putDraft(
  wallet: string,
  id: string,
  input: PutDraftInput,
): Promise<PutResult> {
  if (!isSafeWallet(wallet)) throw new DraftStoreError("invalid wallet");
  if (!isSafeDraftId(id)) throw new DraftStoreError("invalid draft id");

  const existing = await readStored(wallet, id);
  if (existing && existing.meta.version !== input.baseVersion) {
    return {
      ok: false,
      conflict: true,
      server: { ...existing.meta, blob: existing.blob },
    };
  }

  const version = existing ? existing.meta.version + 1 : 1;
  const meta: DraftMeta = {
    id,
    hash: input.hash,
    version,
    updatedAt: Date.now(),
    title: input.title ?? existing?.meta.title ?? "",
  };
  const file: StoredFile = { meta, blob: input.blob };

  const dir = walletDir(wallet);
  await fs.mkdir(dir, { recursive: true });
  const finalPath = draftPath(wallet, id);
  const tmpPath = path.join(dir, `.${id}.${randomUUID()}.tmp`);
  await fs.writeFile(tmpPath, JSON.stringify(file), "utf8");
  await fs.rename(tmpPath, finalPath);

  return { ok: true, meta };
}

export type AuthResult =
  | { ok: true; wallet: string }
  | { ok: false; status: number; error: string };

const DEFAULT_MAX_AGE_MS = 10 * 60_000;

function parseEphemeral(
  message: string,
): { address: string; expiration: number } | null {
  const addr = /Ephemeral address:\s*(0x[0-9a-fA-F]{40})/i.exec(message);
  const exp = /Expiration:\s*(.+)\s*$/im.exec(message);
  if (!addr) return null;
  const expiration = exp ? Date.parse(exp[1].trim()) : NaN;
  return {
    address: addr[1].toLowerCase(),
    expiration: Number.isFinite(expiration) ? expiration : Infinity,
  };
}

export async function verifyAuthChainRequest(
  request: Request,
  opts: { now?: number; maxAgeMs?: number } = {},
): Promise<AuthResult> {
  const now = opts.now ?? Date.now();
  const maxAge = opts.maxAgeMs ?? DEFAULT_MAX_AGE_MS;

  const timestampRaw = request.headers.get(AUTH_TIMESTAMP_HEADER);
  const metadataStr = request.headers.get(AUTH_METADATA_HEADER) ?? "";
  if (!timestampRaw) {
    return { ok: false, status: 401, error: "missing identity timestamp" };
  }
  const timestamp = Number(timestampRaw);
  if (!Number.isFinite(timestamp)) {
    return { ok: false, status: 401, error: "invalid identity timestamp" };
  }
  if (Math.abs(now - timestamp) > maxAge) {
    return { ok: false, status: 401, error: "stale identity timestamp" };
  }

  const chain: AuthLink[] = [];
  for (let i = 0; ; i++) {
    const header = request.headers.get(`${AUTH_CHAIN_HEADER_PREFIX}${i}`);
    if (header === null) break;
    let link: AuthLink;
    try {
      link = JSON.parse(header) as AuthLink;
    } catch {
      return { ok: false, status: 401, error: `malformed auth link ${i}` };
    }
    if (
      !link ||
      typeof link.type !== "string" ||
      typeof link.payload !== "string" ||
      typeof link.signature !== "string"
    ) {
      return { ok: false, status: 401, error: `malformed auth link ${i}` };
    }
    chain.push(link);
    if (i > 8) return { ok: false, status: 401, error: "auth chain too long" };
  }
  if (chain.length < 2) {
    return { ok: false, status: 401, error: "empty auth chain" };
  }

  const signerLink = chain[0];
  if (signerLink.type !== "SIGNER" || !ETH_ADDRESS_RE.test(signerLink.payload)) {
    return { ok: false, status: 401, error: "missing SIGNER link" };
  }
  const owner = signerLink.payload.toLowerCase();

  const { recoverMessageAddress } = await import("viem");
  let authority = owner;
  let finalPayload: string | null = null;

  for (let i = 1; i < chain.length; i++) {
    const link = chain[i];
    let recovered: string;
    try {
      recovered = (
        await recoverMessageAddress({
          message: link.payload,
          signature: link.signature as `0x${string}`,
        })
      ).toLowerCase();
    } catch {
      return {
        ok: false,
        status: 401,
        error: `unrecoverable signature at link ${i}`,
      };
    }
    if (recovered !== authority) {
      return { ok: false, status: 401, error: `broken auth chain at link ${i}` };
    }

    const linkType: string = link.type;
    if (
      linkType === "ECDSA_EPHEMERAL" ||
      linkType === "ECDSA_PERSONAL_EPHEMERAL"
    ) {
      const eph = parseEphemeral(link.payload);
      if (!eph) {
        return { ok: false, status: 401, error: "malformed ephemeral payload" };
      }
      if (eph.expiration <= now) {
        return { ok: false, status: 401, error: "identity expired" };
      }
      authority = eph.address;
    } else if (
      linkType === "ECDSA_SIGNED_ENTITY" ||
      linkType === "ECDSA_PERSONAL_SIGNED_ENTITY"
    ) {
      finalPayload = link.payload;
    } else {
      return {
        ok: false,
        status: 401,
        error: `unsupported auth link type ${linkType}`,
      };
    }
  }

  if (finalPayload === null) {
    return { ok: false, status: 401, error: "missing signed request link" };
  }

  const rawMethod = request.method.toUpperCase();
  const method = rawMethod === "HEAD" ? "GET" : rawMethod;
  const path = new URL(request.url).pathname;
  const expected = buildRequestPayload(method, path, timestampRaw, metadataStr);
  if (finalPayload.toLowerCase() !== expected) {
    return {
      ok: false,
      status: 401,
      error: "request signature does not match request",
    };
  }

  return { ok: true, wallet: owner };
}
