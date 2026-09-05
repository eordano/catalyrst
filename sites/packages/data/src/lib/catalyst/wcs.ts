import { z } from "zod";

import {
  ManagedWorldSchema,
  WalletStatsSchema,
  type ManagedWorld,
} from "./creator-hub/manage-worlds";

export { ManagedWorldSchema, WalletStatsSchema };
export type { ManagedWorld };
export type { WalletStats } from "./creator-hub/manage-worlds";

const DEFAULT_WCS = "https://worlds-content-server.decentraland.org";

/**
 * worlds-content-server is a different host from the catalyst.
 *
 * Do NOT use `worldsBase()` from `./client` for any of these paths: it rewrites
 * `catalystBase()`'s hostname to `worlds.<domain>` (i.e. `worlds.example.com`),
 * which 404s every path used in this module. `wcs.test.ts` asserts that
 * `wcsBase()` never resolves to a `worlds.` subdomain of `catalyst.example.com`.
 */
export function wcsBase(override?: string): string {
  return (
    override ??
    (typeof process !== "undefined"
      ? process.env?.WORLDS_CONTENT_SERVER_URL
      : undefined) ??
    DEFAULT_WCS
  ).replace(/\/$/, "");
}

export const WCS_WORLDS_PATH = "/worlds";
export const WCS_LIVE_DATA_PATH = "/live-data";
export const WCS_STATUS_PATH = "/status";
export function wcsWalletStatsPath(address: string): string {
  return `/wallet/${encodeURIComponent(address.trim().toLowerCase())}/stats`;
}

const strOrNull = z
  .string()
  .nullish()
  .transform((v) => v ?? null);
/**
 * A count the upstream must actually supply.
 *
 * Deliberately NOT `.nullish().transform((v) => v ?? 0)`. That spelling makes
 * `safeParse` incapable of failing, so a malformed or error body parses into a
 * valid-looking zero and reaches the UI as a `live` Datum -- the exact laundering
 * `Datum<T>` exists to prevent, performed one layer above where the type system
 * can see it. A missing count is a broken response, not a measurement of zero:
 * let the parse fail so the caller renders `unavailable`.
 */
const requiredCount = z.number();

/**
 * The wire row from `GET {wcs}/worlds`. It is snake_case; `ManagedWorld` is
 * camelCase. `ManagedWorldSchema` would `safeParse` this row *successfully*
 * while silently dropping `last_deployed_at`, `deployed_scenes` and
 * `blocked_since` (they are all `nullish -> null/0`), which would render a
 * deployed world as "never deployed". Parse the wire shape here and adapt.
 */
export const WcsWorldRowSchema = z.object({
  name: z.string(),
  owner: strOrNull,
  title: strOrNull,
  description: strOrNull,
  content_rating: strOrNull,
  spawn_coordinates: strOrNull,
  last_deployed_at: strOrNull,
  blocked_since: strOrNull,
  deployed_scenes: requiredCount,
  thumbnail_hash: strOrNull,
});
export type WcsWorldRow = z.infer<typeof WcsWorldRowSchema>;

export const WorldsListEnvelopeSchema = z.object({
  total: requiredCount,
  worlds: z.array(z.unknown()),
});

/** The content URL worlds-content-server serves a thumbnail hash from. */
export function wcsContentUrl(hash: string | null, base = wcsBase()): string | null {
  return hash ? `${base}/contents/${hash}` : null;
}

export function toManagedWorld(row: WcsWorldRow, base = wcsBase()): ManagedWorld {
  return {
    name: row.name,
    owner: row.owner,
    title: row.title,
    description: row.description,
    contentRating: row.content_rating,
    spawnCoordinates: row.spawn_coordinates,
    lastDeployedAt: row.last_deployed_at,
    blockedSince: row.blocked_since,
    deployedScenes: row.deployed_scenes,
    thumbnail: wcsContentUrl(row.thumbnail_hash, base),
    role: "owner",
  };
}

/**
 * Per-row `safeParse` + filtering, matching `places/presence.ts`: schema drift
 * on one row degrades that row, it never throws away the response.
 */
export function parseWcsWorlds(raw: unknown, base = wcsBase()): ManagedWorld[] {
  const env = WorldsListEnvelopeSchema.safeParse(raw);
  const rows = env.success ? env.data.worlds : [];
  const out: ManagedWorld[] = [];
  for (const r of rows) {
    const parsed = WcsWorldRowSchema.safeParse(r);
    if (parsed.success) out.push(toManagedWorld(parsed.data, base));
  }
  return out;
}

export function parseWcsWorldsTotal(raw: unknown): number {
  const env = WorldsListEnvelopeSchema.safeParse(raw);
  return env.success ? env.data.total : 0;
}

/** `GET {wcs}/live-data` -- the worlds server's own instant comms figure. */
export const LiveDataSchema = z.object({
  data: z.object({
    totalUsers: requiredCount,
    perWorld: z.array(
      z.object({
        worldName: z.string(),
        users: requiredCount,
      }),
    ),
  }),
  lastUpdated: strOrNull,
});
export type LiveData = z.infer<typeof LiveDataSchema>;

/** `GET {wcs}/status` -- platform totals. */
export const PlatformStatusSchema = z.object({
  content: z.object({
    commitHash: strOrNull,
    worldsCount: z.object({ ens: requiredCount, dcl: requiredCount }),
  }),
  comms: z.object({
    adapterType: strOrNull,
    rooms: requiredCount,
    users: requiredCount,
  }),
});
export type PlatformStatus = z.infer<typeof PlatformStatusSchema>;

/**
 * `usedSpace` / `maxAllowedSpace` / `dclNames[].size` are decimal byte strings
 * that can exceed `Number.MAX_SAFE_INTEGER`. Parse with BigInt; never `Number()`.
 * Returns null for anything that is not a decimal integer string.
 */
export function bytesFromString(raw: string | null | undefined): bigint | null {
  const v = (raw ?? "").trim();
  if (!/^\d+$/.test(v)) return null;
  try {
    return BigInt(v);
  } catch {
    return null;
  }
}

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** Formats a BigInt byte count. `null` in, `null` out -- never "0 B" for absent. */
export function formatBytes(bytes: bigint | null): string | null {
  if (bytes === null) return null;
  if (bytes < 1024n) return `${bytes.toString()} B`;
  let value = Number(bytes);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)} ${UNITS[unit]}`;
}

export function findWorldSize(
  stats: { dclNames: { name: string; size: string }[]; ensNames: { name: string; size: string }[] },
  world: string,
): bigint | null {
  const key = world.trim().toLowerCase();
  const hit =
    stats.dclNames.find((n) => n.name.trim().toLowerCase() === key) ??
    stats.ensNames.find((n) => n.name.trim().toLowerCase() === key);
  return hit ? bytesFromString(hit.size) : null;
}

export function liveUsersFor(live: LiveData, world: string): number | null {
  const key = world.trim().toLowerCase();
  const hit = live.data.perWorld.find(
    (w) => w.worldName.trim().toLowerCase() === key,
  );
  return hit ? hit.users : null;
}
