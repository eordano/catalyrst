import { z } from "zod";

import { catalystBase, getJSON, CatalystError } from "../client";
import type { GetOptions } from "../client";
import { signedFetch } from "../../auth/signer";
import type { AuthIdentity, SignedFetchMetadata } from "../../auth/types";
import {
  KeyListResponseSchema,
  StorageValueRowSchema,
  UsageResponseSchema,
  ValuesListResponseSchema,
} from "../generated-schemas/world-storage";

export const WORLD_STORAGE_PREFIX = "/world-storage";

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

export function formatSize(size: number): string {
  if (!Number.isFinite(size) || size < 0) return "0.00 B";
  if (size < KB) return `${size.toFixed(2)} B`;
  if (size < MB) return `${(size / KB).toFixed(2)} KB`;
  if (size < GB) return `${(size / MB).toFixed(2)} MB`;
  return `${(size / GB).toFixed(2)} GB`;
}

export function normalizeAddress(addr: string | null | undefined): string {
  return (addr ?? "").trim().toLowerCase();
}

/*
 * Validation truth for every read below is the generated world-storage schema
 * (the ts-rs image of catalyrst-world-storage's DTOs). Usage bytes are plain
 * numbers, never strings and never absent; /env and /players answer
 * KeyListResponse with `data: string[]`, not per-row `{key}` objects.
 */
export type Usage = z.infer<typeof UsageResponseSchema>;

/** Throws when the payload is not a UsageResponse; the loader reports the
 *  quota as unread instead of rendering a zero nobody measured. */
export async function fetchUsage(
  endpoint: "world" | "env" | { player: string },
  opts: GetOptions = {},
): Promise<Usage> {
  const path =
    typeof endpoint === "string"
      ? `${WORLD_STORAGE_PREFIX}/usage/${endpoint}`
      : `${WORLD_STORAGE_PREFIX}/usage/players/${encodeURIComponent(
          normalizeAddress(endpoint.player),
        )}`;
  const raw = await getJSON<unknown>(path, opts);
  return UsageResponseSchema.parse(raw);
}

export const StorageValueSchema = StorageValueRowSchema;
export type StorageValue = z.infer<typeof StorageValueSchema>;

/**
 * Throws when the envelope does not parse. An empty list here means the store
 * is empty; returning one for an unreadable response would render "no values"
 * over a broken endpoint.
 */
export async function fetchValues(
  opts: GetOptions = {},
): Promise<StorageValue[]> {
  const path = `${WORLD_STORAGE_PREFIX}/values`;
  const raw = await getJSON<unknown>(path, opts);
  const env = ValuesListResponseSchema.safeParse(raw);
  if (!env.success) {
    throw new CatalystError("world-storage values response had no data list", path);
  }
  return env.data.data;
}

/** The view keeps the `{key}` row shape consumers render; the wire sends bare
 *  strings, lifted post-parse. */
export type EnvKey = { key: string };

export async function fetchEnvKeys(opts: GetOptions = {}): Promise<EnvKey[]> {
  const path = `${WORLD_STORAGE_PREFIX}/env`;
  const raw = await getJSON<unknown>(path, opts);
  const env = KeyListResponseSchema.safeParse(raw);
  if (!env.success) {
    throw new CatalystError("world-storage env response had no data list", path);
  }
  return env.data.data.map((key) => ({ key }));
}

export async function fetchPlayers(opts: GetOptions = {}): Promise<string[]> {
  const path = `${WORLD_STORAGE_PREFIX}/players`;
  const raw = await getJSON<unknown>(path, opts);
  const env = KeyListResponseSchema.safeParse(raw);
  if (!env.success) {
    throw new CatalystError("world-storage players response had no data list", path);
  }
  return env.data.data;
}

export type StorageWorld = {
  name: string;
  role: "owner" | "collaborator";
  scenes: number;
  usedBytes: number;
  maxTotalSizeBytes: number;
};

export type StorageLand = {
  id: string;
  name: string;
  role: string;
};

export type WalletStats = {
  wallet: string;
  usedSpace: number;
  maxAllowedSpace: number;
  dclNames: { name: string; size: number }[];
  ensNames: { name: string; size: number }[];
};

export type StorageScope = {
  realm: string;
  position: string;
  scene: string;
};

export type PlayerEntry = {
  addresses: string[];
  profileNames: Record<string, string>;
};

export type WorldsStorageData = {
  address: string;
  stats: WalletStats | null;
  worlds: StorageWorld[];
  lands: StorageLand[];
  scope: StorageScope | null;
  values: StorageValue[];
  envKeys: EnvKey[];
  players: PlayerEntry;
  source: "live" | "empty";
  fallback: boolean;
};

export function findWorld(
  worlds: ReadonlyArray<StorageWorld>,
  name: string | null | undefined,
): StorageWorld | null {
  if (!name) return null;
  const n = name.trim().toLowerCase();
  return worlds.find((w) => w.name.toLowerCase() === n) ?? null;
}

export const STORAGE_STEPS = ["select", "scene", "edit", "clear"] as const;
export type StorageStep = (typeof STORAGE_STEPS)[number];

export function coerceStep(raw: string | null | undefined): StorageStep {
  return (STORAGE_STEPS as readonly string[]).includes(raw ?? "")
    ? (raw as StorageStep)
    : "select";
}


export type WriteScope = {
  realm: string;
  parcel: string;
};

export type WriteOptions = {
  identity: AuthIdentity;
  scope: WriteScope;
  base?: string;
  signal?: AbortSignal;
};

const CONFIRM_DELETE_ALL_HEADER = "x-confirm-delete-all";

function scopeMetadata(scope: WriteScope): SignedFetchMetadata {
  const meta: SignedFetchMetadata = { parcel: scope.parcel };
  if (scope.realm) {
    meta.realm = { serverName: scope.realm };
    meta.realmName = scope.realm;
  }
  return meta;
}

async function writeWorldStorage(
  method: "PUT" | "DELETE",
  path: string,
  body: { value: unknown } | undefined,
  opts: WriteOptions,
  extraHeaders?: Record<string, string>,
): Promise<void> {
  const url = `${catalystBase(opts.base)}${path}`;
  const headers: Record<string, string> = { ...(extraHeaders ?? {}) };
  if (body !== undefined) headers["content-type"] = "application/json";

  const init: RequestInit & { metadata?: SignedFetchMetadata } = {
    method,
    metadata: scopeMetadata(opts.scope),
    headers,
    signal: opts.signal,
  };
  if (body !== undefined) init.body = JSON.stringify(body);

  let res: Response;
  try {
    res = await signedFetch(opts.identity, url, init);
  } catch (err) {
    throw new CatalystError(
      `World storage write failed: ${(err as Error)?.message ?? "network error"}`,
      url,
    );
  }
  if (!res.ok) {
    throw new CatalystError(
      `World storage write returned ${res.status} ${res.statusText}`,
      url,
      res.status,
    );
  }
}

export async function saveValue(
  key: string,
  value: unknown,
  opts: WriteOptions,
): Promise<void> {
  await writeWorldStorage(
    "PUT",
    `${WORLD_STORAGE_PREFIX}/values/${encodeURIComponent(key)}`,
    { value },
    opts,
  );
}

export async function deleteValue(
  key: string,
  opts: WriteOptions,
): Promise<void> {
  await writeWorldStorage(
    "DELETE",
    `${WORLD_STORAGE_PREFIX}/values/${encodeURIComponent(key)}`,
    undefined,
    opts,
  );
}

export async function clearValues(opts: WriteOptions): Promise<void> {
  await writeWorldStorage(
    "DELETE",
    `${WORLD_STORAGE_PREFIX}/values`,
    undefined,
    opts,
    { [CONFIRM_DELETE_ALL_HEADER]: "true" },
  );
}

export async function saveEnvKey(
  key: string,
  value: string,
  opts: WriteOptions,
): Promise<void> {
  await writeWorldStorage(
    "PUT",
    `${WORLD_STORAGE_PREFIX}/env/${encodeURIComponent(key)}`,
    { value },
    opts,
  );
}

export async function deleteEnvKey(
  key: string,
  opts: WriteOptions,
): Promise<void> {
  await writeWorldStorage(
    "DELETE",
    `${WORLD_STORAGE_PREFIX}/env/${encodeURIComponent(key)}`,
    undefined,
    opts,
  );
}

export async function clearEnvKeys(opts: WriteOptions): Promise<void> {
  await writeWorldStorage(
    "DELETE",
    `${WORLD_STORAGE_PREFIX}/env`,
    undefined,
    opts,
    { [CONFIRM_DELETE_ALL_HEADER]: "true" },
  );
}
